//! flow-power daemon.
//!
//! Sources: sysfs (battery, AC, thermal, backlight); UPower, power-profiles-daemon
//! and logind on the system bus when present. Adaptive poll: slower when idle
//! on battery, faster when warm.

use flow_common::events::{PowerSnapshot, EV_POWER};
use flow_common::ipc::{self, boxed, EventBus, Handler, RpcError};
use flow_power::dbus::{self, upower_state};
use flow_power::policy::{IdleAction, IdleConfig, LowPower, LowPowerSetting, ThermalTracker};
use flow_power::sysfs;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const NAME: &str = "flow-power";

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Config {
    low_power_threshold_pct: f32,
    idle: IdleConfig,
    /// Act on idle actions (suspend via logind). Disable for dry runs.
    act: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            low_power_threshold_pct: 20.0,
            idle: IdleConfig::default(),
            act: true,
        }
    }
}

struct State {
    cfg: Config,
    thermal: ThermalTracker,
    lp: LowPower,
    snap: PowerSnapshot,
    on_ac: bool,
    last_activity: u64,
    last_action: IdleAction,
    inhibited: bool,
    source: &'static str,
}

struct Shared {
    st: Mutex<State>,
    bus: EventBus,
    sysroot: PathBuf,
    system: Option<zbus::Connection>,
}

async fn upower(conn: &zbus::Connection) -> Option<(f32, bool)> {
    let p = dbus::UPowerDisplayProxy::new(conn).await.ok()?;
    if !p.is_present().await.ok()? {
        return None;
    }
    let pct = p.percentage().await.ok()? as f32;
    let (charging, _) = upower_state(p.state().await.ok()?);
    Some((pct, charging))
}

/// Re-read sources, update state, emit if changed. Returns next poll delay.
async fn poll(sh: &Shared) -> Duration {
    let mut src = "none";
    let mut batt: Option<(f32, bool)> = None;
    if let Some(c) = &sh.system {
        if let Some(b) = upower(c).await {
            batt = Some(b);
            src = "upower";
        }
    }
    let sysb = sysfs::battery(&sh.sysroot);
    if batt.is_none() {
        if let Some(b) = &sysb {
            batt = Some((b.percent, b.charging));
            src = "sysfs";
        }
    }
    let ac = sysfs::ac_online(&sh.sysroot) || batt.is_some_and(|b| b.1);
    let temp = sysfs::max_temp_c(&sh.sysroot);
    let mut st = sh.st.lock().unwrap();
    let thermal = temp.map(|t| st.thermal.update(t)).unwrap_or_default();
    let (pct, charging) = (batt.map(|b| b.0), batt.is_some_and(|b| b.1));
    let lp = st.lp.update(pct, charging);
    let snap = PowerSnapshot {
        battery_pct: pct,
        charging,
        thermal,
        low_power: lp,
    };
    st.on_ac = ac;
    st.source = src;
    let changed = st.snap != snap;
    st.snap = snap.clone();
    let delay = if thermal != flow_common::events::Thermal::Normal {
        5
    } else if charging || ac {
        30
    } else if lp {
        120
    } else {
        60
    };
    drop(st);
    if changed {
        sh.bus.emit(EV_POWER, snap);
    }
    Duration::from_secs(delay)
}

#[derive(Deserialize)]
struct ProfileP {
    profile: String,
}
#[derive(Deserialize)]
struct LpP {
    mode: LowPowerSetting,
}
#[derive(Deserialize)]
struct BrP {
    fraction: f32,
}
#[derive(Deserialize)]
struct InhP {
    inhibit: bool,
}

