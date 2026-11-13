//! flow-sensors daemon.
//!
//! Talks to iio-sensor-proxy (`net.hadess.SensorProxy`) on the system bus.
//! Sensors are claimed only while at least one client is subscribed, and
//! released when the last one leaves (sensors cost power).

use flow_common::events::{SensorSnapshot, EV_SENSORS};
use flow_common::ipc::{self, boxed, EventBus, Handler, RpcError};
use flow_sensors::*;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use zbus::proxy;

const NAME: &str = "flow-sensors";

#[proxy(
    interface = "net.hadess.SensorProxy",
    default_service = "net.hadess.SensorProxy",
    default_path = "/net/hadess/SensorProxy"
)]
trait SensorProxy {
    fn claim_accelerometer(&self) -> zbus::Result<()>;
    fn release_accelerometer(&self) -> zbus::Result<()>;
    fn claim_light(&self) -> zbus::Result<()>;
    fn release_light(&self) -> zbus::Result<()>;
    fn claim_proximity(&self) -> zbus::Result<()>;
    fn release_proximity(&self) -> zbus::Result<()>;
    #[zbus(property)]
    fn has_accelerometer(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn accelerometer_orientation(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn has_ambient_light(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn light_level(&self) -> zbus::Result<f64>;
    #[zbus(property)]
    fn light_level_unit(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn has_proximity(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn proximity_near(&self) -> zbus::Result<bool>;
}

#[derive(Default)]
struct State {
    rotate: AutoRotate,
    brightness: AutoBrightness,
    lux: Option<f32>,
    lux_is_lux: bool,
    tilt: Option<(f32, f32)>,
    proximity_near: Option<bool>,
    has: (bool, bool, bool),
    claimed: bool,
    suggested: Option<f32>,
    accel_dev: Option<PathBuf>,
    iio_tilt: bool,
}

struct Shared {
    st: Mutex<State>,
    bus: EventBus,
    conn: Option<zbus::Connection>,
}

fn snapshot(st: &State) -> SensorSnapshot {
    SensorSnapshot {
        lux: if st.lux_is_lux { st.lux } else { None },
        tilt: st.tilt,
    }
}

async fn claim(sh: &Shared, on: bool) {
    let Some(c) = &sh.conn else { return };
    let Ok(p) = SensorProxyProxy::builder(c)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
    else {
        return;
    };
    let has = sh.st.lock().unwrap().has;
    if on {
        if has.0 {
            let _ = p.claim_accelerometer().await;
        }
        if has.1 {
            let _ = p.claim_light().await;
        }
        if has.2 {
            let _ = p.claim_proximity().await;
        }
    } else {
        let _ = p.release_accelerometer().await;
        let _ = p.release_light().await;
        let _ = p.release_proximity().await;
    }
}

/// One sampling pass; returns true if anything the snapshot covers changed.
async fn sample(sh: &Shared) {
    let before = snapshot(&sh.st.lock().unwrap());
    let mut rotated = None;
    let mut sugg = None;
    if let Some(c) = &sh.conn {
        if let Ok(p) = SensorProxyProxy::builder(c)
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
        {
            let has = sh.st.lock().unwrap().has;
            let orient = if has.0 {
                p.accelerometer_orientation().await.ok()
            } else {
                None
            };
            let light = if has.1 {
                p.light_level().await.ok()
            } else {
                None
            };
            let unit = if has.1 {
                p.light_level_unit().await.ok()
            } else {
                None
            };
            let prox = if has.2 {
                p.proximity_near().await.ok()
            } else {
                None
            };
            let mut st = sh.st.lock().unwrap();
            if let Some(o) = orient {
                rotated = st.rotate.feed(Orientation::parse(&o));
            }
            if let Some(l) = light {
                st.lux = Some(l as f32);
                st.lux_is_lux = unit.as_deref() == Some("lux");
                if st.lux_is_lux {
                    sugg = st.brightness.update(l as f32);
                    if sugg.is_some() {
                        st.suggested = sugg;
                    }
                }
            }
            if prox.is_some() {
                st.proximity_near = prox;
            }
        }
    }
    {
        let mut st = sh.st.lock().unwrap();
        if let Some(dev) = st.accel_dev.clone() {
            st.tilt = read_iio_tilt(&dev);
            st.iio_tilt = st.tilt.is_some();
        }
    }
    let after = snapshot(&sh.st.lock().unwrap());
    // Tilt jitter filter: ignore changes < 0.02 g to save wakeups downstream.
    let tilt_moved = match (before.tilt, after.tilt) {
        (Some(a), Some(b)) => (a.0 - b.0).abs() > 0.02 || (a.1 - b.1).abs() > 0.02,
        (a, b) => a != b,
    };
    if before.lux != after.lux || tilt_moved {
        sh.bus.emit(EV_SENSORS, &after);
    }
    if let Some(o) = rotated {
        sh.bus.emit(
            "sensors.orientation",
            json!({"orientation": o, "rotation_deg": o.rotation_deg()}),
        );
    }
    if let Some(b) = sugg {
        sh.bus
            .emit("sensors.brightness_suggestion", json!({"fraction": b}));
    }
}

#[derive(Deserialize)]
struct LockP {
    locked: bool,
}
#[derive(Deserialize)]
struct BiasP {
    bias: f32,
}

fn dispatch(sh: &Shared, m: &str, p: Value) -> ipc::RpcResult {
    let mut st = sh.st.lock().unwrap();
    match m {
        "sensors.snapshot" => ipc::ok(snapshot(&st)),
        "sensors.status" => Ok(json!({
            "available": {"accelerometer": st.has.0, "ambient_light": st.has.1, "proximity": st.has.2, "iio_tilt": st.iio_tilt},
            "claimed": st.claimed,
            "lux": st.lux, "light_unit_is_lux": st.lux_is_lux,
            "ambient": st.lux.filter(|_| st.lux_is_lux).map(classify),
            "orientation": st.rotate.current(), "rotation_deg": st.rotate.current().rotation_deg(),
            "rotation_locked": st.rotate.locked, "tilt": st.tilt,
            "proximity_near": st.proximity_near, "suggested_brightness": st.suggested,
            "system_bus": sh.conn.is_some(),
        })),
        "sensors.rotation_lock" => {
            let p: LockP = ipc::params(p)?;
            st.rotate.locked = p.locked;
            Ok(Value::Bool(true))
        }
        "sensors.auto_brightness_bias" => {
            let p: BiasP = ipc::params(p)?;
            if !(-0.3..=0.3).contains(&p.bias) {
                return Err(RpcError::invalid("bias must be -0.3..0.3"));
            }
            st.brightness.bias = p.bias;
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
    let conn = zbus::Connection::system().await.ok();
    let mut st = State::default();
    if let Some(c) = &conn {
        if let Ok(p) = SensorProxyProxy::builder(c)
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
        {
            st.has = (
                p.has_accelerometer().await.unwrap_or(false),
                p.has_ambient_light().await.unwrap_or(false),
                p.has_proximity().await.unwrap_or(false),
            );
        }
    }
    if conn.is_none() || st.has == (false, false, false) {
        tracing::warn!("iio-sensor-proxy not available: sensors report nothing (no fake data)");
    }
    let sysroot = std::env::var_os("FLOW_SYSFS_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| "/sys".into());
    st.accel_dev = find_iio_accel(&sysroot);
    let bus = EventBus::new();
    let shared = Arc::new(Shared {
        st: Mutex::new(st),
        bus: bus.clone(),
        conn,
    });
    let sh = shared.clone();
    let handler: Arc<dyn Handler> = Arc::new(move |m: String, p: Value| {
        let sh = sh.clone();
        boxed(async move { dispatch(&sh, &m, p) })
    });
    let task = flow_common::daemon::serve_control(NAME, bus, handler).await?;
    let sh = shared.clone();
    let sampler = tokio::spawn(async move {
        loop {
            let subs = sh.bus.subscribers();
            let claimed = sh.st.lock().unwrap().claimed;
            if subs > 0 && !claimed {
                claim(&sh, true).await;
                sh.st.lock().unwrap().claimed = true;
            } else if subs == 0 && claimed {
                claim(&sh, false).await;
                sh.st.lock().unwrap().claimed = false;
            }
            if subs > 0 {
                sample(&sh).await;
                tokio::time::sleep(Duration::from_millis(500)).await;
            } else {
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        }
    });
    flow_common::daemon::shutdown_signal().await;
    sampler.abort();
    claim(&shared, false).await;
    task.abort();
    flow_common::daemon::cleanup(NAME);
    Ok(())
}
