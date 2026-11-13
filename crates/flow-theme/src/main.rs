//! flow-theme daemon: resolves the liquid-glass material from settings,
//! power and sensor state; serves tokens over the control socket and D-Bus.

use flow_common::events::{PowerSnapshot, SensorSnapshot, EV_POWER, EV_SENSORS, EV_SETTINGS};
use flow_common::ipc::{self, boxed, Client, EventBus, Handler, RpcError};
use flow_theme::service::ThemeService;
use flow_theme::themes::{self, AppOverride};
use flow_theme::{gpu, GpuClass, Quality};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

const NAME: &str = "flow-theme";
const DBUS_NAME: &str = "org.flowde.Theme1";
const DBUS_PATH: &str = "/org/flowde/Theme1";

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

struct Shared {
    svc: Mutex<ThemeService>,
    bus: EventBus,
    last: Mutex<String>,
    dbus: OnceLock<zbus::Connection>,
}

impl Shared {
    /// Re-resolve; if tokens changed, emit on the socket bus and D-Bus.
    fn refresh(&self) {
        let (m, pack) = self.svc.lock().unwrap().tokens_for(None, now());
        let key = serde_json::to_string(&(themes::flat(&m), &pack)).unwrap_or_default();
        let mut last = self.last.lock().unwrap();
        if *last == key {
            return;
        }
        *last = key;
        drop(last);
        self.bus
            .emit("theme.changed", json!({"tokens": m, "icon_pack": pack}));
        if let Some(c) = self.dbus.get() {
            let c = c.clone();
            tokio::spawn(async move {
                if let Ok(em) = zbus::object_server::SignalEmitter::new(&c, DBUS_PATH) {
                    let _ = Iface::changed(&em).await;
                }
            });
        }
    }
}

struct Iface(Arc<Shared>);

