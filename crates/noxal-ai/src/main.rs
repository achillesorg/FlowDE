//! noxal-ai daemon. Streaming chat over the control socket (`ai.chat` returns a
//! request id; tokens arrive as `ai.token` events, then `ai.done`), plus a
//! small D-Bus API (`org.flowde.Noxal1`). Tool calls go through the allowlist
//! and per-tool permission prompts (`ai.permission` events / answers).

use flow_common::daemon::now_s;
use flow_common::events::PowerSnapshot;
use flow_common::ipc::{self, EventBus, RpcError};
use noxal_ai::backend::*;
use noxal_ai::convo::*;
use noxal_ai::gate::*;
use noxal_ai::models::Registry;
use noxal_ai::prompt;
use noxal_ai::tools::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Config {
    gate: GateConfig,
    registry: Registry,
    /// Encrypt conversations at rest (passphrase supplied with `ai.unlock`).
    encrypt: bool,
    /// llama.cpp CLI: program and args (placeholders {model} {prompt} {max_tokens} {temp}).
    program: String,
    args: Vec<String>,
    /// Use the scripted mock backend (tests / demos).
    mock: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            gate: GateConfig::default(),
            registry: Registry::default(),
            encrypt: false,
            program: "llama-cli".into(),
            args: [
                "-m",
                "{model}",
                "-p",
                "{prompt}",
                "-n",
                "{max_tokens}",
                "--temp",
                "{temp}",
                "--no-display-prompt",
                "--simple-io",
                "-no-cnv",
            ]
            .map(String::from)
            .to_vec(),
            mock: false,
        }
    }
}

struct Shared {
    cfg: Config,
    store: Mutex<Option<Store>>,
    policy: Mutex<Policy>,
    approvals: Mutex<Approvals>,
    cancels: Mutex<HashMap<u32, Arc<AtomicBool>>>,
    next_req: Mutex<u32>,
    idle: Mutex<IdleUnloader>,
    bus: EventBus,
    backend: Mutex<Option<Arc<dyn Backend>>>,
}

fn dir() -> std::path::PathBuf {
    flow_common::paths::state_dir().join("noxal")
}

#[derive(Deserialize)]
struct ChatP {
    text: String,
    #[serde(default)]
    conversation: Option<String>,
}
#[derive(Deserialize)]
struct IdP {
    id: String,
}
#[derive(Deserialize)]
struct ReqP {
    request: u32,
}
#[derive(Deserialize)]
struct AnsP {
    id: u32,
    allow: bool,
    #[serde(default)]
    remember: bool,
}
#[derive(Deserialize)]
struct PolP {
    tool: String,
    permission: Permission,
}
#[derive(Deserialize)]
struct UnlockP {
    passphrase: String,
}

async fn power_snapshot() -> Option<PowerSnapshot> {
    flow_common::ipc::call_once("flow-power", "power.snapshot", Value::Null)
        .await
        .ok()
        .and_then(|v| serde_json::from_value(v).ok())
}

fn free_ram_mb() -> u64 {
    std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|s| mem_available_mb(&s))
        .unwrap_or(0)
}

fn make_backend(
    cfg: &Config,
    model: Option<&noxal_ai::models::ModelSpec>,
) -> Option<Arc<dyn Backend>> {
    if cfg.mock {
        return Some(Arc::new(MockBackend { script: None }));
    }
    let m = model?;
    Some(Arc::new(CliBackend {
        program: cfg.program.clone(),
        args: cfg.args.clone(),
        model_path: m.path.to_string_lossy().into_owned(),
    }))
}

/// Run tool calls found in the model output. Returns summaries for the user.
async fn handle_tools(sh: &Arc<Shared>, request: u32, calls: Vec<ToolCall>) {
    for call in calls {
        let perm = sh.policy.lock().unwrap().get(&call.name);
        match perm {
            Permission::Deny => sh.bus.emit(
                "ai.tool",
                json!({"request": request, "tool": call.name, "status": "denied"}),
            ),
            Permission::Ask => {
                if to_request(&call).is_err() {
                    sh.bus.emit(
                        "ai.tool",
                        json!({"request": request, "tool": call.name, "status": "rejected"}),
                    );
                    continue;
                }
                let id = sh.approvals.lock().unwrap().ask(call.clone());
                sh.bus.emit("ai.permission", json!({"id": id, "request": request, "tool": call.name, "arguments": call.arguments}));
            }
            Permission::Allow => run_tool(sh, request, &call).await,
        }
    }
}

