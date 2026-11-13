//! flowd: starts and monitors the FlowDE user daemons (for systems without
//! per-daemon systemd user units, e.g. OpenRC/postmarketOS). On systemd the
//! packaged unit files are preferred and flowd only reports health.

use flow_common::daemon::now_s;
use flow_common::ipc::{self, EventBus, RpcError};
use flow_session::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::Notify;

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct Config {
    disabled: Vec<String>,
    /// Directory holding the daemon binaries; default: next to flowd, then PATH.
    bin_dir: Option<PathBuf>,
}

struct Slot {
    unit: Unit,
    status: Status,
    stop: Arc<Notify>,
    stopped_by_user: bool,
    task: Option<tokio::task::JoinHandle<()>>,
}

struct Shared {
    slots: Mutex<BTreeMap<String, Slot>>,
    units: Vec<Unit>,
    bin_dir: Option<PathBuf>,
    bus: EventBus,
}

fn find_binary(dir: &Option<PathBuf>, name: &str) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = dir.iter().cloned().collect();
    if let Ok(me) = std::env::current_exe() {
        dirs.extend(me.parent().map(PathBuf::from));
    }
    if let Some(p) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&p));
    }
    dirs.into_iter().map(|d| d.join(name)).find(|p| p.is_file())
}

async fn run_unit(sh: Arc<Shared>, name: String, stop: Arc<Notify>) {
    let mut backoff = Backoff::default();
    let mut crashes = CrashLoop::default();
    let unit = sh
        .slots
        .lock()
        .unwrap()
        .get(&name)
        .map(|s| s.unit.clone())
        .expect("slot");
    loop {
        let Some(bin) = find_binary(&sh.bin_dir, &name) else {
            tracing::warn!("{name}: binary not found");
            set(&sh, &name, |s| s.status.state = State::Stopped);
            return;
        };
        let started = Instant::now();
        let mut child = match tokio::process::Command::new(&bin)
            .kill_on_drop(true)
            .envs(malloc_env())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("{name}: spawn failed: {e}");
                set(&sh, &name, |s| s.status.state = State::Stopped);
                return;
            }
        };
        let pid = child.id();
        set(&sh, &name, |s| {
            s.status.state = State::Running;
            s.status.pid = pid;
        });
        sh.bus.emit(
            "session.unit",
            json!({"name": name, "state": "running", "pid": pid}),
        );
        let code = tokio::select! {
            r = child.wait() => r.ok().and_then(|s| s.code()),
            _ = stop.notified() => {
                let _ = child.start_kill();
                let _ = child.wait().await;
                set(&sh, &name, |s| { s.status.state = State::Stopped; s.status.pid = None; });
                return;
            }
        };
        let user_stop = sh
            .slots
            .lock()
            .unwrap()
            .get(&name)
            .is_some_and(|s| s.stopped_by_user);
        set(&sh, &name, |s| {
            s.status.last_exit = code;
            s.status.pid = None;
        });
        if !wants_restart(unit.restart, code, user_stop) {
            set(&sh, &name, |s| s.status.state = State::Stopped);
            sh.bus.emit(
                "session.unit",
                json!({"name": name, "state": "stopped", "exit": code}),
            );
            return;
        }
        let mut delay = backoff.next_delay(started.elapsed());
        if let Some(pause) = crashes.record(now_s()) {
            tracing::error!("{name}: crash loop, pausing {pause}s");
            set(&sh, &name, |s| s.status.state = State::GaveUp);
            delay = Duration::from_secs(pause);
        } else {
            set(&sh, &name, |s| s.status.state = State::Backoff);
        }
        sh.bus.emit("session.unit", json!({"name": name, "state": "backoff", "exit": code, "delay_ms": delay.as_millis() as u64}));
        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            _ = stop.notified() => { set(&sh, &name, |s| s.status.state = State::Stopped); return; }
        }
        set(&sh, &name, |s| s.status.restarts += 1);
    }
}

fn set(sh: &Shared, name: &str, f: impl FnOnce(&mut Slot)) {
    if let Some(s) = sh.slots.lock().unwrap().get_mut(name) {
        f(s);
    }
}