#[zbus::interface(name = "org.flowde.Theme1")]
impl Iface {
    /// Resolved material tokens (flat name -> value strings).
    fn get_tokens(&self) -> std::collections::BTreeMap<String, String> {
        themes::flat(&self.0.svc.lock().unwrap().tokens(now()))
    }
    /// `format` is "gtk" or "vars".
    fn get_css(&self, format: String) -> zbus::fdo::Result<String> {
        let m = self.0.svc.lock().unwrap().tokens(now());
        match format.as_str() {
            "gtk" => Ok(themes::gtk_css(&m)),
            "vars" => Ok(themes::css_vars(&m)),
            _ => Err(zbus::fdo::Error::InvalidArgs(
                "format must be gtk|vars".into(),
            )),
        }
    }
    #[zbus(signal)]
    async fn changed(em: &zbus::object_server::SignalEmitter<'_>) -> zbus::Result<()>;
}

#[derive(Deserialize, Default)]
struct AppP {
    #[serde(default)]
    app: Option<String>,
    #[serde(default = "d_fmt")]
    format: String,
}
fn d_fmt() -> String {
    "gtk".into()
}
#[derive(Deserialize)]
struct LumaP {
    luma: Option<f32>,
}
#[derive(Deserialize)]
struct MaxP {
    max: Option<Quality>,
}
#[derive(Deserialize)]
struct OvP {
    app: String,
    #[serde(flatten)]
    ov: Option<AppOverride>,
    #[serde(default)]
    clear: bool,
}
#[derive(Deserialize)]
struct IconP {
    name: String,
    #[serde(default = "d_size")]
    size: u32,
    #[serde(default)]
    app: Option<String>,
}
fn d_size() -> u32 {
    48
}
#[derive(Deserialize)]
struct SeedP {
    seed: String,
}

fn dispatch(sh: &Shared, m: &str, p: Value) -> ipc::RpcResult {
    let r = match m {
        "theme.get" | "theme.tokens" | "theme.css" => {
            let p: AppP = ipc::params(p)?;
            let (t, pack) = sh.svc.lock().unwrap().tokens_for(p.app.as_deref(), now());
            match (m, p.format.as_str()) {
                ("theme.get", _) => Ok(json!({"tokens": t, "icon_pack": pack})),
                ("theme.tokens", _) => Ok(json!({"tokens": themes::flat(&t), "icon_pack": pack})),
                (_, "gtk") => Ok(Value::String(themes::gtk_css(&t))),
                (_, "vars") => Ok(Value::String(themes::css_vars(&t))),
                _ => Err(RpcError::invalid("format must be gtk|vars")),
            }
        }
        "theme.list" => {
            let s = sh.svc.lock().unwrap();
            Ok(json!({
                "themes": themes::ThemeKind::ALL.map(|t| t.id()),
                "icon_packs": s.icons.packs.values().collect::<Vec<_>>(),
                "icon_pack_setting": s.icon_pack,
                "overrides": s.overrides,
            }))
        }
        "theme.icon" => {
            let p: IconP = ipc::params(p)?;
            let mut s = sh.svc.lock().unwrap();
            let (_, pack) = s.tokens_for(p.app.as_deref(), now());
            let f = s.icons.lookup(&pack, &p.name, p.size);
            Ok(json!({"pack": pack, "path": f}))
        }
        "theme.set_app_override" => {
            let p: OvP = ipc::params(p)?;
            let mut s = sh.svc.lock().unwrap();
            if p.clear {
                s.overrides.remove(&p.app);
            } else if let Some(ov) = p.ov {
                s.overrides.insert(p.app, ov);
            }
            save_overrides(&s.overrides);
            Ok(Value::Bool(true))
        }
        "theme.set_seed" => {
            // kiln-wall pushes the wallpaper's dominant colour here.
            let p: SeedP = ipc::params(p)?;
            if !sh
                .svc
                .lock()
                .unwrap()
                .apply_setting("appearance.seed", &Value::String(p.seed))
            {
                return Err(RpcError::invalid("seed must be #rrggbb"));
            }
            Ok(Value::Bool(true))
        }
        // The shell reports what is behind the glass (e.g. wallpaper luma under the dock).
        "theme.set_content_luma" => {
            let p: LumaP = ipc::params(p)?;
            sh.svc.lock().unwrap().env.content_luma = p.luma;
            Ok(Value::Bool(true))
        }
        "theme.set_max_quality" => {
            let p: MaxP = ipc::params(p)?;
            sh.svc.lock().unwrap().force_max = p.max;
            Ok(Value::Bool(true))
        }
        "theme.env" => {
            let s = sh.svc.lock().unwrap();
            Ok(
                json!({"gpu": s.env.gpu, "power": s.env.power, "sensors": s.env.sensors, "content_luma": s.env.content_luma}),
            )
        }
        m => return Err(RpcError::unknown_method(m)),
    };
    sh.refresh();
    r
}

fn overrides_path() -> std::path::PathBuf {
    flow_common::paths::state_dir().join("theme-overrides.json")
}

fn save_overrides(o: &std::collections::BTreeMap<String, AppOverride>) {
    if let Ok(t) = serde_json::to_vec(o) {
        let _ = flow_common::paths::write_atomic(&overrides_path(), &t);
    }
}

/// Follow another daemon's events with reconnect backoff. `on_connect`
/// runs after subscribing (used to fetch initial state).
async fn feed(sh: Arc<Shared>, daemon: &'static str) {
    let mut delay = 1u64;
    loop {
        if let Ok(mut c) = Client::connect(&flow_common::paths::socket_path(daemon)).await {
            if c.call("subscribe", Value::Null).await.is_ok() {
                delay = 1;
                initial(&sh, &mut c, daemon).await;
                while let Ok(ev) = c.next_event().await {
                    handle_event(&sh, &ev.event, ev.data);
                }
            }
        }
        // Peer absent or gone: back off (cheap idle), never spin.
        tokio::time::sleep(Duration::from_secs(delay)).await;
        delay = (delay * 2).min(30);
    }
}

async fn initial(sh: &Arc<Shared>, c: &mut Client, daemon: &str) {
    match daemon {
        "flow-settings" => {
            if let Ok(Value::Array(items)) = c.call("settings.list", json!({})).await {
                for it in items {
                    if let Some(k) = it["key"].as_str() {
                        sh.svc.lock().unwrap().apply_setting(k, &it["value"]);
                    }
                }
                sh.refresh();
            }
        }
        "flow-power" => {
            if let Ok(v) = c.call("power.snapshot", Value::Null).await {
                handle_event(sh, EV_POWER, v);
            }
        }
        "flow-sensors" => {
            if let Ok(v) = c.call("sensors.snapshot", Value::Null).await {
                handle_event(sh, EV_SENSORS, v);
            }
        }
        _ => {}
    }
}

fn handle_event(sh: &Shared, name: &str, data: Value) {
    {
        let mut s = sh.svc.lock().unwrap();
        match name {
            EV_POWER => {
                if let Ok(p) = serde_json::from_value::<PowerSnapshot>(data) {
                    s.set_power(p);
                }
            }
            EV_SENSORS => {
                if let Ok(p) = serde_json::from_value::<SensorSnapshot>(data) {
                    s.set_sensors(p);
                }
            }
            EV_SETTINGS => {
                if let (Some(k), v) = (data["key"].as_str(), &data["value"]) {
                    s.apply_setting(k, v);
                }
            }
            _ => return,
        }
    }
    sh.refresh();
}

fn main() {
    flow_common::logging::init(NAME);
    let rt = flow_common::daemon::runtime().expect("runtime");
    if let Err(e) = rt.block_on(run()) {
        tracing::error!("{e}");
        std::process::exit(1);
    }
}

#[derive(Deserialize, serde::Serialize, Default)]
#[serde(default)]
struct Config {
    /// auto | none | low | mid | high
    gpu: String,
}

async fn run() -> flow_common::Result<()> {
    let cfg: Config = flow_common::config::load(&flow_common::paths::config_file(NAME), 1, &[])?;
    let gpu = match cfg.gpu.as_str() {
        "none" => GpuClass::None,
        "low" => GpuClass::Low,
        "mid" => GpuClass::Mid,
        "high" => GpuClass::High,
        _ => gpu::detect(std::path::Path::new("/sys/class/drm")),
    };
    tracing::info!("gpu class {gpu:?}");
    let bus = EventBus::new();
    let shared = Arc::new(Shared {
        svc: Mutex::new({
            let mut t = ThemeService::new(gpu);
            t.icons = flow_theme::icons::Registry::discover(&flow_theme::icons::search_roots());
            if let Some(o) = std::fs::read_to_string(overrides_path())
                .ok()
                .and_then(|x| serde_json::from_str(&x).ok())
            {
                t.overrides = o;
            }
            t
        }),
        bus: bus.clone(),
        last: Mutex::new(String::new()),
        dbus: OnceLock::new(),
    });
    let sh = shared.clone();
    let handler: Arc<dyn Handler> = Arc::new(move |m: String, p: Value| {
        let sh = sh.clone();
        boxed(async move { dispatch(&sh, &m, p) })
    });
    let task = flow_common::daemon::serve_control(NAME, bus, handler).await?;
    match connect_dbus(shared.clone()).await {
        Ok(c) => {
            let _ = shared.dbus.set(c);
        }
        Err(e) => tracing::warn!("session bus unavailable, D-Bus API disabled: {e}"),
    }
    let feeds: Vec<_> = ["flow-settings", "flow-power", "flow-sensors"]
        .into_iter()
        .map(|d| tokio::spawn(feed(shared.clone(), d)))
        .collect();
    flow_common::daemon::shutdown_signal().await;
    for f in feeds {
        f.abort();
    }
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
