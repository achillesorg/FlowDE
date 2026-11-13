//! Camera backend interface. Capture is TODO(feature): libcamera for the
//! phone ISP cameras and the PipeWire camera portal for apps. This crate
//! provides honest device enumeration (V4L2 sysfs) and the permission-prompt
//! state machine the shell UI answers.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Device {
    pub node: String,
    pub name: String,
    /// Heuristic from the name: "front", "back" or "unknown".
    pub facing: &'static str,
}

pub fn facing_from_name(name: &str) -> &'static str {
    let n = name.to_lowercase();
    if n.contains("front") || n.contains("selfie") || n.contains("user") {
        "front"
    } else if n.contains("back") || n.contains("rear") || n.contains("world") {
        "back"
    } else {
        "unknown"
    }
}

/// List `video4linux/videoN` capture nodes (index 0 of each device; metadata
/// nodes are skipped via `index` file).
pub fn enumerate(sys: &Path) -> Vec<Device> {
    let Ok(rd) = std::fs::read_dir(sys.join("class/video4linux")) else {
        return vec![];
    };
    let mut v: Vec<Device> = rd
        .flatten()
        .filter_map(|e| {
            let node = e.file_name().to_string_lossy().into_owned();
            if !node.starts_with("video") {
                return None;
            }
            let idx = std::fs::read_to_string(e.path().join("index"))
                .ok()
                .and_then(|s| s.trim().parse::<u32>().ok())
                .unwrap_or(0);
            if idx != 0 {
                return None;
            }
            let name = std::fs::read_to_string(e.path().join("name"))
                .ok()?
                .trim()
                .to_string();
            Some(Device {
                facing: facing_from_name(&name),
                node: format!("/dev/{node}"),
                name,
            })
        })
        .collect();
    v.sort_by(|a, b| a.node.cmp(&b.node));
    v
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Grant {
    Allow,
    Deny,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Prompt {
    pub id: u32,
    pub app: String,
    pub since: u64,
}

pub const PROMPT_TTL_S: u64 = 120;

/// Permission state: remembered decisions plus pending prompts.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Permissions {
    next: u32,
    pending: Vec<(u32, String, u64)>,
    pub decided: std::collections::BTreeMap<String, Grant>,
}

#[derive(Debug, PartialEq)]
pub enum Access {
    Granted,
    Denied,
    /// A prompt with this id was queued (or already pending).
    Prompt(u32),
}

impl Permissions {
    /// An app asks for the camera. Decided apps are answered at once; others queue one prompt.
    pub fn request(&mut self, app: &str, now: u64) -> Access {
        match self.decided.get(app) {
            Some(Grant::Allow) => return Access::Granted,
            Some(Grant::Deny) => return Access::Denied,
            None => {}
        }
        self.expire(now);
        if let Some((id, _, _)) = self.pending.iter().find(|p| p.1 == app) {
            return Access::Prompt(*id);
        }
        self.next += 1;
        self.pending.push((self.next, app.to_string(), now));
        Access::Prompt(self.next)
    }

    pub fn pending(&mut self, now: u64) -> Vec<Prompt> {
        self.expire(now);
        self.pending
            .iter()
            .map(|(id, app, since)| Prompt {
                id: *id,
                app: app.clone(),
                since: *since,
            })
            .collect()
    }

    /// The shell answers a prompt; `remember` stores the decision.
    pub fn resolve(&mut self, id: u32, grant: Grant, remember: bool) -> Option<String> {
        let i = self.pending.iter().position(|p| p.0 == id)?;
        let (_, app, _) = self.pending.remove(i);
        if remember {
            self.decided.insert(app.clone(), grant);
        }
        Some(app)
    }

    pub fn revoke(&mut self, app: &str) -> bool {
        self.decided.remove(app).is_some()
    }

    fn expire(&mut self, now: u64) {
        self.pending.retain(|p| now < p.2 + PROMPT_TTL_S);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_flow() {
        let mut p = Permissions::default();
        let Access::Prompt(id) = p.request("cam.app", 0) else {
            panic!()
        };
        assert_eq!(p.request("cam.app", 5), Access::Prompt(id)); // no duplicate
        assert_eq!(p.pending(6).len(), 1);
        assert_eq!(
            p.resolve(id, Grant::Allow, true).as_deref(),
            Some("cam.app")
        );
        assert_eq!(p.request("cam.app", 10), Access::Granted);
        assert!(p.revoke("cam.app"));
        assert!(matches!(p.request("cam.app", 11), Access::Prompt(_)));
    }

    #[test]
    fn deny_and_expire() {
        let mut p = Permissions::default();
        let Access::Prompt(id) = p.request("x", 0) else {
            panic!()
        };
        p.resolve(id, Grant::Deny, true);
        assert_eq!(p.request("x", 1), Access::Denied);
        let Access::Prompt(_) = p.request("y", 0) else {
            panic!()
        };
        assert!(p.pending(PROMPT_TTL_S).is_empty());
        assert_eq!(p.resolve(99, Grant::Allow, false), None);
    }

    #[test]
    fn once_only_not_remembered() {
        let mut p = Permissions::default();
        let Access::Prompt(id) = p.request("z", 0) else {
            panic!()
        };
        p.resolve(id, Grant::Allow, false);
        assert!(matches!(p.request("z", 1), Access::Prompt(_)));
    }

    #[test]
    fn enumerate_sysfs() {
        let d = std::env::temp_dir().join(format!("flow-cam-{}", std::process::id()));
        let v = d.join("class/video4linux");
        for (n, name, idx) in [
            ("video0", "Front Camera", "0"),
            ("video1", "Front Camera", "1"),
            ("video2", "rear_cam", "0"),
        ] {
            std::fs::create_dir_all(v.join(n)).unwrap();
            std::fs::write(v.join(n).join("name"), name).unwrap();
            std::fs::write(v.join(n).join("index"), idx).unwrap();
        }
        let l = enumerate(&d);
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].facing, "front");
        assert_eq!(l[1].facing, "back");
        assert!(enumerate(&d.join("none")).is_empty());
        std::fs::remove_dir_all(d).unwrap();
    }
}
