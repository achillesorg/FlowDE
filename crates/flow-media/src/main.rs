//! flow-media daemon: aggregates MPRIS2 players on the session bus. Without a
//! session bus the player list is empty (no fake data). Refresh is on demand
//! and every 5 s while someone is subscribed.
//! TODO(feature): NameOwnerChanged + PropertiesChanged signals instead of polling.

use flow_common::ipc::{self, Handler, RpcError};
use flow_media::*;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use zbus::zvariant::OwnedValue;

#[zbus::proxy(
    interface = "org.mpris.MediaPlayer2",
    default_path = "/org/mpris/MediaPlayer2"
)]
trait Root {
    #[zbus(property)]
    fn identity(&self) -> zbus::Result<String>;
}

#[zbus::proxy(
    interface = "org.mpris.MediaPlayer2.Player",
    default_path = "/org/mpris/MediaPlayer2"
)]
trait PlayerIface {
    fn play_pause(&self) -> zbus::Result<()>;
    fn play(&self) -> zbus::Result<()>;
    fn pause(&self) -> zbus::Result<()>;
    fn stop(&self) -> zbus::Result<()>;
    fn next(&self) -> zbus::Result<()>;
    fn previous(&self) -> zbus::Result<()>;
    #[zbus(property)]
    fn playback_status(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn metadata(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
    #[zbus(property)]
    fn position(&self) -> zbus::Result<i64>;
    #[zbus(property)]
    fn can_go_next(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn can_go_previous(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn can_control(&self) -> zbus::Result<bool>;
}

struct Shared {
    agg: Mutex<Aggregator>,
    conn: Option<zbus::Connection>,
}

fn meta_str(m: &HashMap<String, OwnedValue>, k: &str) -> String {
    m.get(k)
        .and_then(|v| String::try_from(v.try_clone().ok()?).ok())
        .unwrap_or_default()
}
fn meta_list(m: &HashMap<String, OwnedValue>, k: &str) -> Vec<String> {
    m.get(k)
        .and_then(|v| Vec::<String>::try_from(v.try_clone().ok()?).ok())
        .unwrap_or_default()
}

async fn refresh(sh: &Shared) {
    let Some(c) = &sh.conn else { return };
    let Ok(dbus) = zbus::fdo::DBusProxy::new(c).await else {
        return;
    };
    let Ok(names) = dbus.list_names().await else {
        return;
    };
    let ids: Vec<String> = names
        .iter()
        .filter_map(|n| n.as_str().strip_prefix(MPRIS_PREFIX).map(String::from))
        .collect();
    for id in &ids {
        let bus = format!("{MPRIS_PREFIX}{id}");
        let (Ok(root), Ok(pl)) = (
            RootProxy::builder(c)
                .destination(bus.clone())
                .map(|b| b.cache_properties(zbus::proxy::CacheProperties::No).build()),
            PlayerIfaceProxy::builder(c)
                .destination(bus.clone())
                .map(|b| b.cache_properties(zbus::proxy::CacheProperties::No).build()),
        ) else {
            continue;
        };
        let (Ok(root), Ok(pl)) = (root.await, pl.await) else {
            continue;
        };
        let meta = pl.metadata().await.unwrap_or_default();
        let p = Player {
            id: id.clone(),
            identity: root.identity().await.unwrap_or_else(|_| id.clone()),
            status: Status::parse(&pl.playback_status().await.unwrap_or_default()),
            title: meta_str(&meta, "xesam:title"),
            artist: first_string(&meta_list(&meta, "xesam:artist")),
            album: meta_str(&meta, "xesam:album"),
            art_url: meta_str(&meta, "mpris:artUrl"),
            length_us: meta
                .get("mpris:length")
                .and_then(|v| i64::try_from(v.try_clone().ok()?).ok())
                .unwrap_or(0),
            position_us: pl.position().await.unwrap_or(0),
            can_next: pl.can_go_next().await.unwrap_or(false),
            can_prev: pl.can_go_previous().await.unwrap_or(false),
            can_control: pl.can_control().await.unwrap_or(false),
            last_playing: 0,
        };
        sh.agg.lock().unwrap().update(p);
    }
    let mut a = sh.agg.lock().unwrap();
    let gone: Vec<String> = a
        .players
        .keys()
        .filter(|k| !ids.contains(k))
        .cloned()
        .collect();
    for g in gone {
        a.remove(&g);
    }
}

async fn send_key(sh: &Shared, k: Key) -> ipc::RpcResult {
    let (id, method) = sh
        .agg
        .lock()
        .unwrap()
        .route(k)
        .ok_or_else(|| RpcError::unavailable("no controllable player"))?;
    let c = sh
        .conn
        .as_ref()
        .ok_or_else(|| RpcError::unavailable("session bus not available"))?;
    let pl = PlayerIfaceProxy::builder(c)
        .destination(format!("{MPRIS_PREFIX}{id}"))
        .map_err(|e| RpcError::unavailable(e.to_string()))?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .map_err(|e| RpcError::unavailable(e.to_string()))?;
    let r = match k {
        Key::PlayPause => pl.play_pause().await,
        Key::Play => pl.play().await,
        Key::Pause => pl.pause().await,
        Key::Stop => pl.stop().await,
        Key::Next => pl.next().await,
        Key::Previous => pl.previous().await,
    };
    r.map_err(|e| RpcError::unavailable(e.to_string()))?;
    Ok(json!({"player": id, "method": method}))
}

#[derive(Deserialize)]
struct KeyP {
    key: Key,
}
#[derive(Deserialize)]
struct PinP {
    id: Option<String>,
}

fn main() {
    flow_common::daemon::run_daemon("flow-media", |bus| async move {
        let sh = Arc::new(Shared {
            agg: Mutex::new(Aggregator::default()),
            conn: zbus::Connection::session().await.ok(),
        });
        let s2 = sh.clone();
        let h: Arc<dyn Handler> = Arc::new(move |m: String, p: Value| {
            let sh = s2.clone();
            ipc::boxed(async move {
                match m.as_str() {
                    "media.players" | "media.active" => {
                        refresh(&sh).await;
                        let a = sh.agg.lock().unwrap();
                        if m == "media.active" {
                            ipc::ok(a.active())
                        } else {
                            ipc::ok(a.players.values().collect::<Vec<_>>())
                        }
                    }
                    "media.key" => {
                        let p: KeyP = ipc::params(p)?;
                        refresh(&sh).await;
                        send_key(&sh, p.key).await
                    }
                    "media.pin" => {
                        let p: PinP = ipc::params(p)?;
                        sh.agg.lock().unwrap().pinned = p.id;
                        Ok(Value::Bool(true))
                    }
                    m => Err(RpcError::unknown_method(m)),
                }
            })
        });
        let s3 = sh.clone();
        let t = tokio::spawn(async move {
            let mut last = String::new();
            loop {
                if bus.subscribers() > 0 {
                    refresh(&s3).await;
                    let cur =
                        serde_json::to_string(&s3.agg.lock().unwrap().active()).unwrap_or_default();
                    if cur != last {
                        bus.emit(
                            "media.changed",
                            serde_json::from_str::<Value>(&cur).unwrap_or(Value::Null),
                        );
                        last = cur;
                    }
                    tokio::time::sleep(Duration::from_secs(5)).await;
                } else {
                    tokio::time::sleep(Duration::from_secs(15)).await;
                }
            }
        });
        Ok((h, vec![t]))
    });
}
