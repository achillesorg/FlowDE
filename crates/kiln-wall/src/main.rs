//! kiln-wall daemon: stores wallpaper config, rotates, computes the dominant
//! colour seed and pushes it to flow-theme (`theme.set_seed`) when the
//! setting `appearance.seed_from_wallpaper` is true. Actually painting the
//! wallpaper is the compositor's job: it reads `wall.get` / `wall.changed`.

use flow_common::daemon::now_s;
use flow_common::ipc::{self, EventBus, RpcError};
use kiln_wall::*;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct Shared {
    cfg: Mutex<Config>,
    dark: Mutex<bool>,
    seed: Mutex<Option<String>>,
    bus: EventBus,
}

fn path() -> PathBuf {
    flow_common::paths::state_dir().join("wallpaper.json")
}
fn save(c: &Config) {
    if let Ok(t) = serde_json::to_vec(c) {
        let _ = flow_common::paths::write_atomic(&path(), &t);
    }
}

#[derive(Deserialize)]
struct SetP {
    #[serde(default = "star")]
    output: String,
    light: Option<PathBuf>,
    dark: Option<PathBuf>,
    #[serde(default)]
    mode: FitMode,
}
fn star() -> String {
    "*".into()
}
#[derive(Deserialize)]
struct RotP {
    paths: Vec<PathBuf>,
    interval_min: u32,
}
#[derive(Deserialize)]
struct SchemeP {
    dark: bool,
}

/// Recompute the seed from the wallpaper that applies to "*" and push it.
async fn refresh_seed(sh: &Shared) {
    let (p, dark) = {
        let c = sh.cfg.lock().unwrap();
        let dark = *sh.dark.lock().unwrap();
        (
            c.entry_for("*")
                .and_then(|e| e.for_scheme(dark))
                .map(Path::to_path_buf)
                .or_else(|| c.rotation.current().map(Path::to_path_buf)),
            dark,
        )
    };
    let _ = dark;
    let Some(p) = p else { return };
    let seed = tokio::task::spawn_blocking(move || decode(&p).map(|i| dominant_seed(&i))).await;
    let Ok(Ok(seed)) = seed else { return };
    *sh.seed.lock().unwrap() = Some(seed.clone());
    sh.bus.emit("wall.changed", json!({"seed": seed}));
    let follow = flow_common::ipc::call_once(
        "flow-settings",
        "settings.get",
        json!({"key": "appearance.seed_from_wallpaper"}),
    )
    .await
    .ok()
    .and_then(|v| v.as_bool())
    .unwrap_or(false);
    if follow {
        let _ = flow_common::ipc::call_once("flow-theme", "theme.set_seed", json!({"seed": seed}))
            .await;
    }
}

use std::path::Path;

async fn dispatch(sh: &Shared, m: &str, p: Value) -> ipc::RpcResult {
    match m {
        "wall.get" => {
            let c = sh.cfg.lock().unwrap();
            Ok(
                json!({"config": *c, "dark": *sh.dark.lock().unwrap(), "seed": *sh.seed.lock().unwrap()}),
            )
        }
        "wall.set" => {
            let p: SetP = ipc::params(p)?;
            for f in p.light.iter().chain(p.dark.iter()) {
                validate_path(f).map_err(RpcError::invalid)?;
            }
            if p.output.len() > 64 || p.light.is_none() && p.dark.is_none() {
                return Err(RpcError::invalid(
                    "output name or at least one image required",
                ));
            }
            {
                let mut c = sh.cfg.lock().unwrap();
                c.outputs.insert(
                    p.output,
                    Entry {
                        light: p.light,
                        dark: p.dark,
                        mode: p.mode,
                    },
                );
                save(&c);
            }
            refresh_seed(sh).await;
            Ok(Value::Bool(true))
        }
        "wall.rotation.set" => {
            let p: RotP = ipc::params(p)?;
            for f in &p.paths {
                validate_path(f).map_err(RpcError::invalid)?;
            }
            {
                let mut c = sh.cfg.lock().unwrap();
                c.rotation = Rotation {
                    paths: p.paths,
                    interval_min: p.interval_min.min(24 * 60),
                    index: 0,
                };
                save(&c);
            }
            Ok(Value::Bool(true))
        }
        "wall.next" => {
            let next = {
                let mut c = sh.cfg.lock().unwrap();
                let n = c.rotation.advance().map(Path::to_path_buf);
                save(&c);
                n
            };
            let Some(n) = next else {
                return Err(RpcError::unavailable("no rotation list"));
            };
            {
                let mut c = sh.cfg.lock().unwrap();
                c.outputs.insert(
                    "*".into(),
                    Entry {
                        light: Some(n.clone()),
                        dark: Some(n.clone()),
                        mode: FitMode::Fill,
                    },
                );
                save(&c);
            }
            sh.bus.emit("wall.changed", json!({"path": n}));
            refresh_seed(sh).await;
            Ok(json!({"path": n}))
        }
        // The theme/scheme owner tells us which variant is active.
        "wall.scheme" => {
            let p: SchemeP = ipc::params(p)?;
            *sh.dark.lock().unwrap() = p.dark;
            sh.bus.emit("wall.changed", json!({"dark": p.dark}));
            refresh_seed(sh).await;
            Ok(Value::Bool(true))
        }
        m => Err(RpcError::unknown_method(m)),
    }
}

fn main() {
    flow_common::daemon::run_daemon("kiln-wall", |bus| async {
        let cfg: Config = std::fs::read_to_string(path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        let sh = Arc::new(Shared {
            cfg: Mutex::new(cfg),
            dark: Mutex::new(false),
            seed: Mutex::new(None),
            bus,
        });
        let s2 = sh.clone();
        let h: Arc<dyn ipc::Handler> = Arc::new(move |m: String, p: Value| {
            let sh = s2.clone();
            ipc::boxed(async move { dispatch(&sh, &m, p).await })
        });
        // Rotation timer: sleeps long when rotation is off.
        let s3 = sh.clone();
        let t = tokio::spawn(async move {
            let mut last = now_s();
            loop {
                let every = {
                    let c = s3.cfg.lock().unwrap();
                    if c.rotation.enabled() {
                        Some(u64::from(c.rotation.interval_min) * 60)
                    } else {
                        None
                    }
                };
                match every {
                    Some(e) if now_s() >= last + e => {
                        let _ = dispatch(&s3, "wall.next", Value::Null).await;
                        last = now_s();
                    }
                    _ => {}
                }
                tokio::time::sleep(Duration::from_secs(if every.is_some() { 30 } else { 300 }))
                    .await;
            }
        });
        Ok((h, vec![t]))
    });
}
