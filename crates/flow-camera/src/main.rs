//! flow-camera daemon: device list + permission prompts. `camera.capture`
//! is intentionally unimplemented (TODO(feature)).

use flow_camera::*;
use flow_common::daemon::now_s;
use flow_common::ipc::{self, EventBus, RpcError};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

struct Shared {
    perms: Mutex<Permissions>,
    bus: EventBus,
    sys: PathBuf,
}
fn path() -> PathBuf {
    flow_common::paths::state_dir().join("camera-perms.json")
}
fn save(p: &Permissions) {
    if let Ok(t) = serde_json::to_vec(p) {
        let _ = flow_common::paths::write_atomic(&path(), &t);
    }
}
#[derive(Deserialize)]
struct AppP {
    app: String,
}
#[derive(Deserialize)]
struct ResP {
    id: u32,
    grant: Grant,
    #[serde(default)]
    remember: bool,
}

fn dispatch(sh: &Shared, m: &str, p: Value) -> ipc::RpcResult {
    let mut perms = sh.perms.lock().unwrap();
    match m {
        "camera.devices" => ipc::ok(enumerate(&sh.sys)),
        "camera.request" => {
            let p: AppP = ipc::params(p)?;
            if enumerate(&sh.sys).is_empty() {
                return Err(RpcError::unavailable("no camera device found"));
            }
            match perms.request(&p.app, now_s()) {
                Access::Granted => Ok(json!({"access": "granted"})),
                Access::Denied => Ok(json!({"access": "denied"})),
                Access::Prompt(id) => {
                    sh.bus
                        .emit("camera.prompt", json!({"id": id, "app": p.app}));
                    Ok(json!({"access": "prompt", "id": id}))
                }
            }
        }
        "camera.pending" => ipc::ok(perms.pending(now_s())),
        "camera.resolve" => {
            let p: ResP = ipc::params(p)?;
            let app = perms
                .resolve(p.id, p.grant, p.remember)
                .ok_or_else(|| RpcError::not_found("prompt"))?;
            save(&perms);
            sh.bus
                .emit("camera.decision", json!({"app": app, "grant": p.grant}));
            Ok(Value::Bool(true))
        }
        "camera.revoke" => {
            let p: AppP = ipc::params(p)?;
            let r = perms.revoke(&p.app);
            save(&perms);
            Ok(json!(r))
        }
        "camera.decided" => ipc::ok(&perms.decided),
        "camera.capture" => Err(RpcError::unavailable(
            "capture not implemented (TODO(feature): libcamera / PipeWire portal)",
        )),
        m => Err(RpcError::unknown_method(m)),
    }
}

fn main() {
    flow_common::daemon::run_daemon("flow-camera", |bus| async {
        let perms: Permissions = std::fs::read_to_string(path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        let sys = std::env::var_os("FLOW_SYSFS_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| "/sys".into());
        let sh = Arc::new(Shared {
            perms: Mutex::new(perms),
            bus,
            sys,
        });
        Ok((flow_common::daemon::sync_handler(sh, dispatch), vec![]))
    });
}
