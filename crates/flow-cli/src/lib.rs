//! Command-line parsing for `flow`: maps arguments to control-socket calls.

use serde_json::{json, Value};

/// All daemons `flow status` knows about.
pub const DAEMONS: &[&str] = &[
    "flowd",
    "flow-settings",
    "kiln-notifs",
    "flow-power",
    "flow-sensors",
    "flow-haptics",
    "flow-net",
    "kiln-lock",
    "flow-apps",
    "flow-theme",
    "kiln-wall",
    "flow-clock",
    "flow-calendar",
    "flow-media",
    "kiln-desktop-mode",
    "flow-camera",
    "flow-store",
    "flow-dlp",
    "noxal-ai",
    "kiln-dm",
];

#[derive(Debug, PartialEq)]
pub enum Cmd {
    Status,
    Call {
        daemon: String,
        method: String,
        params: Value,
    },
    /// Print a string result raw (css etc.).
    CallText {
        daemon: String,
        method: String,
        params: Value,
    },
    Help,
}

pub const HELP: &str = "flow: control FlowDE daemons

  flow status
  flow settings get KEY | set KEY VALUE | reset KEY | list [PREFIX] | export | import FILE
  flow notify send SUMMARY [BODY] | list | groups | history | dismiss ID | clear | dnd on|off|for MINUTES
  flow power status | profile power-saver|balanced|performance | lowpower auto|on|off | brightness 0..1
  flow net status | airplane on|off | wifi on|off | cellular on|off | usage
  flow theme get|tokens [APP] | css [gtk|vars] [APP] | list | set THEME | iconpack ID | icon NAME [SIZE] | seed #RRGGBB
  flow apps list [QUERY] | launch ID [URI...] | recent | frequent | running
  flow lock status | lock | unlock PIN | set-pin NEW [CURRENT]
  flow sensors status | haptic PATTERN | media players|active|key KEY | wall get|next
  flow clock list | clock timer SECONDS [LABEL]
  flow ai status | chat TEXT
  flow ipc DAEMON METHOD [JSON]

Options: values that parse as JSON are sent as JSON, anything else as a string.";

/// JSON if it parses, otherwise a string.
pub fn value(s: &str) -> Value {
    serde_json::from_str(s).unwrap_or_else(|_| Value::String(s.to_string()))
}

fn call(d: &str, m: &str, p: Value) -> Result<Cmd, String> {
    Ok(Cmd::Call {
        daemon: d.into(),
        method: m.into(),
        params: p,
    })
}

fn on_off(s: &str) -> Result<bool, String> {
    match s {
        "on" | "true" | "1" => Ok(true),
        "off" | "false" | "0" => Ok(false),
        _ => Err(format!("expected on|off, got {s:?}")),
    }
}