async fn run_tool(sh: &Shared, request: u32, call: &ToolCall) {
    let status = match to_request(call) {
        Err(e) => json!({"status": "rejected", "error": format!("{e:?}")}),
        Ok((daemon, method, params)) => {
            match flow_common::ipc::call_once(daemon, method, params).await {
                Ok(v) => json!({"status": "ok", "result": v}),
                Err(e) => json!({"status": "error", "error": e.to_string()}),
            }
        }
    };
    sh.bus.emit(
        "ai.tool",
        json!({"request": request, "tool": call.name, "outcome": status}),
    );
}

async fn chat(sh: &Arc<Shared>, p: ChatP) -> ipc::RpcResult {
    if p.text.trim().is_empty() || p.text.len() > 8000 {
        return Err(RpcError::invalid("text must be 1..8000 bytes"));
    }
    let power = power_snapshot().await;
    let free = free_ram_mb();
    let model = sh
        .cfg
        .registry
        .pick(free, sh.cfg.gate.ram_headroom_mb)
        .cloned();
    if !sh.cfg.mock {
        let need = model.as_ref().map(|m| m.ram_mb).unwrap_or_else(|| {
            sh.cfg
                .registry
                .installed()
                .iter()
                .map(|m| m.ram_mb)
                .min()
                .unwrap_or(0)
        });
        if sh.cfg.registry.installed().is_empty() {
            return Err(RpcError::unavailable("no model installed: add a .gguf model to the registry (nothing is downloaded automatically)"));
        }
        check(&sh.cfg.gate, power.as_ref(), free, need)
            .map_err(|d| RpcError::unavailable(d.to_string()))?;
    }
    let backend = {
        let mut b = sh.backend.lock().unwrap();
        if b.is_none() {
            *b = make_backend(&sh.cfg, model.as_ref());
        }
        b.clone()
            .ok_or_else(|| RpcError::unavailable("no backend/model fits current memory"))?
    };
    if !backend.available() {
        return Err(RpcError::unavailable(format!(
            "{} backend not installed",
            backend.name()
        )));
    }
    let conv_id = p
        .conversation
        .clone()
        .unwrap_or_else(|| format!("c{}", now_s()));
    let mut conv = {
        let st = sh.store.lock().unwrap();
        let store = st
            .as_ref()
            .ok_or_else(|| RpcError::unavailable("conversation store locked: call ai.unlock"))?;
        store.load(&conv_id).unwrap_or_else(|_| Conversation {
            id: conv_id.clone(),
            title: p.text.chars().take(40).collect(),
            messages: vec![],
        })
    };
    conv.messages.push(Message {
        role: Role::User,
        text: p.text,
        time: now_s(),
    });
    let prompt = prompt::build(&conv.messages, 12_000);
    let request = {
        let mut n = sh.next_req.lock().unwrap();
        *n += 1;
        *n
    };
    let cancel = Arc::new(AtomicBool::new(false));
    sh.cancels.lock().unwrap().insert(request, cancel.clone());
    sh.idle.lock().unwrap().used(now_s());
    let sh2 = sh.clone();
    let rt = tokio::runtime::Handle::current();
    std::thread::spawn(move || {
        let mut full = String::new();
        let req = GenRequest {
            prompt,
            max_tokens: 512,
            temperature: 0.7,
        };
        let bus = sh2.bus.clone();
        let res = backend.generate(&req, &cancel, &mut |t| {
            full.push_str(t);
            bus.emit("ai.token", json!({"request": request, "text": t}));
        });
        match &res {
            Ok(()) => {}
            Err(BackendError::Cancelled) => bus.emit(
                "ai.error",
                json!({"request": request, "error": "cancelled"}),
            ),
            Err(e) => bus.emit(
                "ai.error",
                json!({"request": request, "error": format!("{e:?}")}),
            ),
        }
        let calls = parse_tool_calls(&full);
        let shown = strip_tool_calls(&full);
        conv.messages.push(Message {
            role: Role::Assistant,
            text: shown,
            time: now_s(),
        });
        if let Some(store) = sh2.store.lock().unwrap().as_ref() {
            let _ = store.save(&conv);
        }
        sh2.cancels.lock().unwrap().remove(&request);
        sh2.bus.emit(
            "ai.done",
            json!({"request": request, "conversation": conv.id, "ok": res.is_ok()}),
        );
        if res.is_ok() && !calls.is_empty() {
            rt.block_on(handle_tools(&sh2, request, calls));
        }
    });
    Ok(json!({"request": request, "conversation": conv_id}))
}