fn start(sh: &Arc<Shared>, name: &str) {
    let stop = {
        let mut slots = sh.slots.lock().unwrap();
        let Some(s) = slots.get_mut(name) else { return };
        if s.task.as_ref().is_some_and(|t| !t.is_finished()) {
            return;
        }
        s.stopped_by_user = false;
        s.stop = Arc::new(Notify::new());
        s.stop.clone()
    };
    let t = tokio::spawn(run_unit(sh.clone(), name.to_string(), stop));
    set(sh, name, |s| s.task = Some(t));
}

#[derive(Deserialize)]
struct NameP {
    name: String,
}

async fn dispatch(sh: &Arc<Shared>, m: &str, p: Value) -> ipc::RpcResult {
    match m {
        "session.status" => {
            let slots = sh.slots.lock().unwrap();
            Ok(json!(slots.values().map(|s| &s.status).collect::<Vec<_>>()))
        }
        "session.start" => {
            let p: NameP = ipc::params(p)?;
            for n in closure(&sh.units, &p.name).map_err(RpcError::not_found)? {
                if sh.slots.lock().unwrap().contains_key(&n) {
                    start(sh, &n);
                }
            }
            Ok(Value::Bool(true))
        }
        "session.stop" | "session.restart" => {
            let p: NameP = ipc::params(p)?;
            let stop = {
                let mut slots = sh.slots.lock().unwrap();
                let s = slots
                    .get_mut(&p.name)
                    .ok_or_else(|| RpcError::not_found("unit"))?;
                s.stopped_by_user = true;
                s.stop.clone()
            };
            stop.notify_one();
            if m == "session.restart" {
                tokio::time::sleep(Duration::from_millis(300)).await;
                start(sh, &p.name);
            }
            Ok(Value::Bool(true))
        }
        "session.health" => {
            // Ping every unit's control socket (bounded by the client timeout).
            let names: Vec<String> = sh.slots.lock().unwrap().keys().cloned().collect();
            let mut out = BTreeMap::new();
            for n in names {
                let ok = ipc::call_once(&n, "ping", Value::Null).await.is_ok();
                out.insert(n, ok);
            }
            Ok(json!(out))
        }
        m => Err(RpcError::unknown_method(m)),
    }
}

fn main() {
    flow_common::daemon::run_daemon("flowd", |bus| async move {
        let cfg: Config =
            flow_common::config::load(&flow_common::paths::config_file("flow-session"), 1, &[])?;
        let units: Vec<Unit> = without(default_units(), &cfg.disabled);
        let ord = order(&units).map_err(flow_common::FlowError::Config)?;
        let mut slots = BTreeMap::new();
        for n in &ord {
            let unit = units.iter().find(|u| &u.name == n).cloned().expect("unit");
            let status = Status {
                name: n.clone(),
                state: State::Stopped,
                pid: None,
                restarts: 0,
                last_exit: None,
            };
            slots.insert(
                n.clone(),
                Slot {
                    unit,
                    status,
                    stop: Arc::new(Notify::new()),
                    stopped_by_user: false,
                    task: None,
                },
            );
        }
        let sh = Arc::new(Shared {
            slots: Mutex::new(slots),
            units,
            bin_dir: cfg.bin_dir,
            bus,
        });
        // Staggered boot start in dependency order; keeps the startup CPU spike low.
        let s2 = sh.clone();
        let boot = tokio::spawn(async move {
            for n in ord {
                if s2.units.iter().any(|u| u.name == n && u.autostart) {
                    start(&s2, &n);
                    tokio::time::sleep(Duration::from_millis(150)).await;
                }
            }
        });
        let s3 = sh.clone();
        let h: Arc<dyn ipc::Handler> = Arc::new(move |m: String, p: Value| {
            let sh = s3.clone();
            ipc::boxed(async move { dispatch(&sh, &m, p).await })
        });
        Ok((h, vec![boot]))
    });
}

/// Keep small daemons small: single malloc arena and a fixed mmap threshold so
/// freed large buffers (e.g. argon2's 19 MiB) go back to the OS. Only set when
/// the user has not set them.
fn malloc_env() -> Vec<(&'static str, &'static str)> {
    [
        ("MALLOC_ARENA_MAX", "1"),
        ("MALLOC_MMAP_THRESHOLD_", "131072"),
        ("MALLOC_TRIM_THRESHOLD_", "131072"),
    ]
    .into_iter()
    .filter(|(k, _)| std::env::var_os(k).is_none())
    .collect()
}
