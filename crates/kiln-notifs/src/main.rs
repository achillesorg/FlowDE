//! kiln-notifs daemon: org.freedesktop.Notifications on the session bus plus
//! a control socket (`notify.*`) for the shell and `flow` CLI.

use flow_common::events::EV_NOTIFY;
use flow_common::ipc::{self, boxed, EventBus, Handler, RpcError};
use kiln_notifs::{Dnd, NewNotification, Store, Urgency};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use zbus::zvariant::OwnedValue;

const NAME: &str = "kiln-notifs";

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

struct Shared {
    store: Mutex<Store>,
    bus: EventBus,
}

struct Server(Arc<Shared>);

fn parse_actions(a: Vec<String>) -> Vec<(String, String)> {
    a.chunks(2)
        .filter(|c| c.len() == 2)
        .map(|c| (c[0].clone(), c[1].clone()))
        .collect()
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Server {
    fn get_capabilities(&self) -> Vec<String> {
        ["body", "actions", "persistence", "icon-static"]
            .map(String::from)
            .to_vec()
    }

    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        expire_timeout: i32,
    ) -> u32 {
        let urgency = hints
            .get("urgency")
            .and_then(|v| u8::try_from(v).ok())
            .map(Urgency::from_hint)
            .unwrap_or_default();
        let item = self.0.store.lock().unwrap().post(
            NewNotification {
                app: app_name,
                replaces: replaces_id,
                icon: app_icon,
                summary,
                body,
                urgency,
                actions: parse_actions(actions),
                expire_ms: expire_timeout,
            },
            now(),
        );
        self.0.bus.emit(
            EV_NOTIFY,
            json!({"posted": item.id, "silenced": item.silenced}),
        );
        item.id
    }

    async fn close_notification(
        &self,
        id: u32,
        #[zbus(signal_emitter)] em: zbus::object_server::SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        let r = self.0.store.lock().unwrap().dismiss(id);
        if r.is_ok() {
            self.0.bus.emit(EV_NOTIFY, json!({"closed": id}));
            Self::notification_closed(&em, id, 3).await?;
        }
        Ok(())
    }

    fn get_server_information(&self) -> (String, String, String, String) {
        (
            "kiln-notifs".into(),
            "FlowDE".into(),
            env!("CARGO_PKG_VERSION").into(),
            "1.2".into(),
        )
    }

    #[zbus(signal)]
    async fn notification_closed(
        em: &zbus::object_server::SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn action_invoked(
        em: &zbus::object_server::SignalEmitter<'_>,
        id: u32,
        key: &str,
    ) -> zbus::Result<()>;
}

#[derive(Deserialize)]
struct PostP {
    #[serde(default = "d_app")]
    app: String,
    summary: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    urgency: Urgency,
}
fn d_app() -> String {
    "flow".into()
}
#[derive(Deserialize)]
struct IdP {
    id: u32,
}
#[derive(Deserialize)]
struct AppP {
    app: String,
}
#[derive(Deserialize)]
struct HistP {
    #[serde(default = "d_lim")]
    limit: usize,
}
fn d_lim() -> usize {
    50
}
#[derive(Deserialize)]
struct DndP {
    mode: String,
    #[serde(default)]
    minutes: u64,
}

fn dispatch(sh: &Shared, m: &str, p: Value) -> ipc::RpcResult {
    let mut st = sh.store.lock().unwrap();
    let t = now();
    match m {
        "notify.post" => {
            let p: PostP = ipc::params(p)?;
            let n = st.post(
                NewNotification {
                    app: p.app,
                    summary: p.summary,
                    body: p.body,
                    urgency: p.urgency,
                    expire_ms: -1,
                    ..Default::default()
                },
                t,
            );
            sh.bus
                .emit(EV_NOTIFY, json!({"posted": n.id, "silenced": n.silenced}));
            Ok(json!({"id": n.id, "silenced": n.silenced}))
        }
        "notify.list" => ipc::ok(st.active()),
        "notify.groups" => ipc::ok(st.groups()),
        "notify.history" => {
            let p: HistP = ipc::params(p)?;
            ipc::ok(st.history(p.limit.min(kiln_notifs::MAX_HISTORY)))
        }
        "notify.dismiss" => {
            let p: IdP = ipc::params(p)?;
            st.dismiss(p.id)?;
            sh.bus.emit(EV_NOTIFY, json!({"closed": p.id}));
            Ok(Value::Bool(true))
        }
        "notify.dismiss_app" => {
            let p: AppP = ipc::params(p)?;
            let n = st.dismiss_app(&p.app);
            sh.bus.emit(EV_NOTIFY, json!({"closed_app": p.app}));
            Ok(json!({"dismissed": n}))
        }
        "notify.clear" => {
            st.clear_history();
            sh.bus.emit(EV_NOTIFY, json!({"cleared": true}));
            Ok(Value::Bool(true))
        }
        "notify.dnd.get" => {
            let d = st.dnd();
            Ok(json!({"state": d, "active": d.active(t)}))
        }
        "notify.dnd.set" => {
            let p: DndP = ipc::params(p)?;
            let d = match p.mode.as_str() {
                "on" => Dnd::On,
                "off" => Dnd::Off,
                "for" if p.minutes > 0 => Dnd::Until {
                    until: t + p.minutes * 60,
                },
                _ => return Err(RpcError::invalid("mode must be on|off|for (with minutes)")),
            };
            st.set_dnd(d);
            sh.bus.emit(EV_NOTIFY, json!({"dnd": d}));
            Ok(json!({"state": d}))
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
    let store = Store::new(Some(
        flow_common::paths::state_dir().join("notifications.json"),
    ))?;
    let bus = EventBus::new();
    let shared = Arc::new(Shared {
        store: Mutex::new(store),
        bus: bus.clone(),
    });
    let sh = shared.clone();
    let handler: Arc<dyn Handler> = Arc::new(move |m: String, p: Value| {
        let sh = sh.clone();
        boxed(async move { dispatch(&sh, &m, p) })
    });
    let task = flow_common::daemon::serve_control(NAME, bus, handler).await?;

    // The bus name is optional: another daemon (e.g. mako) may own it.
    let _conn = match zbus::connection::Builder::session()
        .and_then(|b| b.name("org.freedesktop.Notifications"))
        .and_then(|b| b.serve_at("/org/freedesktop/Notifications", Server(shared.clone())))
    {
        Ok(b) => match b.build().await {
            Ok(c) => Some(c),
            Err(e) => {
                tracing::warn!(
                    "cannot own org.freedesktop.Notifications: {e}; control socket only"
                );
                None
            }
        },
        Err(e) => {
            tracing::warn!("session bus unavailable: {e}; control socket only");
            None
        }
    };
    flow_common::daemon::shutdown_signal().await;
    task.abort();
    flow_common::daemon::cleanup(NAME);
    Ok(())
}
