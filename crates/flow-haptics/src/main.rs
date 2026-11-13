//! flow-haptics daemon. Backends in order: feedbackd (session bus), sysfs vibrator.
//! With neither, requests succeed as no-ops and `haptics.status` says so.

use flow_common::events::{PowerSnapshot, EV_POWER};
use flow_common::ipc::{self, boxed, Client, Handler, RpcError};
use flow_haptics::*;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use zbus::zvariant::Value as ZV;

#[zbus::proxy(
    interface = "org.sigxcpu.Feedback",
    default_service = "org.sigxcpu.Feedback",
    default_path = "/org/sigxcpu/Feedback"
)]
trait Feedback {
    fn trigger_feedback(
        &self,
        app_id: &str,
        event: &str,
        hints: HashMap<&str, ZV<'_>>,
        timeout: i32,
    ) -> zbus::Result<u32>;
}

struct Shared {
    set: Mutex<Settings>,
    conn: Option<zbus::Connection>,
    sysfs: Option<SysfsVibrator>,
}

#[derive(Deserialize)]
struct PlayP {
    pattern: Pattern,
}
#[derive(Deserialize)]
struct CfgP {
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    intensity: Option<f32>,
}

async fn play(sh: &Shared, p: Pattern) -> ipc::RpcResult {
    let eff = sh.set.lock().unwrap().effective(p);
    if eff < 0.05 {
        return Ok(json!({"played": false, "reason": "disabled or zero intensity"}));
    }
    if let Some(c) = &sh.conn {
        if let Ok(px) = FeedbackProxy::builder(c)
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
        {
            let mut hints: HashMap<&str, ZV<'_>> = HashMap::new();
            hints.insert("profile", ZV::from("full"));
            if px
                .trigger_feedback("org.flowde.Haptics", p.feedbackd_event(), hints, -1)
                .await
                .is_ok()
            {
                return Ok(json!({"played": true, "backend": "feedbackd"}));
            }
        }
    }
    if let Some(v) = &sh.sysfs {
        for (on, off) in scale(p, eff) {
            v.pulse(on)
                .map_err(|e| RpcError::unavailable(e.to_string()))?;
            tokio::time::sleep(std::time::Duration::from_millis(u64::from(on + off))).await;
        }
        return Ok(json!({"played": true, "backend": "sysfs"}));
    }
    Ok(json!({"played": false, "reason": "no haptic backend available"}))
}

fn main() {
    flow_common::daemon::run_daemon("flow-haptics", |_bus| async {
        let sysroot = std::env::var_os("FLOW_SYSFS_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| "/sys".into());
        let sh = Arc::new(Shared {
            set: Mutex::new(Settings::default()),
            conn: zbus::Connection::session().await.ok(),
            sysfs: SysfsVibrator::find(&sysroot),
        });
        let s2 = sh.clone();
        let handler: Arc<dyn Handler> = Arc::new(move |m: String, p: Value| {
            let sh = s2.clone();
            boxed(async move {
                match m.as_str() {
                    "haptics.play" => {
                        let p: PlayP = ipc::params(p)?;
                        play(&sh, p.pattern).await
                    }
                    "haptics.configure" => {
                        let p: CfgP = ipc::params(p)?;
                        let mut s = sh.set.lock().unwrap();
                        if let Some(e) = p.enabled {
                            s.enabled = e;
                        }
                        if let Some(i) = p.intensity {
                            if !(0.0..=1.0).contains(&i) {
                                return Err(RpcError::invalid("intensity 0..1"));
                            }
                            s.intensity = i;
                        }
                        Ok(Value::Bool(true))
                    }
                    "haptics.status" => {
                        let s = *sh.set.lock().unwrap();
                        Ok(
                            json!({"enabled": s.enabled, "intensity": s.intensity, "low_power": s.low_power,
                                  "feedbackd_bus": sh.conn.is_some(), "sysfs_vibrator": sh.sysfs.is_some()}),
                        )
                    }
                    m => Err(RpcError::unknown_method(m)),
                }
            })
        });
        // Follow low-power mode from flow-power (reconnecting, cheap when absent).
        let s3 = sh.clone();
        let t = tokio::spawn(async move {
            let mut delay = 1u64;
            loop {
                if let Ok(mut c) =
                    Client::connect(&flow_common::paths::socket_path("flow-power")).await
                {
                    if c.call("subscribe", Value::Null).await.is_ok() {
                        delay = 1;
                        if let Ok(v) = c.call("power.snapshot", Value::Null).await {
                            if let Ok(p) = serde_json::from_value::<PowerSnapshot>(v) {
                                s3.set.lock().unwrap().low_power = p.low_power;
                            }
                        }
                        while let Ok(ev) = c.next_event().await {
                            if ev.event == EV_POWER {
                                if let Ok(p) = serde_json::from_value::<PowerSnapshot>(ev.data) {
                                    s3.set.lock().unwrap().low_power = p.low_power;
                                }
                            }
                        }
                    }
                }
                tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                delay = (delay * 2).min(30);
            }
        });
        Ok((handler, vec![t]))
    });
}
