//! Safe tool layer: a fixed allowlist of system actions. The model can only
//! *request* a tool; arguments are validated here, mapped to a control-socket
//! call, and each tool needs a permission (ask / allow / deny).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub const TOOLS: &[&str] = &[
    "open_app",
    "set_setting",
    "notify",
    "timer",
    "calendar_query",
];

/// Settings the assistant may change. Security-relevant keys (lock, power
/// policy, accessibility off-switches) are deliberately absent.
pub const SETTING_PREFIXES: &[&str] = &[
    "appearance.",
    "display.brightness",
    "display.auto_rotate",
    "haptics.",
    "notifications.dnd",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ToolError {
    Unknown(String),
    BadArgs(String),
    Forbidden(String),
}

/// Extract `<tool_call>{json}</tool_call>` blocks (Qwen's tool-call format).
/// At most 4 calls per reply; malformed JSON is ignored.
pub fn parse_tool_calls(text: &str) -> Vec<ToolCall> {
    let mut out = vec![];
    let mut rest = text;
    while let Some(i) = rest.find("<tool_call>") {
        let after = &rest[i + "<tool_call>".len()..];
        let Some(j) = after.find("</tool_call>") else {
            break;
        };
        if let Ok(c) = serde_json::from_str::<ToolCall>(after[..j].trim()) {
            out.push(c);
            if out.len() == 4 {
                break;
            }
        }
        rest = &after[j + "</tool_call>".len()..];
    }
    out
}

/// Text with tool-call blocks removed (what the user sees).
pub fn strip_tool_calls(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find("<tool_call>") {
        out.push_str(&rest[..i]);
        match rest[i..].find("</tool_call>") {
            Some(j) => rest = &rest[i + j + "</tool_call>".len()..],
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out.trim().to_string()
}

fn str_arg<'a>(a: &'a Value, k: &str, max: usize) -> Result<&'a str, ToolError> {
    let s = a[k]
        .as_str()
        .ok_or_else(|| ToolError::BadArgs(format!("{k} must be a string")))?;
    if s.is_empty() || s.len() > max {
        return Err(ToolError::BadArgs(format!("{k} length must be 1..{max}")));
    }
    Ok(s)
}

/// Validate a call and map it to (daemon, method, params).
pub fn to_request(c: &ToolCall) -> Result<(&'static str, &'static str, Value), ToolError> {
    let a = &c.arguments;
    match c.name.as_str() {
        "open_app" => {
            let id = str_arg(a, "id", 200)?;
            if !id
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_' | '+'))
            {
                return Err(ToolError::BadArgs("invalid app id".into()));
            }
            Ok(("flow-apps", "apps.launch", json!({"id": id})))
        }
        "set_setting" => {
            let key = str_arg(a, "key", 100)?;
            if !SETTING_PREFIXES.iter().any(|p| key.starts_with(p)) {
                return Err(ToolError::Forbidden(format!(
                    "setting {key} may not be changed by the assistant"
                )));
            }
            if a.get("value").is_none() || a["value"].is_object() || a["value"].is_array() {
                return Err(ToolError::BadArgs("value must be a scalar".into()));
            }
            Ok((
                "flow-settings",
                "settings.set",
                json!({"key": key, "value": a["value"]}),
            ))
        }
        "notify" => {
            let s = str_arg(a, "summary", 200)?;
            let b = a["body"].as_str().unwrap_or("");
            if b.len() > 1000 {
                return Err(ToolError::BadArgs("body too long".into()));
            }
            Ok((
                "kiln-notifs",
                "notify.post",
                json!({"app": "Noxal", "summary": s, "body": b}),
            ))
        }
        "timer" => {
            let secs = a["seconds"]
                .as_u64()
                .filter(|s| (1..=86_400).contains(s))
                .ok_or_else(|| ToolError::BadArgs("seconds must be 1..86400".into()))?;
            let label = a["label"].as_str().unwrap_or("");
            if label.len() > 100 {
                return Err(ToolError::BadArgs("label too long".into()));
            }
            Ok((
                "flow-clock",
                "clock.timer.start",
                json!({"seconds": secs, "label": label}),
            ))
        }
        "calendar_query" => {
            let (f, t) = (a["from"].as_i64(), a["to"].as_i64());
            match (f, t) {
                (Some(f), Some(t)) if t >= f && t - f <= 31 * 86_400 => Ok((
                    "flow-calendar",
                    "calendar.range",
                    json!({"from": f, "to": t}),
                )),
                _ => Err(ToolError::BadArgs(
                    "from/to unix seconds, at most 31 days".into(),
                )),
            }
        }
        other => Err(ToolError::Unknown(other.to_string())),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    #[default]
    Ask,
    Allow,
    Deny,
}

/// Read-only tools default to Allow-able; everything that changes state defaults to Ask.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Policy {
    pub tools: BTreeMap<String, Permission>,
}

impl Policy {
    pub fn get(&self, tool: &str) -> Permission {
        self.tools.get(tool).copied().unwrap_or_default()
    }
    pub fn set(&mut self, tool: &str, p: Permission) -> Result<(), String> {
        if !TOOLS.contains(&tool) {
            return Err(format!("unknown tool {tool}"));
        }
        self.tools.insert(tool.to_string(), p);
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Pending {
    pub id: u32,
    pub call: ToolCall,
}

/// Pending permission prompts.
#[derive(Debug, Default)]
pub struct Approvals {
    next: u32,
    pending: Vec<Pending>,
}

impl Approvals {
    pub fn ask(&mut self, call: ToolCall) -> u32 {
        self.next += 1;
        self.pending.push(Pending {
            id: self.next,
            call,
        });
        if self.pending.len() > 20 {
            self.pending.remove(0);
        }
        self.next
    }
    pub fn pending(&self) -> &[Pending] {
        &self.pending
    }
    pub fn take(&mut self, id: u32) -> Option<ToolCall> {
        let i = self.pending.iter().position(|p| p.id == id)?;
        Some(self.pending.remove(i).call)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, args: Value) -> ToolCall {
        ToolCall {
            name: name.into(),
            arguments: args,
        }
    }

    #[test]
    fn parses_qwen_tool_calls() {
        let t = "Sure.\n<tool_call>\n{\"name\": \"timer\", \"arguments\": {\"seconds\": 300}}\n</tool_call> done <tool_call>not json</tool_call>";
        let c = parse_tool_calls(t);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].name, "timer");
        assert_eq!(strip_tool_calls(t), "Sure.\n done");
        assert!(parse_tool_calls("<tool_call>{\"name\":\"x\"").is_empty());
    }

    #[test]
    fn allowlist_enforced() {
        assert!(matches!(
            to_request(&call("rm_rf", json!({}))),
            Err(ToolError::Unknown(_))
        ));
        assert!(matches!(
            to_request(&call(
                "set_setting",
                json!({"key": "lock.auto_lock_s", "value": 0})
            )),
            Err(ToolError::Forbidden(_))
        ));
        assert!(matches!(
            to_request(&call(
                "set_setting",
                json!({"key": "power.screen_off_s", "value": 600})
            )),
            Err(ToolError::Forbidden(_))
        ));
        assert!(to_request(&call(
            "set_setting",
            json!({"key": "appearance.scheme", "value": "dark"})
        ))
        .is_ok());
        assert!(matches!(
            to_request(&call(
                "set_setting",
                json!({"key": "appearance.scheme", "value": {"a": 1}})
            )),
            Err(ToolError::BadArgs(_))
        ));
    }

    #[test]
    fn argument_validation() {
        assert_eq!(
            to_request(&call("open_app", json!({"id": "org.kiln.Files.desktop"})))
                .unwrap()
                .1,
            "apps.launch"
        );
        assert!(to_request(&call("open_app", json!({"id": "x; reboot"}))).is_err());
        assert!(to_request(&call("open_app", json!({}))).is_err());
        assert!(to_request(&call("timer", json!({"seconds": 0}))).is_err());
        assert!(to_request(&call("timer", json!({"seconds": 90000}))).is_err());
        assert_eq!(
            to_request(&call("timer", json!({"seconds": 60, "label": "tea"})))
                .unwrap()
                .0,
            "flow-clock"
        );
        assert!(to_request(&call("calendar_query", json!({"from": 0, "to": 99999999}))).is_err());
        assert!(to_request(&call("calendar_query", json!({"from": 0, "to": 86400}))).is_ok());
        assert!(to_request(&call("notify", json!({"summary": "hi"}))).is_ok());
        assert!(to_request(&call("notify", json!({"summary": ""}))).is_err());
    }

    #[test]
    fn policy_and_approvals() {
        let mut p = Policy::default();
        assert_eq!(p.get("timer"), Permission::Ask);
        p.set("timer", Permission::Allow).unwrap();
        assert_eq!(p.get("timer"), Permission::Allow);
        assert!(p.set("format_disk", Permission::Allow).is_err());
        let mut a = Approvals::default();
        let id = a.ask(call("timer", json!({"seconds": 5})));
        assert_eq!(a.pending().len(), 1);
        assert_eq!(a.take(id).unwrap().name, "timer");
        assert!(a.take(id).is_none());
    }
}
