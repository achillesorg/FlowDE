//! kiln-dm daemon (unprivileged part). Serves the catalogue/policy API and a
//! read-only logind view. `dm.login` is refused until a PAM backend exists.

use flow_common::ipc::{self, EventBus, RpcError};
use kiln_dm::*;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[zbus::proxy(
    interface = "org.freedesktop.login1.Manager",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1"
)]
trait Login1 {
    fn list_sessions(
        &self,
    ) -> zbus::Result<Vec<(String, u32, String, String, zbus::zvariant::OwnedObjectPath)>>;
}

struct Shared {
    cfg: Mutex<Config>,
    sessions: Vec<Session>,
    machine: Mutex<Machine>,
    conn: Option<zbus::Connection>,
    bus: EventBus,
    handover: Mutex<Option<String>>,
}

fn cfg_path() -> std::path::PathBuf {
    flow_common::paths::config_file("kiln-dm")
}

#[derive(Deserialize)]
struct LoginP {
    user: String,
    secret: String,
    session: String,
}
#[derive(Deserialize)]
struct UserP {
    user: String,
}
#[derive(Deserialize)]
struct HandP {
    session: String,
}

async fn dispatch(sh: &Shared, m: &str, p: Value) -> ipc::RpcResult {
    match m {
        "dm.sessions" => ipc::ok(&sh.sessions),
        "dm.preselect" => {
            let p: UserP = ipc::params(p)?;
            if !valid_user(&p.user) {
                return Err(RpcError::invalid("bad user"));
            }
            let c = sh.cfg.lock().unwrap();
            Ok(json!({"session": c.preselect(&p.user, &sh.sessions).map(|s| s.id.clone())}))
        }
        "dm.config.get" => ipc::ok(sh.cfg.lock().unwrap().clone()),
        "dm.config.set" => {
            let c: Config = ipc::params(p)?;
            c.validate().map_err(RpcError::invalid)?;
            flow_common::config::save(&cfg_path(), 1, &c)?;
            *sh.cfg.lock().unwrap() = c;
            Ok(Value::Bool(true))
        }
        "dm.autologin" => {
            let c = sh.cfg.lock().unwrap();
            Ok(match c.autologin(&sh.sessions) {
                Some((u, s)) => json!({"user": u, "session": s.id}),
                None => Value::Null,
            })
        }
        "dm.state" => ipc::ok(sh.machine.lock().unwrap().state.clone()),
        "dm.login" => {
            let p: LoginP = ipc::params(p)?;
            let mut mach = sh.machine.lock().unwrap();
            let r = mach.login(
                &mut NoAuth,
                &sh.sessions,
                &p.user,
                &p.secret,
                &p.session,
                flow_common::daemon::now_s(),
            );
            match r {
                Ok(()) => Ok(json!({"ok": true})),
                Err(LoginError::Unavailable(m)) => Err(RpcError::unavailable(m)),
                Err(LoginError::Throttled(s)) => {
                    Err(RpcError::new("throttled", format!("retry in {s}s")))
                }
                Err(e) => Err(RpcError::invalid(format!("{e:?}"))),
            }
        }
        // Handover between FlowDE and FeDe sessions: recorded and announced; starting the
        // other session needs the privileged launcher (TODO(feature)).
        "dm.handover.request" => {
            let p: HandP = ipc::params(p)?;
            if !sh.sessions.iter().any(|s| s.id == p.session) {
                return Err(RpcError::not_found("session"));
            }
            *sh.handover.lock().unwrap() = Some(p.session.clone());
            sh.bus.emit("dm.handover", json!({"session": p.session}));
            Ok(
                json!({"recorded": true, "note": "session switch needs the privileged launcher (not implemented)"}),
            )
        }
        "dm.logind.sessions" => {
            let c = sh
                .conn
                .as_ref()
                .ok_or_else(|| RpcError::unavailable("system bus not available"))?;
            let px = Login1Proxy::builder(c)
                .cache_properties(zbus::proxy::CacheProperties::No)
                .build()
                .await
                .map_err(|e| RpcError::unavailable(e.to_string()))?;
            let l = px
                .list_sessions()
                .await
                .map_err(|e| RpcError::unavailable(e.to_string()))?;
            Ok(json!(l.into_iter().map(|(id, uid, user, seat, _)| json!({"id": id, "uid": uid, "user": user, "seat": seat})).collect::<Vec<_>>()))
        }
        m => Err(RpcError::unknown_method(m)),
    }
}

fn main() {
    flow_common::daemon::run_daemon("kiln-dm", |bus| async {
        let cfg: Config = flow_common::config::load(&cfg_path(), 1, &[])?;
        let dirs = std::env::var("KILN_DM_DATA_DIRS")
            .unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
        let paths: Vec<std::path::PathBuf> = dirs
            .split(':')
            .filter(|d| !d.is_empty())
            .map(Into::into)
            .collect();
        let refs: Vec<&Path> = paths.iter().map(|p| p.as_path()).collect();
        let sessions = scan_sessions(&refs);
        tracing::info!("{} sessions", sessions.len());
        let sh = Arc::new(Shared {
            cfg: Mutex::new(cfg),
            sessions,
            machine: Mutex::new(Machine::new()),
            conn: zbus::Connection::system().await.ok(),
            bus,
            handover: Mutex::new(None),
        });
        let s2 = sh.clone();
        let h: Arc<dyn ipc::Handler> = Arc::new(move |m: String, p: Value| {
            let sh = s2.clone();
            ipc::boxed(async move { dispatch(&sh, &m, p).await })
        });
        Ok((h, vec![]))
    });
}
