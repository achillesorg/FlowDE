//! flow-dlp daemon. Engines run on blocking threads (one per active job).
//! Nothing is downloaded unless a client calls `dlp.add`.

use flow_common::daemon::now_s;
use flow_common::ipc::{self, EventBus, RpcError};
use flow_dlp::args::Options;
use flow_dlp::engine::{EngineError, Engines, YtDlpEngine};
use flow_dlp::queue::{Queue, State};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Config {
    max_parallel: usize,
    /// Wrap yt-dlp in bubblewrap when available.
    sandbox: bool,
    download_dir: Option<String>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            max_parallel: 1,
            sandbox: true,
            download_dir: None,
        }
    }
}

struct Shared {
    q: Mutex<Queue>,
    engines: Engines,
    cancels: Mutex<HashMap<u32, Arc<AtomicBool>>>,
    bus: EventBus,
    default_dir: std::path::PathBuf,
}

fn qpath() -> std::path::PathBuf {
    flow_common::paths::state_dir().join("dlp-queue.json")
}
fn save(q: &Queue) {
    if let Ok(t) = serde_json::to_vec(q) {
        let _ = flow_common::paths::write_atomic(&qpath(), &t);
    }
}

#[derive(Deserialize)]
struct AddP {
    url: String,
    #[serde(default)]
    options: Option<Options>,
}
#[derive(Deserialize)]
struct UrlP {
    url: String,
}
#[derive(Deserialize)]
struct IdP {
    id: u32,
}

fn start_jobs(sh: &Arc<Shared>) {
    let ids = sh.q.lock().unwrap().schedulable(now_s());
    for id in ids {
        let (url, opts) = {
            let q = sh.q.lock().unwrap();
            let j = q.jobs.iter().find(|j| j.id == id).unwrap();
            (j.url.clone(), j.options.clone())
        };
        let flag = Arc::new(AtomicBool::new(false));
        sh.cancels.lock().unwrap().insert(id, flag.clone());
        let sh2 = sh.clone();
        std::thread::spawn(move || {
            let res = match sh2.engines.pick(&url) {
                None => Err(EngineError::NotInstalled(
                    "no engine available (is yt-dlp installed?)".into(),
                )),
                Some(e) => {
                    if let Ok(info) = e.probe(&url) {
                        sh2.q.lock().unwrap().set_title(id, &info.title);
                    }
                    let sh3 = sh2.clone();
                    e.download(&url, &opts, &flag, &mut |p| {
                        sh3.bus.emit("dlp.progress", json!({"id": id, "downloaded": p.downloaded, "total": p.total, "speed_bps": p.speed_bps, "eta_s": p.eta_s}));
                        sh3.q.lock().unwrap().progress(id, p);
                    })
                }
            };
            {
                let mut q = sh2.q.lock().unwrap();
                match &res {
                    Ok(()) => q.finished_ok(id),
                    Err(EngineError::Cancelled) => q.cancelled(id),
                    Err(EngineError::NotInstalled(m)) => q.failed(id, m, false, now_s()),
                    Err(EngineError::Failed {
                        message, retryable, ..
                    }) => q.failed(id, message, *retryable, now_s()),
                }
                save(&q);
            }
            sh2.cancels.lock().unwrap().remove(&id);
            sh2.bus.emit("dlp.job", json!({"id": id}));
        });
    }
}