async fn dispatch(sh: &Arc<Shared>, m: &str, p: Value) -> ipc::RpcResult {
    match m {
        "ai.chat" => chat(sh, ipc::params(p)?).await,
        "ai.cancel" => {
            let p: ReqP = ipc::params(p)?;
            if let Some(f) = sh.cancels.lock().unwrap().get(&p.request) {
                f.store(true, Ordering::Relaxed);
            }
            Ok(Value::Bool(true))
        }
        "ai.status" => {
            let power = power_snapshot().await;
            let free = free_ram_mb();
            let gate = sh
                .cfg
                .registry
                .pick(free, sh.cfg.gate.ram_headroom_mb)
                .map(|m| {
                    check(&sh.cfg.gate, power.as_ref(), free, m.ram_mb).map_err(|d| d.to_string())
                });
            Ok(json!({
                "mock": sh.cfg.mock, "installed_models": sh.cfg.registry.installed().iter().map(|m| &m.id).collect::<Vec<_>>(),
                "free_ram_mb": free, "power": power, "ready": gate.as_ref().is_some_and(|g| g.is_ok()) || sh.cfg.mock,
                "blocked_by": gate.and_then(|g| g.err()), "store_unlocked": sh.store.lock().unwrap().is_some(),
                "encrypted": sh.cfg.encrypt, "privacy": "local only; no network; no telemetry",
            }))
        }
        "ai.models" => ipc::ok(&sh.cfg.registry.models),
        "ai.unlock" => {
            let p: UnlockP = ipc::params(p)?;
            let salt_path = dir().join("vault.salt");
            let salt: [u8; 16] = match std::fs::read(&salt_path)
                .ok()
                .and_then(|b| <[u8; 16]>::try_from(b).ok())
            {
                Some(s) => s,
                None => {
                    let mut s = [0u8; 16];
                    password_hash::rand_core::RngCore::fill_bytes(
                        &mut password_hash::rand_core::OsRng,
                        &mut s,
                    );
                    flow_common::paths::write_atomic(&salt_path, &s)
                        .map_err(|e| RpcError::unavailable(e.to_string()))?;
                    s
                }
            };
            let v = Vault::derive(&p.passphrase, &salt, false).map_err(RpcError::unavailable)?;
            *sh.store.lock().unwrap() = Some(Store::new(dir().join("conversations"), Some(v)));
            Ok(Value::Bool(true))
        }
        "ai.conversations" => {
            let st = sh.store.lock().unwrap();
            Ok(json!(st.as_ref().map(|s| s.list()).unwrap_or_default()))
        }
        "ai.conversation.get" | "ai.conversation.delete" => {
            let p: IdP = ipc::params(p)?;
            let st = sh.store.lock().unwrap();
            let s = st
                .as_ref()
                .ok_or_else(|| RpcError::unavailable("store locked"))?;
            if m.ends_with("get") {
                s.load(&p.id).map(|c| json!(c)).map_err(RpcError::not_found)
            } else {
                Ok(json!(s.delete(&p.id)))
            }
        }
        "ai.privacy.delete_all" => {
            let st = sh.store.lock().unwrap();
            Ok(json!({"deleted": st.as_ref().map(|s| s.delete_all()).unwrap_or(0)}))
        }
        "ai.tools.policy" => Ok(json!(sh.policy.lock().unwrap().tools)),
        "ai.tools.policy.set" => {
            let p: PolP = ipc::params(p)?;
            sh.policy
                .lock()
                .unwrap()
                .set(&p.tool, p.permission)
                .map_err(RpcError::invalid)?;
            let t = serde_json::to_vec(&*sh.policy.lock().unwrap()).unwrap_or_default();
            let _ = flow_common::paths::write_atomic(&dir().join("policy.json"), &t);
            Ok(Value::Bool(true))
        }
        "ai.permission.pending" => ipc::ok(sh.approvals.lock().unwrap().pending()),
        "ai.permission.answer" => {
            let p: AnsP = ipc::params(p)?;
            let call = sh
                .approvals
                .lock()
                .unwrap()
                .take(p.id)
                .ok_or_else(|| RpcError::not_found("prompt"))?;
            if p.remember {
                let perm = if p.allow {
                    Permission::Allow
                } else {
                    Permission::Deny
                };
                let _ = sh.policy.lock().unwrap().set(&call.name, perm);
            }
            if p.allow {
                run_tool(sh, 0, &call).await;
            }
            Ok(Value::Bool(true))
        }
        m => Err(RpcError::unknown_method(m)),
    }
}

