//! flow-clock daemon. Persists alarms/timers/stopwatch, fires them (also after
//! suspend), arms a wake timer via `systemd-run --user` when possible.

use flow_clock::*;
use flow_common::daemon::now_s;
use flow_common::ipc::{self, EventBus, RpcError};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct Shared {
    st: Mutex<ClockState>,
    bus: EventBus,
    last_check: Mutex<u64>,
}

fn path() -> std::path::PathBuf {
    flow_common::paths::state_dir().join("clock.json")
}
fn save(st: &ClockState) {
    if let Ok(t) = serde_json::to_vec(st) {
        let _ = flow_common::paths::write_atomic(&path(), &t);
    }
}

#[derive(Deserialize)]
struct IdP {
    id: u32,
}
#[derive(Deserialize)]
struct TimerP {
    #[serde(default)]
    label: String,
    seconds: u64,
}
#[derive(Deserialize)]
struct SnoozeP {
    id: u32,
    #[serde(default = "d9")]
    minutes: u64,
}
fn d9() -> u64 {
    9
}

/// Fire due items; emits `clock.fired` events. Returns seconds to sleep.
fn tick(sh: &Shared) -> u64 {
    let now = now_s();
    let off = flow_common::time::local_offset_s();
    let mut st = sh.st.lock().unwrap();
    let mut last = sh.last_check.lock().unwrap();
    for f in st.collect_fired(*last, now, off) {
        tracing::info!("fired {} {}", f.kind, f.id);
        sh.bus.emit("clock.fired", &f);
    }
    *last = now;
    save(&st);
    // Cap sleeps at 30 s so a resume or clock jump is noticed quickly; the
    // wake timer covers deep sleep.
    st.next_wake(now, off)
        .map(|t| t.saturating_sub(now).clamp(1, 30))
        .unwrap_or(30)
}

fn arm_wake(sh: &Shared) {
    let now = now_s();
    let off = flow_common::time::local_offset_s();
    let Some(t) = sh.st.lock().unwrap().next_wake(now, off) else {
        return;
    };
    let Some(argv) = wake_timer_argv("flow-clock-wake", t.saturating_sub(now), "flow") else {
        return;
    };
    // Best effort: failures (no systemd --user, no WakeSystem right) are logged only.
    match std::process::Command::new(&argv[0])
        .args(&argv[1..])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
    {
        Ok(s) if s.success() => {}
        Ok(s) => tracing::debug!("wake timer not armed: {s}"),
        Err(e) => tracing::debug!("wake timer not armed: {e}"),
    }
}

fn dispatch(sh: &Shared, m: &str, p: Value) -> ipc::RpcResult {
    let now = now_s();
    let mut st = sh.st.lock().unwrap();
    let r = match m {
        "clock.list" => {
            Ok(json!({"alarms": st.alarms, "timers": st.timers, "stopwatch": st.stopwatch}))
        }
        "clock.alarm.add" => {
            let a: Alarm = ipc::params(p)?;
            st.add_alarm(a)
                .map(|id| json!({"id": id}))
                .map_err(RpcError::invalid)
        }
        "clock.alarm.remove" => {
            let p: IdP = ipc::params(p)?;
            let n = st.alarms.len();
            st.alarms.retain(|a| a.id != p.id);
            if st.alarms.len() == n {
                Err(RpcError::not_found("alarm"))
            } else {
                Ok(Value::Bool(true))
            }
        }
        "clock.alarm.snooze" => {
            let p: SnoozeP = ipc::params(p)?;
            if st.snooze(p.id, now, p.minutes) {
                Ok(Value::Bool(true))
            } else {
                Err(RpcError::not_found("alarm"))
            }
        }
        "clock.timer.start" => {
            let p: TimerP = ipc::params(p)?;
            st.add_timer(&p.label, p.seconds, now)
                .map(|id| json!({"id": id}))
                .map_err(RpcError::invalid)
        }
        "clock.timer.cancel" => {
            let p: IdP = ipc::params(p)?;
            st.timers.retain(|t| t.id != p.id);
            Ok(Value::Bool(true))
        }
        "clock.timer.pause" | "clock.timer.resume" => {
            let p: IdP = ipc::params(p)?;
            match st.timers.iter_mut().find(|t| t.id == p.id) {
                Some(t) => {
                    if m.ends_with("pause") {
                        t.pause(now)
                    } else {
                        t.resume(now)
                    }
                    Ok(Value::Bool(true))
                }
                None => Err(RpcError::not_found("timer")),
            }
        }
        "clock.stopwatch" => {
            #[derive(Deserialize)]
            struct P {
                action: String,
            }
            let p: P = ipc::params(p)?;
            let ms = now * 1000;
            match p.action.as_str() {
                "start" => st.stopwatch.start(ms),
                "stop" => st.stopwatch.stop(ms),
                "lap" => st.stopwatch.lap(ms),
                "reset" => st.stopwatch.reset(),
                _ => return Err(RpcError::invalid("action: start|stop|lap|reset")),
            }
            Ok(json!({"elapsed_ms": st.stopwatch.elapsed_ms(ms)}))
        }
        // Invoked by the systemd wake timer, or by anyone after resume.
        "clock.tick" => {
            drop(st);
            tick(sh);
            return Ok(Value::Bool(true));
        }
        m => return Err(RpcError::unknown_method(m)),
    };
    save(&st);
    drop(st);
    arm_wake(sh);
    r
}

fn main() {
    flow_common::daemon::run_daemon("flow-clock", |bus| async {
        let st: ClockState = std::fs::read_to_string(path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        let sh = Arc::new(Shared {
            st: Mutex::new(st),
            bus,
            last_check: Mutex::new(now_s()),
        });
        let handler = flow_common::daemon::sync_handler(sh.clone(), dispatch);
        let s2 = sh.clone();
        let t = tokio::spawn(async move {
            loop {
                let d = tick(&s2);
                tokio::time::sleep(Duration::from_secs(d)).await;
            }
        });
        Ok((handler, vec![t]))
    });
}
