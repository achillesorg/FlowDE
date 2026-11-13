//! flow-settings daemon: settings over the control socket and `org.flowde.Settings1` on D-Bus.

use flow_common::events::EV_SETTINGS;
use flow_common::ipc::{self, boxed, EventBus, Handler, RpcError};
use flow_settings::{Change, Schema, Store};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex, OnceLock};

const NAME: &str = "flow-settings";

#[derive(Deserialize)]
struct KeyP {
    key: String,
}
#[derive(Deserialize)]
struct SetP {
    key: String,
    value: Value,
}
#[derive(Deserialize, Default)]
struct ListP {
    #[serde(default)]
    prefix: String,
}

struct Shared {
    store: Mutex<Store>,
    bus: EventBus,
    dbus: OnceLock<zbus::Connection>,
}

const DBUS_NAME: &str = "org.flowde.Settings1";
const DBUS_PATH: &str = "/org/flowde/Settings1";

struct Iface(Arc<Shared>);

#[zbus::interface(name = "org.flowde.Settings1")]
impl Iface {
    /// Value of `key` as a JSON string.
    async fn get(&self, key: String) -> zbus::fdo::Result<String> {
        let v = self
            .0
            .store
            .lock()
            .unwrap()
            .get(&key)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        Ok(v.to_string())
    }
    /// Set `key` to a JSON-encoded value. Returns whether it changed.
    async fn set(&self, key: String, json: String) -> zbus::fdo::Result<bool> {
        let v: Value = serde_json::from_str(&json)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        let c = self
            .0
            .store
            .lock()
            .unwrap()
            .set(&key, &v)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        let changed = c.is_some();
        if let Some(c) = c {
            announce(&self.0, &[c]);
        }
        Ok(changed)
    }
    #[zbus(signal)]
    async fn changed(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        key: &str,
        json: &str,
    ) -> zbus::Result<()>;
}

fn announce(sh: &Shared, changes: &[Change]) {
    for c in changes {
        sh.bus.emit(EV_SETTINGS, c);
    }
    if let Some(conn) = sh.dbus.get() {
        let conn = conn.clone();
        let items: Vec<(String, String)> = changes
            .iter()
            .map(|c| (c.key.clone(), c.value.to_string()))
            .collect();
        tokio::spawn(async move {
            if let Ok(em) = zbus::object_server::SignalEmitter::new(&conn, DBUS_PATH) {
                for (k, v) in items {
                    let _ = Iface::changed(&em, &k, &v).await;
                }
            }
        });
    }
}

fn dispatch(sh: &Shared, method: &str, p: Value) -> ipc::RpcResult {
    let mut st = sh.store.lock().unwrap();
    match method {
        "settings.get" => {
            let p: KeyP = ipc::params(p)?;
            ipc::ok(st.get(&p.key)?)
        }
        "settings.set" => {
            let p: SetP = ipc::params(p)?;
            let c = st.set(&p.key, &p.value)?;
            let changed = c.is_some();
            if let Some(c) = c {
                announce(sh, &[c]);
            }
            Ok(json!({ "changed": changed }))
        }
        "settings.reset" => {
            let p: KeyP = ipc::params(p)?;
            let c = st.reset(&p.key)?;
            let changed = c.is_some();
            if let Some(c) = c {
                announce(sh, &[c]);
            }
            Ok(json!({ "changed": changed }))
        }
        "settings.list" => {
            let p: ListP = ipc::params(p)?;
            ipc::ok(st.list(&p.prefix))
        }
        "settings.export" => ipc::ok(st.export()),
        "settings.import" => {
            let ch = st.import(&p)?;
            let n = ch.len();
            announce(sh, &ch);
            Ok(json!({ "changed": n }))
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
    let mut schema = Schema::builtin();
    let dropins = flow_common::paths::config_dir().join("schema.d");
    if let Ok(rd) = std::fs::read_dir(&dropins) {
        for e in rd.flatten() {
            if e.path().extension().is_some_and(|x| x == "toml") {
                match std::fs::read_to_string(e.path())
                    .map_err(Into::into)
                    .and_then(|t| Schema::parse(&t))
                {
                    Ok(s) => schema.extend(s),
                    Err(err) => tracing::warn!("schema drop-in {:?}: {err}", e.path()),
                }
            }
        }
    }
    let store = Store::new(
        schema,
        Some(flow_common::paths::state_dir().join("settings.toml")),
    )?;
    let bus = EventBus::new();
    let shared = Arc::new(Shared {
        store: Mutex::new(store),
        bus: bus.clone(),
        dbus: OnceLock::new(),
    });
    match connect_dbus(shared.clone()).await {
        Ok(c) => {
            let _ = shared.dbus.set(c);
        }
        Err(e) => tracing::warn!("session bus unavailable, D-Bus API disabled: {e}"),
    }
    let sh = shared.clone();
    let handler: Arc<dyn Handler> = Arc::new(move |m: String, p: Value| {
        let sh = sh.clone();
        boxed(async move { dispatch(&sh, &m, p) })
    });
    let task = flow_common::daemon::serve_control(NAME, bus, handler).await?;
    flow_common::daemon::shutdown_signal().await;
    task.abort();
    flow_common::daemon::cleanup(NAME);
    Ok(())
}

async fn connect_dbus(sh: Arc<Shared>) -> zbus::Result<zbus::Connection> {
    zbus::connection::Builder::session()?
        .name(DBUS_NAME)?
        .serve_at(DBUS_PATH, Iface(sh))?
        .build()
        .await
}
