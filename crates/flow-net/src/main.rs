//! flow-net daemon: NetworkManager (system bus) radios and state, ModemManager
//! presence, airplane mode, usage counters. Without the buses, status reports
//! `nm: false` and set operations fail with `unavailable` (never fake data).

use flow_common::ipc::{self, boxed, Handler, RpcError};
use flow_net::*;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use zbus::proxy;

#[proxy(
    interface = "org.freedesktop.NetworkManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager"
)]
trait NetworkManager {
    #[zbus(property)]
    fn networking_enabled(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn wireless_enabled(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn set_wireless_enabled(&self, v: bool) -> zbus::Result<()>;
    #[zbus(property)]
    fn wireless_hardware_enabled(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn wwan_enabled(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn set_wwan_enabled(&self, v: bool) -> zbus::Result<()>;
    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn connectivity(&self) -> zbus::Result<u32>;
}

#[proxy(
    interface = "org.freedesktop.DBus.ObjectManager",
    default_service = "org.freedesktop.ModemManager1",
    default_path = "/org/freedesktop/ModemManager1"
)]
trait ModemObjects {
    fn get_managed_objects(
        &self,
    ) -> zbus::Result<
        std::collections::HashMap<
            zbus::zvariant::OwnedObjectPath,
            std::collections::HashMap<
                String,
                std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
            >,
        >,
    >;
}

fn nm_state(s: u32) -> &'static str {
    match s {
        10 => "asleep",
        20 => "disconnected",
        30 => "disconnecting",
        40 => "connecting",
        50 => "connected-local",
        60 => "connected-site",
        70 => "connected-global",
        _ => "unknown",
    }
}
fn connectivity(s: u32) -> &'static str {
    match s {
        1 => "none",
        2 => "portal",
        3 => "limited",
        4 => "full",
        _ => "unknown",
    }
}

struct Shared {
    conn: Option<zbus::Connection>,
    air: Mutex<Airplane>,
    usage: Mutex<Usage>,
    sysroot: PathBuf,
}

fn state_file() -> PathBuf {
    flow_common::paths::state_dir().join("net.json")
}

fn persist(sh: &Shared) {
    let v = json!({"air": *sh.air.lock().unwrap(), "usage": *sh.usage.lock().unwrap()});
    let _ = flow_common::paths::write_atomic(&state_file(), v.to_string().as_bytes());
}

async fn radios(sh: &Shared) -> Result<Radios, RpcError> {
    let c = sh
        .conn
        .as_ref()
        .ok_or_else(|| RpcError::unavailable("system bus not available"))?;
    let nm = NetworkManagerProxy::new(c)
        .await
        .map_err(|e| RpcError::unavailable(e.to_string()))?;
    Ok(Radios {
        wifi: nm
            .wireless_enabled()
            .await
            .map_err(|e| RpcError::unavailable(e.to_string()))?,
        cellular: nm.wwan_enabled().await.unwrap_or(false),
    })
}

async fn apply(sh: &Shared, r: Radios) -> Result<(), RpcError> {
    let c = sh
        .conn
        .as_ref()
        .ok_or_else(|| RpcError::unavailable("system bus not available"))?;
    let nm = NetworkManagerProxy::new(c)
        .await
        .map_err(|e| RpcError::unavailable(e.to_string()))?;
    nm.set_wireless_enabled(r.wifi)
        .await
        .map_err(|e| RpcError::unavailable(e.to_string()))?;
    // WWAN may not exist on wifi-only devices; ignore failure there.
    let _ = nm.set_wwan_enabled(r.cellular).await;
    Ok(())
}

#[derive(Deserialize)]
struct OnP {
    on: bool,
}

fn sample_usage(sh: &Shared) {
    let mut u = sh.usage.lock().unwrap();
    for (i, rx, tx) in read_counters(&sh.sysroot) {
        u.record(&i, rx, tx);
    }
}