async fn dispatch(sh: &Shared, m: &str, p: Value) -> ipc::RpcResult {
    match m {
        "power.snapshot" => ipc::ok(sh.st.lock().unwrap().snap.clone()),
        "power.status" => {
            let (snap, ac, src, lp_setting, idle_s, act) = {
                let st = sh.st.lock().unwrap();
                (
                    st.snap.clone(),
                    st.on_ac,
                    st.source,
                    st.lp.setting,
                    now().saturating_sub(st.last_activity),
                    st.last_action,
                )
            };
            let batt = sysfs::battery(&sh.sysroot);
            let bl = sysfs::backlight(&sh.sysroot);
            let profile = match &sh.system {
                Some(c) => match dbus::PowerProfilesProxy::new(c).await {
                    Ok(p) => p.active_profile().await.ok(),
                    Err(_) => None,
                },
                None => None,
            };
            Ok(json!({
                "snapshot": snap, "ac_online": ac, "source": src, "battery": batt,
                "temp_c": sysfs::max_temp_c(&sh.sysroot), "backlight": bl,
                "profile": profile, "low_power_setting": lp_setting,
                "idle_s": idle_s, "last_idle_action": act,
            }))
        }
        "power.profile.set" => {
            let p: ProfileP = ipc::params(p)?;
            if !dbus::PROFILES.contains(&p.profile.as_str()) {
                return Err(RpcError::invalid(format!(
                    "profile must be one of {:?}",
                    dbus::PROFILES
                )));
            }
            let c = sh
                .system
                .as_ref()
                .ok_or_else(|| RpcError::unavailable("system bus not available"))?;
            let px = dbus::PowerProfilesProxy::new(c)
                .await
                .map_err(|e| RpcError::unavailable(e.to_string()))?;
            px.set_active_profile(&p.profile)
                .await
                .map_err(|e| RpcError::unavailable(e.to_string()))?;
            Ok(json!({"profile": p.profile}))
        }
        "power.low_power.set" => {
            let p: LpP = ipc::params(p)?;
            sh.st.lock().unwrap().lp.setting = p.mode;
            poll(sh).await;
            ipc::ok(sh.st.lock().unwrap().snap.low_power)
        }
        "power.brightness.set" => {
            let p: BrP = ipc::params(p)?;
            if !(0.0..=1.0).contains(&p.fraction) {
                return Err(RpcError::invalid("fraction must be 0..1"));
            }
            let bl = sysfs::backlight(&sh.sysroot)
                .ok_or_else(|| RpcError::unavailable("no backlight device"))?;
            let c = sh
                .system
                .as_ref()
                .ok_or_else(|| RpcError::unavailable("system bus not available"))?;
            let s = dbus::Login1SessionProxy::new(c)
                .await
                .map_err(|e| RpcError::unavailable(e.to_string()))?;
            // Never go fully black via this API: floor at 1 raw step.
            let raw = ((p.fraction * bl.max as f32).round() as u32).clamp(1, bl.max);
            s.set_brightness("backlight", &bl.name, raw)
                .await
                .map_err(|e| RpcError::unavailable(e.to_string()))?;
            Ok(json!({"raw": raw, "max": bl.max}))
        }
        // Compositor/shell reports input activity (TODO(feature): ext-idle-notify-v1 client).
        "power.activity" => {
            let mut st = sh.st.lock().unwrap();
            st.last_activity = now();
            st.last_action = IdleAction::None;
            Ok(Value::Bool(true))
        }
        "power.inhibit" => {
            let p: InhP = ipc::params(p)?;
            sh.st.lock().unwrap().inhibited = p.inhibit;
            Ok(Value::Bool(true))
        }
        m => Err(RpcError::unknown_method(m)),
    }
}

/// Idle tick: compute the escalating action; emit on change; suspend via logind.
async fn idle_tick(sh: &Shared) -> Duration {
    let (action, wait, suspend) = {
        let mut st = sh.st.lock().unwrap();
        let idle = now().saturating_sub(st.last_activity);
        let a = st
            .cfg
            .idle
            .action(idle, st.snap.low_power, st.on_ac, st.inhibited);
        let wait = st
            .cfg
            .idle
            .next_threshold(idle, st.snap.low_power, st.on_ac)
            .map(|s| s + 1)
            .unwrap_or(30);
        let fire = a != st.last_action && a != IdleAction::None;
        if fire {
            st.last_action = a;
        }
        (
            fire.then_some(a),
            wait,
            st.cfg.act && a == IdleAction::Suspend,
        )
    };
    if let Some(a) = action {
        tracing::info!("idle action {a:?}");
        sh.bus.emit("power.idle_action", json!({"action": a}));
        if suspend {
            if let Some(c) = &sh.system {
                if let Ok(l) = dbus::Login1Proxy::new(c).await {
                    if let Err(e) = l.suspend(false).await {
                        tracing::warn!("suspend failed: {e}");
                    }
                }
            }
        }
    }
    Duration::from_secs(wait.clamp(1, 30))
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
    let cfg: Config = flow_common::config::load(&flow_common::paths::config_file(NAME), 1, &[])?;
    let system = match zbus::Connection::system().await {
        Ok(c) => Some(c),
        Err(e) => {
            tracing::warn!("system bus unavailable ({e}); sysfs only");
            None
        }
    };
    let sysroot = std::env::var_os("FLOW_SYSFS_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| "/sys".into());
    let bus = EventBus::new();
    let st = State {
        lp: LowPower::new(cfg.low_power_threshold_pct),
        cfg,
        thermal: ThermalTracker::new(),
        snap: PowerSnapshot::default(),
        on_ac: false,
        last_activity: now(),
        last_action: IdleAction::None,
        inhibited: false,
        source: "none",
    };
    let shared = Arc::new(Shared {
        st: Mutex::new(st),
        bus: bus.clone(),
        sysroot,
        system,
    });
    let sh = shared.clone();
    let handler: Arc<dyn Handler> = Arc::new(move |m: String, p: Value| {
        let sh = sh.clone();
        boxed(async move { dispatch(&sh, &m, p).await })
    });
    let task = flow_common::daemon::serve_control(NAME, bus, handler).await?;
    let sh = shared.clone();
    let poller = tokio::spawn(async move {
        loop {
            let d = poll(&sh).await;
            tokio::time::sleep(d).await;
        }
    });
    let sh = shared.clone();
    let idler = tokio::spawn(async move {
        loop {
            let d = idle_tick(&sh).await;
            tokio::time::sleep(d).await;
        }
    });
    flow_common::daemon::shutdown_signal().await;
    poller.abort();
    idler.abort();
    task.abort();
    flow_common::daemon::cleanup(NAME);
    Ok(())
}