struct Iface(Arc<Shared>);

#[zbus::interface(name = "org.flowde.Noxal1")]
impl Iface {
    /// Start a chat turn; tokens are delivered by the `Token` signal. Returns the request id.
    async fn ask(&self, text: String) -> zbus::fdo::Result<u32> {
        let v = chat(
            &self.0,
            ChatP {
                text,
                conversation: None,
            },
        )
        .await
        .map_err(|e| zbus::fdo::Error::Failed(e.message))?;
        Ok(v["request"].as_u64().unwrap_or(0) as u32)
    }
    #[zbus(signal)]
    async fn token(
        em: &zbus::object_server::SignalEmitter<'_>,
        request: u32,
        text: &str,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn done(em: &zbus::object_server::SignalEmitter<'_>, request: u32) -> zbus::Result<()>;
}

fn main() {
    flow_common::daemon::run_daemon("noxal-ai", |bus| async move {
        let cfg: Config =
            flow_common::config::load(&flow_common::paths::config_file("noxal-ai"), 1, &[])?;
        cfg.registry
            .validate()
            .map_err(flow_common::FlowError::Config)?;
        let policy: Policy = std::fs::read_to_string(dir().join("policy.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        let store = if cfg.encrypt {
            None
        } else {
            Some(Store::new(dir().join("conversations"), None))
        };
        let sh = Arc::new(Shared {
            cfg,
            store: Mutex::new(store),
            policy: Mutex::new(policy),
            approvals: Mutex::new(Approvals::default()),
            cancels: Mutex::new(HashMap::new()),
            next_req: Mutex::new(0),
            idle: Mutex::new(IdleUnloader::default()),
            bus: bus.clone(),
            backend: Mutex::new(None),
        });
        let conn = zbus::connection::Builder::session()
            .and_then(|b| b.name("org.flowde.Noxal1"))
            .and_then(|b| b.serve_at("/org/flowde/Noxal1", Iface(sh.clone())));
        let _conn = match conn {
            Ok(b) => b
                .build()
                .await
                .map_err(|e| tracing::warn!("session bus: {e}"))
                .ok(),
            Err(e) => {
                tracing::warn!("session bus unavailable: {e}");
                None
            }
        };
        let s2 = sh.clone();
        let h: Arc<dyn ipc::Handler> = Arc::new(move |m: String, p: Value| {
            let sh = s2.clone();
            ipc::boxed(async move { dispatch(&sh, &m, p).await })
        });
        // Idle unload: drop the backend (frees an in-process model) after the idle timeout.
        let s3 = sh.clone();
        let t = tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(30)).await;
                if s3
                    .idle
                    .lock()
                    .unwrap()
                    .should_unload(now_s(), s3.cfg.gate.idle_unload_s)
                {
                    if let Some(b) = s3.backend.lock().unwrap().take() {
                        b.unload();
                    }
                }
            }
        });
        Ok((h, vec![t]))
    });
}
