//! flow-apps daemon: app registry, launching, recents/frecency, lifecycle, permissions.

use flow_apps::exec::{build_argv, LaunchCtx};
use flow_apps::frecency::Frecency;
use flow_apps::perms::{Grant, Permissions};
use flow_apps::registry::{search_dirs, Instance, Registry, RunState};
use flow_common::ipc::{self, boxed, EventBus, Handler, RpcError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::process::Stdio;
use std::sync::{Arc, Mutex};

const NAME: &str = "flow-apps";

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Default, Serialize, Deserialize)]
struct Persist {
    frecency: Frecency,
    perms: Permissions,
}

struct State {
    reg: Registry,
    persist: Persist,
    instances: Vec<Instance>,
}

type Shared = Arc<Mutex<State>>;

fn persist_path() -> std::path::PathBuf {
    flow_common::paths::state_dir().join("apps.json")
}

fn save(st: &State) {
    if let Ok(t) = serde_json::to_vec(&st.persist) {
        if let Err(e) = flow_common::paths::write_atomic(&persist_path(), &t) {
            tracing::warn!("cannot persist apps state: {e}");
        }
    }
}

#[derive(Deserialize)]
struct LaunchP {
    id: String,
    #[serde(default)]
    uris: Vec<String>,
}
#[derive(Deserialize, Default)]
struct SearchP {
    #[serde(default)]
    query: String,
    #[serde(default = "d50")]
    limit: usize,
}
fn d50() -> usize {
    50
}
#[derive(Deserialize)]
struct PermP {
    app: String,
    #[serde(default)]
    perm: Option<String>,
    #[serde(default)]
    grant: Option<Grant>,
}

fn launch(sh: &Shared, p: LaunchP) -> ipc::RpcResult {
    let mut st = sh.lock().unwrap();
    let e = st
        .reg
        .entries
        .get(&p.id)
        .ok_or_else(|| RpcError::not_found(format!("app {}", p.id)))?
        .clone();
    if e.terminal {
        return Err(RpcError::unavailable(
            "terminal apps need a terminal emulator (TODO(feature))",
        ));
    }
    let ctx = LaunchCtx {
        uris: &p.uris,
        name: &e.name,
        icon: e.icon.as_deref(),
        desktop_path: e.path.as_deref(),
    };
    let argv = build_argv(&e.exec, &ctx)?;
    let mut cmd = std::process::Command::new(&argv[0]);
    cmd.args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(dir) = e.work_dir.as_deref() {
        cmd.current_dir(dir);
    }
    let child = cmd
        .spawn()
        .map_err(|err| RpcError::unavailable(format!("spawn {}: {err}", argv[0])))?;
    let pid = child.id();
    let t = now();
    st.persist.frecency.record(&p.id, t);
    st.instances.push(Instance {
        app: p.id.clone(),
        pid,
        started: t,
        state: RunState::Running,
        exit_code: None,
    });
    save(&st);
    drop(st);
    // Reap in the background so no zombies remain and exit state is recorded.
    let sh2 = sh.clone();
    tokio::task::spawn_blocking(move || {
        let mut child = child;
        let code = child.wait().ok().and_then(|s| s.code());
        if let Some(i) = sh2
            .lock()
            .unwrap()
            .instances
            .iter_mut()
            .find(|i| i.pid == pid)
        {
            i.state = RunState::Exited;
            i.exit_code = code;
        }
    });
    Ok(json!({"pid": pid, "argv0": argv[0]}))
}

fn dispatch(sh: &Shared, m: &str, p: Value) -> ipc::RpcResult {
    match m {
        "apps.list" | "apps.search" => {
            let p: SearchP = ipc::params(p)?;
            let st = sh.lock().unwrap();
            ipc::ok(
                st.reg
                    .search(&p.query)
                    .into_iter()
                    .take(p.limit)
                    .collect::<Vec<_>>(),
            )
        }
        "apps.launch" => launch(sh, ipc::params(p)?),
        "apps.recent" => {
            let st = sh.lock().unwrap();
            ipc::ok(st.persist.frecency.recents(10))
        }
        "apps.frequent" => {
            let st = sh.lock().unwrap();
            ipc::ok(st.persist.frecency.top(10, now()))
        }
        "apps.running" => {
            let st = sh.lock().unwrap();
            ipc::ok(&st.instances)
        }
        "apps.reload" => {
            let reg = Registry::scan(&search_dirs(), None, "FlowDE");
            let n = reg.entries.len();
            sh.lock().unwrap().reg = reg;
            Ok(json!({"apps": n}))
        }
        "apps.perm.get" => {
            let p: PermP = ipc::params(p)?;
            let st = sh.lock().unwrap();
            match p.perm {
                Some(perm) => ipc::ok(st.persist.perms.get(&p.app, &perm)),
                None => ipc::ok(st.persist.perms.of(&p.app)),
            }
        }
        "apps.perm.set" => {
            let p: PermP = ipc::params(p)?;
            let (perm, grant) = match (p.perm, p.grant) {
                (Some(a), Some(b)) => (a, b),
                _ => return Err(RpcError::invalid("perm and grant required")),
            };
            let mut st = sh.lock().unwrap();
            st.persist.perms.set(&p.app, &perm, grant)?;
            save(&st);
            Ok(Value::Bool(true))
        }
        m => Err(RpcError::unknown_method(m)),
    }
}

fn main() {
    flow_common::logging::init(NAME);
    let rt = flow_common::daemon::runtime().expect("runtime");
    if let Err(e) = rt.block_on(run()) {
        tracing::error!("{e}");
        std::process::exit(1);
    }
}

async fn run() -> flow_common::Result<()> {
    let persist: Persist = std::fs::read_to_string(persist_path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let reg = Registry::scan(
        &search_dirs(),
        std::env::var("LANG").ok().as_deref(),
        "FlowDE",
    );
    tracing::info!("{} applications", reg.entries.len());
    let shared: Shared = Arc::new(Mutex::new(State {
        reg,
        persist,
        instances: vec![],
    }));
    let sh = shared.clone();
    let handler: Arc<dyn Handler> = Arc::new(move |m: String, p: Value| {
        let sh = sh.clone();
        boxed(async move { dispatch(&sh, &m, p) })
    });
    let task = flow_common::daemon::serve_control(NAME, EventBus::new(), handler).await?;
    flow_common::daemon::shutdown_signal().await;
    task.abort();
    flow_common::daemon::cleanup(NAME);
    Ok(())
}
