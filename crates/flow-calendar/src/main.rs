//! flow-calendar daemon: local store, range queries, reminders.

use flow_calendar::*;
use flow_common::daemon::now_s;
use flow_common::ipc::{self, EventBus, RpcError};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct Shared {
    cal: Mutex<Calendar>,
    bus: EventBus,
}

fn path() -> std::path::PathBuf {
    flow_common::paths::state_dir().join("calendar.json")
}
fn save(c: &Calendar) {
    if let Ok(t) = serde_json::to_vec(c) {
        let _ = flow_common::paths::write_atomic(&path(), &t);
    }
}

#[derive(Deserialize)]
struct RangeP {
    from: i64,
    to: i64,
}
#[derive(Deserialize)]
struct UidP {
    uid: String,
}
#[derive(Deserialize)]
struct IcsP {
    ics: String,
}

fn dispatch(sh: &Shared, m: &str, p: Value) -> ipc::RpcResult {
    let mut c = sh.cal.lock().unwrap();
    match m {
        "calendar.range" => {
            let p: RangeP = ipc::params(p)?;
            if p.to < p.from || p.to - p.from > 366 * 86_400 {
                return Err(RpcError::invalid("range must be 0..366 days"));
            }
            ipc::ok(c.range(p.from, p.to))
        }
        "calendar.upsert" => {
            let e: Event = ipc::params(p)?;
            if e.uid.is_empty() || e.end < e.start {
                return Err(RpcError::invalid("uid required and end >= start"));
            }
            c.upsert(e);
            save(&c);
            Ok(Value::Bool(true))
        }
        "calendar.remove" => {
            let p: UidP = ipc::params(p)?;
            let ok = c.remove(&p.uid);
            save(&c);
            if ok {
                Ok(Value::Bool(true))
            } else {
                Err(RpcError::not_found("event"))
            }
        }
        "calendar.import_ics" => {
            let p: IcsP = ipc::params(p)?;
            if p.ics.len() > 4 << 20 {
                return Err(RpcError::invalid("ics too large"));
            }
            let n = c.import_ics(&p.ics);
            save(&c);
            Ok(json!({"imported": n}))
        }
        "calendar.next_reminder" => ipc::ok(c.next_reminder(now_s() as i64, 7 * 86_400)),
        "calendar.events" => ipc::ok(&c.events),
        m => Err(RpcError::unknown_method(m)),
    }
}

fn main() {
    flow_common::daemon::run_daemon("flow-calendar", |bus| async {
        let cal: Calendar = std::fs::read_to_string(path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        let sh = Arc::new(Shared {
            cal: Mutex::new(cal),
            bus,
        });
        let handler = flow_common::daemon::sync_handler(sh.clone(), dispatch);
        // Reminder loop: sleep until the next reminder (capped at 5 min so edits are noticed).
        let s2 = sh.clone();
        let t = tokio::spawn(async move {
            let mut last = now_s() as i64;
            loop {
                let now = now_s() as i64;
                let (due, wait) = {
                    let c = s2.cal.lock().unwrap();
                    let due = c.reminders(last, now);
                    let wait = c
                        .next_reminder(now, 300)
                        .map(|r| (r.fire_at - now).clamp(1, 300))
                        .unwrap_or(300);
                    (due, wait)
                };
                for r in due {
                    s2.bus.emit("calendar.reminder", &r);
                }
                last = now;
                tokio::time::sleep(Duration::from_secs(wait as u64)).await;
            }
        });
        Ok((handler, vec![t]))
    });
}