pub fn parse(args: &[String]) -> Result<Cmd, String> {
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    match a.as_slice() {
        [] | ["help"] | ["-h"] | ["--help"] => Ok(Cmd::Help),
        ["status"] => Ok(Cmd::Status),
        ["ipc", d, m, rest @ ..] => {
            call(d, m, rest.first().map(|j| value(j)).unwrap_or(Value::Null))
        }
        ["ipc", ..] => Err("usage: flow ipc DAEMON METHOD [JSON]".into()),
        ["settings", rest @ ..] => match rest {
            ["get", k] => call("flow-settings", "settings.get", json!({"key": k})),
            ["set", k, v] => call(
                "flow-settings",
                "settings.set",
                json!({"key": k, "value": value(v)}),
            ),
            ["reset", k] => call("flow-settings", "settings.reset", json!({"key": k})),
            ["list"] => call("flow-settings", "settings.list", json!({})),
            ["list", p] => call("flow-settings", "settings.list", json!({"prefix": p})),
            ["export"] => call("flow-settings", "settings.export", Value::Null),
            ["import", f] => {
                let t = std::fs::read_to_string(f).map_err(|e| format!("{f}: {e}"))?;
                call(
                    "flow-settings",
                    "settings.import",
                    serde_json::from_str(&t).map_err(|e| format!("{f}: {e}"))?,
                )
            }
            _ => Err("usage: flow settings get|set|reset|list|export|import".into()),
        },
        ["notify", rest @ ..] => match rest {
            ["send", s] => call("kiln-notifs", "notify.post", json!({"summary": s})),
            ["send", s, b] => call(
                "kiln-notifs",
                "notify.post",
                json!({"summary": s, "body": b}),
            ),
            ["list"] => call("kiln-notifs", "notify.list", Value::Null),
            ["groups"] => call("kiln-notifs", "notify.groups", Value::Null),
            ["history"] => call("kiln-notifs", "notify.history", json!({})),
            ["dismiss", id] => call(
                "kiln-notifs",
                "notify.dismiss",
                json!({"id": id.parse::<u32>().map_err(|e| e.to_string())?}),
            ),
            ["clear"] => call("kiln-notifs", "notify.clear", Value::Null),
            ["dnd"] => call("kiln-notifs", "notify.dnd.get", Value::Null),
            ["dnd", "for", m] => call(
                "kiln-notifs",
                "notify.dnd.set",
                json!({"mode": "for", "minutes": m.parse::<u64>().map_err(|e| e.to_string())?}),
            ),
            ["dnd", s @ ("on" | "off")] => {
                call("kiln-notifs", "notify.dnd.set", json!({"mode": s}))
            }
            _ => Err("usage: flow notify send|list|groups|history|dismiss|clear|dnd".into()),
        },
        ["power", rest @ ..] => match rest {
            [] | ["status"] => call("flow-power", "power.status", Value::Null),
            ["profile", p] => call("flow-power", "power.profile.set", json!({"profile": p})),
            ["lowpower", m] => call("flow-power", "power.low_power.set", json!({"mode": m})),
            ["brightness", f] => call(
                "flow-power",
                "power.brightness.set",
                json!({"fraction": f.parse::<f64>().map_err(|e| e.to_string())?}),
            ),
            _ => Err("usage: flow power status|profile|lowpower|brightness".into()),
        },
        ["net", rest @ ..] => match rest {
            [] | ["status"] => call("flow-net", "net.status", Value::Null),
            ["airplane", s] => call("flow-net", "net.airplane.set", json!({"on": on_off(s)?})),
            ["wifi", s] => call("flow-net", "net.wifi.set", json!({"on": on_off(s)?})),
            ["cellular", s] => call("flow-net", "net.cellular.set", json!({"on": on_off(s)?})),
            ["usage"] => call("flow-net", "net.usage", Value::Null),
            _ => Err("usage: flow net status|airplane|wifi|cellular|usage".into()),
        },
        ["theme", rest @ ..] => match rest {
            [] | ["get"] => call("flow-theme", "theme.get", json!({})),
            ["get", app] => call("flow-theme", "theme.get", json!({"app": app})),
            ["tokens"] => call("flow-theme", "theme.tokens", json!({})),
            ["tokens", app] => call("flow-theme", "theme.tokens", json!({"app": app})),
            ["css"] => Ok(Cmd::CallText {
                daemon: "flow-theme".into(),
                method: "theme.css".into(),
                params: json!({"format": "gtk"}),
            }),
            ["css", f @ ("gtk" | "vars")] => Ok(Cmd::CallText {
                daemon: "flow-theme".into(),
                method: "theme.css".into(),
                params: json!({"format": f}),
            }),
            ["css", f @ ("gtk" | "vars"), app] => Ok(Cmd::CallText {
                daemon: "flow-theme".into(),
                method: "theme.css".into(),
                params: json!({"format": f, "app": app}),
            }),
            ["list"] => call("flow-theme", "theme.list", Value::Null),
            ["set", t] => call(
                "flow-settings",
                "settings.set",
                json!({"key": "appearance.theme", "value": t}),
            ),
            ["iconpack", id] => call(
                "flow-settings",
                "settings.set",
                json!({"key": "appearance.icon_pack", "value": id}),
            ),
            ["icon", n] => call("flow-theme", "theme.icon", json!({"name": n})),
            ["icon", n, s] => call(
                "flow-theme",
                "theme.icon",
                json!({"name": n, "size": s.parse::<u32>().map_err(|e| e.to_string())?}),
            ),
            ["seed", c] => call("flow-theme", "theme.set_seed", json!({"seed": c})),
            _ => Err("usage: flow theme get|tokens|css|list|set|iconpack|icon|seed".into()),
        },
        ["apps", rest @ ..] => match rest {
            [] | ["list"] => call("flow-apps", "apps.list", json!({})),
            ["list", q] => call("flow-apps", "apps.search", json!({"query": q})),
            ["launch", id, uris @ ..] => {
                call("flow-apps", "apps.launch", json!({"id": id, "uris": uris}))
            }
            ["recent"] => call("flow-apps", "apps.recent", Value::Null),
            ["frequent"] => call("flow-apps", "apps.frequent", Value::Null),
            ["running"] => call("flow-apps", "apps.running", Value::Null),
            _ => Err("usage: flow apps list|launch|recent|frequent|running".into()),
        },
        ["lock", rest @ ..] => match rest {
            [] | ["status"] => call("kiln-lock", "lock.status", Value::Null),
            ["lock"] => call("kiln-lock", "lock.lock", Value::Null),
            ["unlock", p] => call("kiln-lock", "lock.unlock", json!({"pin": p})),
            ["set-pin", n] => call("kiln-lock", "lock.set_passcode", json!({"new": n})),
            ["set-pin", n, c] => call(
                "kiln-lock",
                "lock.set_passcode",
                json!({"new": n, "current": c}),
            ),
            _ => Err("usage: flow lock status|lock|unlock PIN|set-pin NEW [CURRENT]".into()),
        },
        ["sensors", ..] => call("flow-sensors", "sensors.status", Value::Null),
        ["haptic", p] => call("flow-haptics", "haptics.play", json!({"pattern": p})),
        ["media", "players"] => call("flow-media", "media.players", Value::Null),
        ["media", "active"] => call("flow-media", "media.active", Value::Null),
        ["media", "key", k] => call("flow-media", "media.key", json!({"key": k})),
        ["wall", "get"] => call("kiln-wall", "wall.get", Value::Null),
        ["wall", "next"] => call("kiln-wall", "wall.next", Value::Null),
        ["clock"] | ["clock", "list"] => call("flow-clock", "clock.list", Value::Null),
        ["clock", "timer", s] => call(
            "flow-clock",
            "clock.timer.start",
            json!({"seconds": s.parse::<u64>().map_err(|e| e.to_string())?}),
        ),
        ["clock", "timer", s, l] => call(
            "flow-clock",
            "clock.timer.start",
            json!({"seconds": s.parse::<u64>().map_err(|e| e.to_string())?, "label": l}),
        ),
        ["ai", "status"] => call("noxal-ai", "ai.status", Value::Null),
        ["ai", "chat", t] => call("noxal-ai", "ai.chat", json!({"text": t})),
        [other, ..] => Err(format!("unknown command {other:?} (try `flow help`)")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Result<Cmd, String> {
        parse(&s.split_whitespace().map(String::from).collect::<Vec<_>>())
    }

    #[test]
    fn settings_value_parsing() {
        assert_eq!(
            p("settings set display.brightness 0.5").unwrap(),
            Cmd::Call {
                daemon: "flow-settings".into(),
                method: "settings.set".into(),
                params: json!({"key": "display.brightness", "value": 0.5})
            }
        );
        assert_eq!(
            p("settings set appearance.scheme dark").unwrap(),
            Cmd::Call {
                daemon: "flow-settings".into(),
                method: "settings.set".into(),
                params: json!({"key": "appearance.scheme", "value": "dark"})
            }
        );
        assert!(
            matches!(p("settings set display.auto_rotate true").unwrap(), Cmd::Call { params, .. } if params["value"] == json!(true))
        );
        assert!(p("settings").is_err());
    }

    #[test]
    fn simple_mappings() {
        assert_eq!(p("status").unwrap(), Cmd::Status);
        assert_eq!(p("").unwrap(), Cmd::Help);
        assert!(
            matches!(p("net airplane on").unwrap(), Cmd::Call { method, params, .. } if method == "net.airplane.set" && params["on"] == json!(true))
        );
        assert!(p("net airplane maybe").is_err());
        assert!(
            matches!(p("notify dnd for 30").unwrap(), Cmd::Call { params, .. } if params["minutes"] == json!(30))
        );
        assert!(p("notify dismiss x").is_err());
        assert!(
            matches!(p("theme set material-you").unwrap(), Cmd::Call { daemon, params, .. } if daemon == "flow-settings" && params["value"] == json!("material-you"))
        );
        assert!(matches!(p("theme css vars").unwrap(), Cmd::CallText { .. }));
        assert!(
            matches!(p("lock unlock 1234").unwrap(), Cmd::Call { daemon, .. } if daemon == "kiln-lock")
        );
        assert!(
            matches!(p("haptic tap").unwrap(), Cmd::Call { daemon, .. } if daemon == "flow-haptics")
        );
        assert!(p("bogus").is_err());
    }

    #[test]
    fn apps_launch_with_uris() {
        assert!(
            matches!(p("apps launch a.desktop x y").unwrap(), Cmd::Call { params, .. } if params["uris"] == json!(["x", "y"]))
        );
    }

    #[test]
    fn raw_ipc() {
        assert_eq!(
            p("ipc flow-apps apps.list {\"query\":\"f\"}").unwrap(),
            Cmd::Call {
                daemon: "flow-apps".into(),
                method: "apps.list".into(),
                params: json!({"query": "f"})
            }
        );
        assert_eq!(
            p("ipc x ping").unwrap(),
            Cmd::Call {
                daemon: "x".into(),
                method: "ping".into(),
                params: Value::Null
            }
        );
        assert!(p("ipc x").is_err());
    }

    #[test]
    fn daemon_list_has_unique_names() {
        let mut v = DAEMONS.to_vec();
        v.sort();
        v.dedup();
        assert_eq!(v.len(), DAEMONS.len());
    }
}
