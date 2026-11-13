//! Typed settings store with schema validation.
//!
//! Keys are dotted (`display.brightness`). The schema is TOML (built in plus
//! optional drop-ins). Values persist in `<state>/settings.toml`.

use flow_common::{FlowError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

pub const SCHEMA_TOML: &str = include_str!("schema.toml");
pub const FILE_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Bool,
    Int,
    Float,
    String,
    Enum,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeySpec {
    #[serde(rename = "type")]
    pub kind: Kind,
    pub default: toml::Value,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub choices: Vec<String>,
    /// Named string pattern; only `hex_color` is supported.
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Default)]
pub struct Schema {
    pub keys: BTreeMap<String, KeySpec>,
}

impl Schema {
    /// Parse a schema document: nested tables `[group.key]`.
    pub fn parse(text: &str) -> Result<Schema> {
        let root: toml::Table = text.parse()?;
        let mut keys = BTreeMap::new();
        for (group, v) in root {
            let t = v
                .as_table()
                .ok_or_else(|| FlowError::Config(format!("schema group {group} not a table")))?;
            for (name, spec) in t {
                let spec: KeySpec = spec.clone().try_into()?;
                let full = format!("{group}.{name}");
                // default must itself validate
                validate(&full, &spec, &toml_to_json(&spec.default))
                    .map_err(|e| FlowError::Config(format!("bad default for {full}: {e}")))?;
                keys.insert(full, spec);
            }
        }
        Ok(Schema { keys })
    }

    pub fn builtin() -> Schema {
        Schema::parse(SCHEMA_TOML).expect("builtin schema valid")
    }

    /// Merge another schema; existing keys win (drop-ins cannot override).
    pub fn extend(&mut self, other: Schema) {
        for (k, v) in other.keys {
            self.keys.entry(k).or_insert(v);
        }
    }
}

fn toml_to_json(v: &toml::Value) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

fn json_to_toml(v: &Value) -> toml::Value {
    match v {
        Value::Bool(b) => toml::Value::Boolean(*b),
        Value::Number(n) => match n.as_i64() {
            Some(i) => toml::Value::Integer(i),
            None => toml::Value::Float(n.as_f64().unwrap_or(0.0)),
        },
        Value::String(s) => toml::Value::String(s.clone()),
        _ => toml::Value::String(v.to_string()),
    }
}

/// Validate (and normalise, e.g. int -> float) a JSON value against a spec.
pub fn validate(key: &str, spec: &KeySpec, v: &Value) -> Result<Value> {
    let bad = |m: String| FlowError::Invalid(format!("{key}: {m}"));
    let range = |x: f64| -> Result<()> {
        if let Some(min) = spec.min {
            if x < min {
                return Err(bad(format!("{x} < min {min}")));
            }
        }
        if let Some(max) = spec.max {
            if x > max {
                return Err(bad(format!("{x} > max {max}")));
            }
        }
        Ok(())
    };
    match spec.kind {
        Kind::Bool => v
            .as_bool()
            .map(Value::Bool)
            .ok_or_else(|| bad("expected bool".into())),
        Kind::Int => {
            let i = v.as_i64().ok_or_else(|| bad("expected integer".into()))?;
            range(i as f64)?;
            Ok(Value::from(i))
        }
        Kind::Float => {
            let f = v.as_f64().ok_or_else(|| bad("expected number".into()))?;
            if !f.is_finite() {
                return Err(bad("not finite".into()));
            }
            range(f)?;
            Ok(Value::from(f))
        }
        Kind::String | Kind::Enum => {
            let s = v.as_str().ok_or_else(|| bad("expected string".into()))?;
            if s.len() > 4096 {
                return Err(bad("string too long".into()));
            }
            if spec.kind == Kind::Enum && !spec.choices.iter().any(|c| c == s) {
                return Err(bad(format!("must be one of {:?}", spec.choices)));
            }
            if spec.pattern.as_deref() == Some("hex_color") && !is_hex_color(s) {
                return Err(bad("expected #rrggbb or #rrggbbaa".into()));
            }
            Ok(Value::String(s.to_string()))
        }
    }
}

