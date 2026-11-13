//! kiln-lock daemon. Control socket `lock.*`; emits `lock.changed`.

use flow_common::events::EV_LOCK;
use flow_common::ipc::{self, boxed, EventBus, Handler, RpcError};
use kiln_lock::passcode::{Params, PasscodeStore};
use kiln_lock::{LockMachine, NoBiometric, Reason};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

const NAME: &str = "kiln-lock";

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Deserialize)]
struct PinP {
    pin: String,
}
#[derive(Deserialize)]
struct SetP {
    new: String,
    #[serde(default)]
    current: Option<String>,
}
#[derive(Deserialize)]
struct AutoP {
    seconds: u64,
}

struct Shared {
    m: Mutex<LockMachine>,
    bus: EventBus,
}

fn status_event(sh: &Shared, m: &LockMachine) {
    sh.bus.emit(EV_LOCK, m.status(now()));
}

fn dispatch(sh: &Shared, method: &str, p: Value) -> ipc::RpcResult {
    let mut m = sh.m.lock().unwrap();
    let t = now();
    match method {
        "lock.status" => ipc::ok(m.status(t)),
        "lock.lock" => {
            let changed = m.lock(Reason::Manual)?;
            if changed {
                status_event(sh, &m);
            }
            Ok(json!({"changed": changed}))
        }
        "lock.unlock" => {
            let p: PinP = ipc::params(p)?;
            let r = m.unlock_with_pin(&p.pin, t);
            status_event(sh, &m);
            Ok(json!({"unlocked": r?}))
        }
        "lock.unlock_biometric" => {
            let ok = m.unlock_with_biometric();
            if ok {
                status_event(sh, &m);
            }
            Ok(json!({"unlocked": ok}))
        }
        "lock.set_passcode" => {
            let p: SetP = ipc::params(p)?;
            m.set_passcode(&p.new, p.current.as_deref())?;
            status_event(sh, &m);
            Ok(Value::Bool(true))
        }
        "lock.clear_passcode" => {
            let p: PinP = ipc::params(p)?;
            m.clear_passcode(&p.pin)?;
            status_event(sh, &m);
            Ok(Value::Bool(true))
        }
        "lock.set_auto_lock" => {
            let p: AutoP = ipc::params(p)?;
            m.auto_lock_s = p.seconds.min(3600);
            Ok(Value::Bool(true))
        }
        "lock.screen_off" => {
            let locked = m.screen_off(t);
            if locked {
                status_event(sh, &m);
            }
            Ok(json!({"locked": locked}))
        }
        "lock.screen_on" => {
            m.screen_on();
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
    let store = PasscodeStore::open(
        flow_common::paths::state_dir().join("passcode"),
        Params::standard(),
    )?;
    let bus = EventBus::new();
    let shared = Arc::new(Shared {
        m: Mutex::new(LockMachine::new(store, 30, Box::new(NoBiometric))),
        bus: bus.clone(),
    });
    let sh = shared.clone();
    let handler: Arc<dyn Handler> = Arc::new(move |m: String, p: Value| {
        let sh = sh.clone();
        boxed(async move { dispatch(&sh, &m, p) })
    });
    let task = flow_common::daemon::serve_control(NAME, bus, handler).await?;
    // Auto-lock timer: only wakes when a deadline is pending, otherwise sleeps long.
    let sh = shared.clone();
    let timer = tokio::spawn(async move {
        loop {
            let wait =
                sh.m.lock()
                    .unwrap()
                    .next_deadline(now())
                    .map(|s| s + 1)
                    .unwrap_or(3600);
            tokio::time::sleep(std::time::Duration::from_secs(wait.min(3600))).await;
            let mut m = sh.m.lock().unwrap();
            if m.tick(now()) {
                status_event(&sh, &m);
            }
        }
    });
    flow_common::daemon::shutdown_signal().await;
    timer.abort();
    task.abort();
    flow_common::daemon::cleanup(NAME);
    Ok(())
}