async fn dispatch(sh: &Shared, m: &str, p: Value) -> ipc::RpcResult {
    match m {
        "net.status" => {
            let mut out = json!({"nm": false, "modem_manager": false, "airplane": sh.air.lock().unwrap().active});
            if let Some(c) = &sh.conn {
                if let Ok(nm) = NetworkManagerProxy::new(c).await {
                    if let Ok(st) = nm.state().await {
                        out["nm"] = json!(true);
                        out["state"] = json!(nm_state(st));
                        out["connectivity"] =
                            json!(connectivity(nm.connectivity().await.unwrap_or(0)));
                        out["wifi_enabled"] = json!(nm.wireless_enabled().await.ok());
                        out["wifi_hw_enabled"] = json!(nm.wireless_hardware_enabled().await.ok());
                        out["cellular_enabled"] = json!(nm.wwan_enabled().await.ok());
                        out["networking_enabled"] = json!(nm.networking_enabled().await.ok());
                    }
                }
                if let Ok(mm) = ModemObjectsProxy::new(c).await {
                    if let Ok(objs) = mm.get_managed_objects().await {
                        out["modem_manager"] = json!(true);
                        out["modems"] = json!(objs.len());
                    }
                }
            }
            Ok(out)
        }
        "net.wifi.set" => {
            let p: OnP = ipc::params(p)?;
            let mut r = radios(sh).await?;
            r.wifi = p.on;
            if p.on {
                sh.air.lock().unwrap().user_enabled_wifi();
            }
            apply(sh, r).await?;
            Ok(json!({"wifi": p.on}))
        }
        "net.cellular.set" => {
            let p: OnP = ipc::params(p)?;
            let mut r = radios(sh).await?;
            r.cellular = p.on;
            apply(sh, r).await?;
            Ok(json!({"cellular": p.on}))
        }
        "net.airplane.set" => {
            let p: OnP = ipc::params(p)?;
            let target = if p.on {
                let cur = radios(sh).await?;
                sh.air.lock().unwrap().enable(cur)
            } else {
                sh.air.lock().unwrap().disable()
            };
            apply(sh, target).await?;
            persist(sh);
            Ok(json!({"airplane": p.on, "radios": target}))
        }
        "net.usage" => {
            sample_usage(sh);
            let u = sh.usage.lock().unwrap();
            Ok(json!({"total": u.total, "period_start": u.period_start,
                      "period": u.total.keys().map(|k| (k.clone(), u.period(k))).collect::<std::collections::BTreeMap<_,_>>()}))
        }
        "net.usage.reset_period" => {
            sample_usage(sh);
            sh.usage
                .lock()
                .unwrap()
                .reset_period(flow_common::daemon::now_s());
            persist(sh);
            Ok(Value::Bool(true))
        }
        // TODO(feature): hotspot via NM AddAndActivateConnection (mode=ap, ipv4.method=shared).
        "net.hotspot.start" | "net.hotspot.stop" => Err(RpcError::unavailable(
            "hotspot not implemented yet (TODO(feature))",
        )),
        m => Err(RpcError::unknown_method(m)),
    }
}

fn main() {
    flow_common::daemon::run_daemon("flow-net", |_bus| async {
        let (air, usage): (Airplane, Usage) = std::fs::read_to_string(state_file())
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .map(|v| {
                (
                    serde_json::from_value(v["air"].clone()).unwrap_or_default(),
                    serde_json::from_value(v["usage"].clone()).unwrap_or_default(),
                )
            })
            .unwrap_or_default();
        let sysroot = std::env::var_os("FLOW_SYSFS_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| "/sys".into());
        let conn = match zbus::Connection::system().await {
            Ok(c) => Some(c),
            Err(e) => {
                tracing::warn!("system bus unavailable: {e}");
                None
            }
        };
        let sh = Arc::new(Shared {
            conn,
            air: Mutex::new(air),
            usage: Mutex::new(usage),
            sysroot,
        });
        let s2 = sh.clone();
        let handler: Arc<dyn Handler> = Arc::new(move |m: String, p: Value| {
            let sh = s2.clone();
            boxed(async move { dispatch(&sh, &m, p).await })
        });
        // Usage sampling every 5 minutes (counters are cumulative; no need to be faster).
        let s3 = sh.clone();
        let t = tokio::spawn(async move {
            loop {
                sample_usage(&s3);
                persist(&s3);
                tokio::time::sleep(Duration::from_secs(300)).await;
            }
        });
        Ok((handler, vec![t]))
    });
}
