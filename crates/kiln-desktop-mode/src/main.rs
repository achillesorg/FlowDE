//! kiln-desktop-mode daemon. Polls DRM connector status (cheap sysfs reads,
//! fast only while a dock is settling) and drives the controller. Starting the
//! FeDe session runs the configured command; input redirection is an event
//! (`desktop.input_redirect`) the compositor handles.

use flow_common::daemon::now_s;
use flow_common::events::{PowerSnapshot, EV_POWER};
use flow_common::ipc::{self, EventBus, RpcError};
use kiln_desktop_mode::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Config {
    policy: Policy,
    require_power: bool,
    /// Command to start the desktop session; `{output}` is substituted.
    start_command: String,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            policy: Policy::Ask,
            require_power: false,
            start_command: "fede-session --output {output}".into(),
        }
    }
}

struct Shared {
    ctl: Mutex<Controller>,
    cfg: Config,
    bus: EventBus,
    sys: PathBuf,
    child: Mutex<Option<tokio::process::Child>>,
    charging: Mutex<bool>,
}

async fn perform(sh: &Shared, a: Action) {
    match &a {
        Action::None => return,
        Action::Prompt { .. } => {}
        Action::StartDesktop { output } => match launch_argv(&sh.cfg.start_command, output) {
            Some(argv) => match tokio::process::Command::new(&argv[0])
                .args(&argv[1..])
                .kill_on_drop(true)
                .spawn()
            {
                Ok(c) => {
                    *sh.child.lock().unwrap() = Some(c);
                    sh.bus.emit(
                        "desktop.input_redirect",
                        json!({"output": output, "enable": true}),
                    );
                }
                Err(e) => tracing::warn!("cannot start desktop session {argv:?}: {e}"),
            },
            None => tracing::warn!("unsafe output name {output:?}, not starting"),
        },
        Action::StopDesktop { output } => {
            let c = sh.child.lock().unwrap().take();
            if let Some(mut c) = c {
                let _ = c.start_kill();
            }
            sh.bus.emit(
                "desktop.input_redirect",
                json!({"output": output, "enable": false}),
            );
        }
    }
    sh.bus.emit("desktop.action", &a);
}

async fn poll(sh: &Shared) -> u64 {
    let conns = external_connectors(&sh.sys);
    let charging = *sh.charging.lock().unwrap();
    let (a, settling) = {
        let mut c = sh.ctl.lock().unwrap();
        let a = c.update(&conns, charging, now_s());
        (a, matches!(c.mode(), Mode::Settling { .. }))
    };
    perform(sh, a).await;
    if settling {
        1
    } else {
        3
    }
}

fn main() {
    flow_common::daemon::run_daemon("kiln-desktop-mode", |bus| async {
        let cfg: Config = flow_common::config::load(
            &flow_common::paths::config_file("kiln-desktop-mode"),
            1,
            &[],
        )?;
        let mut ctl = Controller::new(cfg.policy);
        ctl.require_power = cfg.require_power;
        let sys = std::env::var_os("FLOW_SYSFS_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| "/sys".into());
        let sh = Arc::new(Shared {
            ctl: Mutex::new(ctl),
            cfg,
            bus,
            sys,
            child: Mutex::new(None),
            charging: Mutex::new(false),
        });
        let s2 = sh.clone();
        let h: Arc<dyn ipc::Handler> = Arc::new(move |m: String, p: Value| {
            let sh = s2.clone();
            ipc::boxed(async move {
                match m.as_str() {
                    "desktop.status" => {
                        let c = sh.ctl.lock().unwrap();
                        Ok(
                            json!({"mode": c.mode(), "policy": c.policy, "connectors": external_connectors(&sh.sys)}),
                        )
                    }
                    "desktop.answer" => {
                        #[derive(Deserialize)]
                        struct P {
                            yes: bool,
                        }
                        let p: P = ipc::params(p)?;
                        let a = sh.ctl.lock().unwrap().answer(p.yes);
                        perform(&sh, a).await;
                        Ok(Value::Bool(true))
                    }
                    "desktop.force" => {
                        #[derive(Deserialize)]
                        struct P {
                            on: bool,
                        }
                        let p: P = ipc::params(p)?;
                        let conns = external_connectors(&sh.sys);
                        let a = sh.ctl.lock().unwrap().force(p.on, &conns);
                        if a == Action::None && p.on {
                            return Err(RpcError::unavailable("no external display connected"));
                        }
                        perform(&sh, a).await;
                        Ok(Value::Bool(true))
                    }
                    "desktop.policy" => {
                        #[derive(Deserialize)]
                        struct P {
                            policy: Policy,
                        }
                        let p: P = ipc::params(p)?;
                        sh.ctl.lock().unwrap().policy = p.policy;
                        Ok(Value::Bool(true))
                    }
                    m => Err(RpcError::unknown_method(m)),
                }
            })
        });
        let s3 = sh.clone();
        let poller = tokio::spawn(async move {
            loop {
                let d = poll(&s3).await;
                tokio::time::sleep(Duration::from_secs(d)).await;
            }
        });
        // Charging state from flow-power (docks usually supply power).
        let s4 = sh.clone();
        let feed = tokio::spawn(async move {
            let mut delay = 1u64;
            loop {
                if let Ok(mut c) =
                    ipc::Client::connect(&flow_common::paths::socket_path("flow-power")).await
                {
                    if c.call("subscribe", Value::Null).await.is_ok() {
                        delay = 1;
                        if let Ok(v) = c.call("power.snapshot", Value::Null).await {
                            if let Ok(p) = serde_json::from_value::<PowerSnapshot>(v) {
                                *s4.charging.lock().unwrap() = p.charging;
                            }
                        }
                        while let Ok(ev) = c.next_event().await {
                            if ev.event == EV_POWER {
                                if let Ok(p) = serde_json::from_value::<PowerSnapshot>(ev.data) {
                                    *s4.charging.lock().unwrap() = p.charging;
                                }
                            }
                        }
                    }
                }
                tokio::time::sleep(Duration::from_secs(delay)).await;
                delay = (delay * 2).min(30);
            }
        });
        Ok((h, vec![poller, feed]))
    });
}