pub fn is_hex_color(s: &str) -> bool {
    let b = s.strip_prefix('#');
    matches!(b, Some(h) if (h.len() == 6 || h.len() == 8) && h.chars().all(|c| c.is_ascii_hexdigit()))
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Change {
    pub key: String,
    pub value: Value,
}

pub struct Store {
    schema: Schema,
    values: BTreeMap<String, Value>,
    path: Option<PathBuf>,
}

impl Store {
    pub fn new(schema: Schema, path: Option<PathBuf>) -> Result<Store> {
        let mut s = Store {
            schema,
            values: BTreeMap::new(),
            path,
        };
        s.load()?;
        Ok(s)
    }

    fn load(&mut self) -> Result<()> {
        let Some(p) = &self.path else { return Ok(()) };
        let text = match std::fs::read_to_string(p) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        let (t, _): (toml::Table, bool) =
            flow_common::config::parse_versioned(&text, FILE_VERSION, &[])?;
        // Flat table of dotted keys under [values]; invalid/unknown entries are dropped with a log.
        if let Some(vals) = t.get("values").and_then(|v| v.as_table()) {
            for (k, v) in vals {
                match self.schema.keys.get(k) {
                    Some(spec) => match validate(k, spec, &toml_to_json(v)) {
                        Ok(v) => {
                            self.values.insert(k.clone(), v);
                        }
                        Err(e) => tracing::warn!("dropping stored value: {e}"),
                    },
                    None => tracing::warn!("dropping unknown stored key {k}"),
                }
            }
        }
        Ok(())
    }

    fn save(&self) -> Result<()> {
        let Some(p) = &self.path else { return Ok(()) };
        let mut vals = toml::Table::new();
        for (k, v) in &self.values {
            vals.insert(k.clone(), json_to_toml(v));
        }
        let mut root = toml::Table::new();
        root.insert("values".into(), toml::Value::Table(vals));
        let text = format!(
            "version = {FILE_VERSION}\n{}",
            toml::to_string_pretty(&root)?
        );
        flow_common::paths::write_atomic(p, text.as_bytes())?;
        Ok(())
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    pub fn get(&self, key: &str) -> Result<Value> {
        let spec = self
            .schema
            .keys
            .get(key)
            .ok_or_else(|| FlowError::NotFound(key.into()))?;
        Ok(self
            .values
            .get(key)
            .cloned()
            .unwrap_or_else(|| toml_to_json(&spec.default)))
    }

    /// Set a value; returns `Some(Change)` only if the value actually changed.
    pub fn set(&mut self, key: &str, value: &Value) -> Result<Option<Change>> {
        let spec = self
            .schema
            .keys
            .get(key)
            .ok_or_else(|| FlowError::NotFound(key.into()))?;
        let v = validate(key, spec, value)?;
        if self.get(key)? == v {
            return Ok(None);
        }
        let default = toml_to_json(&spec.default);
        if v == default {
            self.values.remove(key);
        } else {
            self.values.insert(key.to_string(), v.clone());
        }
        self.save()?;
        Ok(Some(Change {
            key: key.to_string(),
            value: v,
        }))
    }

    pub fn reset(&mut self, key: &str) -> Result<Option<Change>> {
        let spec = self
            .schema
            .keys
            .get(key)
            .ok_or_else(|| FlowError::NotFound(key.into()))?;
        let d = toml_to_json(&spec.default);
        self.set(key, &d)
    }

    /// All keys with effective values, as a flat JSON object.
    pub fn export(&self) -> Value {
        let mut m = serde_json::Map::new();
        for k in self.schema.keys.keys() {
            m.insert(k.clone(), self.get(k).unwrap_or(Value::Null));
        }
        Value::Object(m)
    }

    /// Import a flat object atomically: nothing is applied if any entry is invalid.
    pub fn import(&mut self, obj: &Value) -> Result<Vec<Change>> {
        let map = obj
            .as_object()
            .ok_or_else(|| FlowError::Invalid("expected object".into()))?;
        for (k, v) in map {
            let spec = self
                .schema
                .keys
                .get(k)
                .ok_or_else(|| FlowError::NotFound(k.clone()))?;
            validate(k, spec, v)?;
        }
        let mut changes = vec![];
        for (k, v) in map {
            if let Some(c) = self.set(k, v)? {
                changes.push(c);
            }
        }
        Ok(changes)
    }

    pub fn list(&self, prefix: &str) -> Vec<Value> {
        self.schema
            .keys
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, s)| {
                serde_json::json!({
                    "key": k, "type": s.kind, "value": self.get(k).unwrap_or(Value::Null),
                    "default": toml_to_json(&s.default), "description": s.description,
                    "choices": s.choices, "min": s.min, "max": s.max,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn store() -> Store {
        Store::new(Schema::builtin(), None).unwrap()
    }

    #[test]
    fn builtin_schema_parses_and_defaults() {
        let s = store();
        assert_eq!(s.get("appearance.scheme").unwrap(), json!("auto"));
        assert_eq!(s.get("display.refresh_rate_hz").unwrap(), json!(60));
    }

    #[test]
    fn validation() {
        let mut s = store();
        assert!(s.set("display.brightness", &json!(1.5)).is_err());
        assert!(s.set("display.brightness", &json!("x")).is_err());
        assert!(s.set("appearance.scheme", &json!("pink")).is_err());
        assert!(s.set("appearance.accent", &json!("red")).is_err());
        assert!(s.set("nope.key", &json!(1)).is_err());
        assert!(s.set("display.brightness", &json!(1)).unwrap().is_some()); // int -> float
        assert_eq!(s.get("display.brightness").unwrap(), json!(1.0));
    }

    #[test]
    fn change_only_when_different() {
        let mut s = store();
        assert!(s
            .set("display.auto_rotate", &json!(true))
            .unwrap()
            .is_none());
        let c = s
            .set("display.auto_rotate", &json!(false))
            .unwrap()
            .unwrap();
        assert_eq!(c.key, "display.auto_rotate");
        assert!(s.reset("display.auto_rotate").unwrap().is_some());
    }

    #[test]
    fn persistence() {
        let d = std::env::temp_dir().join(format!("flow-set-{}", std::process::id()));
        let p = d.join("settings.toml");
        {
            let mut s = Store::new(Schema::builtin(), Some(p.clone())).unwrap();
            s.set("appearance.scheme", &json!("dark")).unwrap();
            s.set("power.screen_off_s", &json!(90)).unwrap();
        }
        let s = Store::new(Schema::builtin(), Some(p)).unwrap();
        assert_eq!(s.get("appearance.scheme").unwrap(), json!("dark"));
        assert_eq!(s.get("power.screen_off_s").unwrap(), json!(90));
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn import_is_atomic() {
        let mut s = store();
        let bad = json!({"appearance.scheme":"dark","display.brightness":9});
        assert!(s.import(&bad).is_err());
        assert_eq!(s.get("appearance.scheme").unwrap(), json!("auto"));
        let good = json!({"appearance.scheme":"dark"});
        assert_eq!(s.import(&good).unwrap().len(), 1);
        let exp = s.export();
        assert_eq!(exp["appearance.scheme"], "dark");
    }

    #[test]
    fn dropin_cannot_override() {
        let mut a = Schema::builtin();
        let b = Schema::parse(
            "[appearance.scheme]\ntype='bool'\ndefault=true\n[x.y]\ntype='bool'\ndefault=false\n",
        )
        .unwrap();
        a.extend(b);
        assert_eq!(a.keys["appearance.scheme"].kind, Kind::Enum);
        assert!(a.keys.contains_key("x.y"));
    }

    #[test]
    fn bad_default_rejected() {
        assert!(Schema::parse("[a.b]\ntype='int'\ndefault=5\nmax=3\n").is_err());
    }

    #[test]
    fn hex_colors() {
        assert!(is_hex_color("#0a84ff"));
        assert!(is_hex_color("#0a84ff80"));
        assert!(!is_hex_color("0a84ff"));
        assert!(!is_hex_color("#12"));
    }
}