fn dispatch(sh: &Arc<Shared>, m: &str, p: Value) -> ipc::RpcResult {
    let r = match m {
        "dlp.add" => {
            let p: AddP = ipc::params(p)?;
            let mut o = p.options.unwrap_or_default();
            if o.out_dir == Options::default().out_dir {
                o.out_dir = sh.default_dir.clone();
            }
            // Validate the full argv now so bad requests fail at add time.
            flow_dlp::args::build_args(
                &p.url,
                &o,
                &flow_common::paths::config_dir().join("cookies"),
            )
            .map_err(RpcError::invalid)?;
            let id =
                sh.q.lock()
                    .unwrap()
                    .add(&p.url, o)
                    .map_err(RpcError::invalid)?;
            Ok(json!({"id": id}))
        }
        "dlp.probe" => {
            let p: UrlP = ipc::params(p)?;
            let e = sh.engines.pick(&p.url).ok_or_else(|| {
                RpcError::unavailable("no engine available (is yt-dlp installed?)")
            })?;
            e.probe(&p.url)
                .map(|i| json!(i))
                .map_err(|e| RpcError::unavailable(e.to_string()))
        }
        "dlp.list" => {
            let q = sh.q.lock().unwrap();
            Ok(json!(q.jobs.iter().map(|j| json!({
                "id": j.id, "url": j.url, "title": j.title, "state": j.state, "error": j.error,
                "attempts": j.attempts, "fraction": j.progress.fraction(), "downloaded": j.progress.downloaded,
                "total": j.progress.total, "speed_bps": j.progress.speed_bps, "eta_s": j.progress.eta_s,
            })).collect::<Vec<_>>()))
        }
        "dlp.pause" | "dlp.cancel" => {
            let p: IdP = ipc::params(p)?;
            let mut q = sh.q.lock().unwrap();
            let paused = m == "dlp.pause";
            if paused {
                q.pause(p.id);
            }
            if let Some(f) = sh.cancels.lock().unwrap().get(&p.id) {
                f.store(true, Ordering::Relaxed);
            } else {
                q.cancelled(p.id);
            }
            Ok(Value::Bool(true))
        }
        "dlp.resume" => {
            let p: IdP = ipc::params(p)?;
            if sh.q.lock().unwrap().resume(p.id) {
                Ok(Value::Bool(true))
            } else {
                Err(RpcError::not_found("resumable job"))
            }
        }
        "dlp.remove" => {
            let p: IdP = ipc::params(p)?;
            Ok(json!(sh.q.lock().unwrap().remove(p.id)))
        }
        "dlp.clear_finished" => Ok(json!(sh.q.lock().unwrap().clear_finished())),
        "dlp.status" => {
            let q = sh.q.lock().unwrap();
            Ok(json!({
                "engines": sh.engines.0.iter().map(|e| json!({"name": e.name(), "available": e.available()})).collect::<Vec<_>>(),
                "active": q.jobs.iter().filter(|j| j.state == State::Downloading).count(),
                "queued": q.jobs.iter().filter(|j| j.state == State::Queued).count(),
                "legal": "You are responsible for content rights and site terms of service.",
            }))
        }
        m => return Err(RpcError::unknown_method(m)),
    };
    save(&sh.q.lock().unwrap());
    start_jobs(sh);
    r
}

fn main() {
    flow_common::daemon::run_daemon("flow-dlp", |bus| async {
        let cfg: Config =
            flow_common::config::load(&flow_common::paths::config_file("flow-dlp"), 1, &[])?;
        let mut q: Queue = std::fs::read_to_string(qpath())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_else(|| Queue::new(cfg.max_parallel));
        q.max_parallel = cfg.max_parallel.max(1);
        q.recover();
        let home = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| "/tmp".into());
        let default_dir = cfg
            .download_dir
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| home.join("Downloads"));
        let cookies = flow_common::paths::config_dir().join("cookies");
        let have_bwrap = std::env::var_os("PATH")
            .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join("bwrap").is_file()));
        let engines = Engines(vec![Box::new(YtDlpEngine::new(
            cookies,
            cfg.sandbox && have_bwrap,
        ))]);
        let sh = Arc::new(Shared {
            q: Mutex::new(q),
            engines,
            cancels: Mutex::new(HashMap::new()),
            bus,
            default_dir,
        });
        let s2 = sh.clone();
        let handler: Arc<dyn ipc::Handler> = Arc::new(move |m: String, p: Value| {
            let sh = s2.clone();
            ipc::boxed(async move { dispatch(&sh, &m, p) })
        });
        // Retry timer: wake queued jobs whose backoff elapsed.
        let s3 = sh.clone();
        let t = tokio::spawn(async move {
            loop {
                start_jobs(&s3);
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
        });
        Ok((handler, vec![t]))
    });
}
