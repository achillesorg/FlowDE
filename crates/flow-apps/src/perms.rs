//! Per-app permission state. This daemon stores and serves decisions; the
//! enforcement points (portals, compositor, sensors) query it.

use flow_common::{FlowError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const KNOWN: &[&str] = &[
    "camera",
    "microphone",
    "location",
    "notifications",
    "background",
    "storage",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Grant {
    #[default]
    Ask,
    Allow,
    Deny,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Permissions {
    pub apps: BTreeMap<String, BTreeMap<String, Grant>>,
}

impl Permissions {
    pub fn get(&self, app: &str, perm: &str) -> Grant {
        self.apps
            .get(app)
            .and_then(|m| m.get(perm))
            .copied()
            .unwrap_or_default()
    }

    pub fn set(&mut self, app: &str, perm: &str, g: Grant) -> Result<()> {
        if !KNOWN.contains(&perm) {
            return Err(FlowError::Invalid(format!("unknown permission {perm}")));
        }
        let m = self.apps.entry(app.to_string()).or_default();
        if g == Grant::Ask {
            m.remove(perm);
        } else {
            m.insert(perm.to_string(), g);
        }
        if m.is_empty() {
            self.apps.remove(app);
        }
        Ok(())
    }

    pub fn of(&self, app: &str) -> BTreeMap<String, Grant> {
        KNOWN
            .iter()
            .map(|p| (p.to_string(), self.get(app, p)))
            .collect()
    }

    pub fn forget(&mut self, app: &str) {
        self.apps.remove(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_ask_and_roundtrip() {
        let mut p = Permissions::default();
        assert_eq!(p.get("a", "camera"), Grant::Ask);
        p.set("a", "camera", Grant::Deny).unwrap();
        assert_eq!(p.get("a", "camera"), Grant::Deny);
        assert_eq!(p.of("a")["microphone"], Grant::Ask);
        p.set("a", "camera", Grant::Ask).unwrap();
        assert!(p.apps.is_empty());
        assert!(p.set("a", "teleport", Grant::Allow).is_err());
    }
}
