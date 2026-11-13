//! KilnDesktop (DeX-like) backend logic: dock detection with debounce, mode
//! policy, and the handover state machine. No UI. The Glaze session start and
//! input redirection are done through hooks (commands / events) the compositor
//! implements; see docs/ARCHITECTURE.md.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Policy {
    /// Start desktop mode as soon as a dock is detected.
    Auto,
    /// Emit a prompt event and wait for `desktop.answer`.
    #[default]
    Ask,
    Never,
}

/// External connectors currently `connected` according to
/// `<sys>/class/drm/card*-<NAME>/status`. Built-in panels (eDP, DSI, LVDS,
/// Virtual) are not external.
pub fn external_connectors(sys: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let Ok(rd) = std::fs::read_dir(sys.join("class/drm")) else {
        return out;
    };
    for e in rd.flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        let Some((_, conn)) = n.split_once('-') else {
            continue;
        };
        if conn.starts_with("eDP")
            || conn.starts_with("DSI")
            || conn.starts_with("LVDS")
            || conn.starts_with("Virtual")
        {
            continue;
        }
        if std::fs::read_to_string(e.path().join("status")).is_ok_and(|s| s.trim() == "connected") {
            out.insert(conn.to_string());
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Mode {
    Phone,
    /// A dock was seen; waiting for the debounce to elapse.
    Settling {
        output: String,
        since: u64,
    },
    /// Waiting for the user's answer (policy ask).
    Asking {
        output: String,
    },
    Desktop {
        output: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    None,
    Prompt { output: String },
    StartDesktop { output: String },
    StopDesktop { output: String },
}

pub const DEBOUNCE_S: u64 = 2;

pub struct Controller {
    pub policy: Policy,
    mode: Mode,
    /// Remember a "no" for this dock until it is unplugged.
    declined: Option<String>,
    /// Require charging (docks supply power); None = unknown/ignored.
    pub require_power: bool,
}

impl Controller {
    pub fn new(policy: Policy) -> Self {
        Self {
            policy,
            mode: Mode::Phone,
            declined: None,
            require_power: false,
        }
    }
    pub fn mode(&self) -> &Mode {
        &self.mode
    }

    /// Feed the connector set (and charging state). Returns the action to perform.
    pub fn update(&mut self, connectors: &BTreeSet<String>, charging: bool, now: u64) -> Action {
        let first = connectors.iter().next().cloned();
        let usable = first.filter(|_| !self.require_power || charging);
        match (&self.mode.clone(), usable) {
            (Mode::Phone, Some(o))
                if self.policy != Policy::Never && self.declined.as_deref() != Some(o.as_str()) =>
            {
                self.mode = Mode::Settling {
                    output: o,
                    since: now,
                };
                Action::None
            }
            (Mode::Phone, None) => {
                self.declined = None;
                Action::None
            }
            (Mode::Settling { output, since }, Some(o)) if &o == output => {
                if now < since + DEBOUNCE_S {
                    return Action::None;
                }
                match self.policy {
                    Policy::Auto => {
                        self.mode = Mode::Desktop { output: o.clone() };
                        Action::StartDesktop { output: o }
                    }
                    Policy::Ask => {
                        self.mode = Mode::Asking { output: o.clone() };
                        Action::Prompt { output: o }
                    }
                    Policy::Never => {
                        self.mode = Mode::Phone;
                        Action::None
                    }
                }
            }
            (Mode::Settling { .. }, Some(o)) => {
                self.mode = Mode::Settling {
                    output: o,
                    since: now,
                };
                Action::None
            }
            (Mode::Settling { .. } | Mode::Asking { .. }, None) => {
                self.mode = Mode::Phone;
                Action::None
            }
            (Mode::Desktop { output }, None) => {
                let o = output.clone();
                self.mode = Mode::Phone;
                Action::StopDesktop { output: o }
            }
            // The docked output changed (cable moved to another port).
            (Mode::Desktop { output }, Some(o)) if &o != output => {
                let old = output.clone();
                self.mode = Mode::Settling {
                    output: o,
                    since: now,
                };
                Action::StopDesktop { output: old }
            }
            _ => Action::None,
        }
    }

    /// The user answered the prompt.
    pub fn answer(&mut self, yes: bool) -> Action {
        match self.mode.clone() {
            Mode::Asking { output } if yes => {
                self.mode = Mode::Desktop {
                    output: output.clone(),
                };
                Action::StartDesktop { output }
            }
            Mode::Asking { output } => {
                self.declined = Some(output);
                self.mode = Mode::Phone;
                Action::None
            }
            _ => Action::None,
        }
    }

    /// Manual toggle (settings button), ignoring dock policy.
    pub fn force(&mut self, on: bool, connectors: &BTreeSet<String>) -> Action {
        match (on, self.mode.clone(), connectors.iter().next()) {
            (true, Mode::Desktop { .. }, _) => Action::None,
            (true, _, Some(o)) => {
                self.mode = Mode::Desktop { output: o.clone() };
                Action::StartDesktop { output: o.clone() }
            }
            (false, Mode::Desktop { output }, _) => {
                self.mode = Mode::Phone;
                Action::StopDesktop { output }
            }
            _ => Action::None,
        }
    }
}

/// Build the argv that starts the Glaze session on `output`. The template is
/// split on whitespace (no shell); `{output}` is substituted only if the
/// output name is a safe connector name.
pub fn launch_argv(template: &str, output: &str) -> Option<Vec<String>> {
    if output.is_empty()
        || !output
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }
    let v: Vec<String> = template
        .split_whitespace()
        .map(|t| t.replace("{output}", output))
        .collect();
    (!v.is_empty()).then_some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn auto_policy_debounces() {
        let mut c = Controller::new(Policy::Auto);
        assert_eq!(c.update(&set(&["DP-1"]), true, 10), Action::None);
        assert_eq!(c.update(&set(&["DP-1"]), true, 11), Action::None); // still settling
        assert_eq!(
            c.update(&set(&["DP-1"]), true, 12),
            Action::StartDesktop {
                output: "DP-1".into()
            }
        );
        assert_eq!(c.update(&set(&["DP-1"]), true, 13), Action::None);
        assert_eq!(
            c.update(&set(&[]), true, 14),
            Action::StopDesktop {
                output: "DP-1".into()
            }
        );
        assert_eq!(*c.mode(), Mode::Phone);
    }

    #[test]
    fn flapping_cable_never_starts() {
        let mut c = Controller::new(Policy::Auto);
        c.update(&set(&["HDMI-A-1"]), true, 0);
        c.update(&set(&[]), true, 1);
        c.update(&set(&["HDMI-A-1"]), true, 2);
        assert_eq!(c.update(&set(&["HDMI-A-1"]), true, 3), Action::None);
    }

    #[test]
    fn ask_policy_and_decline_memory() {
        let mut c = Controller::new(Policy::Ask);
        c.update(&set(&["DP-1"]), true, 0);
        assert_eq!(
            c.update(&set(&["DP-1"]), true, 2),
            Action::Prompt {
                output: "DP-1".into()
            }
        );
        assert_eq!(c.answer(false), Action::None);
        // declined: not asked again until unplugged
        c.update(&set(&["DP-1"]), true, 5);
        assert_eq!(c.update(&set(&["DP-1"]), true, 9), Action::None);
        c.update(&set(&[]), true, 10);
        c.update(&set(&["DP-1"]), true, 11);
        assert!(matches!(
            c.update(&set(&["DP-1"]), true, 13),
            Action::Prompt { .. }
        ));
        assert_eq!(
            c.answer(true),
            Action::StartDesktop {
                output: "DP-1".into()
            }
        );
    }

    #[test]
    fn never_and_power_requirement() {
        let mut n = Controller::new(Policy::Never);
        n.update(&set(&["DP-1"]), true, 0);
        assert_eq!(n.update(&set(&["DP-1"]), true, 9), Action::None);
        let mut p = Controller::new(Policy::Auto);
        p.require_power = true;
        p.update(&set(&["DP-1"]), false, 0);
        assert_eq!(p.update(&set(&["DP-1"]), false, 9), Action::None);
    }

    #[test]
    fn force_toggle() {
        let mut c = Controller::new(Policy::Never);
        assert_eq!(c.force(true, &set(&[])), Action::None);
        assert_eq!(
            c.force(true, &set(&["DP-2"])),
            Action::StartDesktop {
                output: "DP-2".into()
            }
        );
        assert_eq!(
            c.force(false, &set(&["DP-2"])),
            Action::StopDesktop {
                output: "DP-2".into()
            }
        );
    }

    #[test]
    fn sysfs_connectors() {
        let d = std::env::temp_dir().join(format!("flow-dock-{}", std::process::id()));
        for (n, s) in [
            ("card0-eDP-1", "connected"),
            ("card0-DP-1", "connected"),
            ("card0-HDMI-A-1", "disconnected"),
            ("card0-DSI-1", "connected"),
        ] {
            let p = d.join("class/drm").join(n);
            std::fs::create_dir_all(&p).unwrap();
            std::fs::write(p.join("status"), s).unwrap();
        }
        assert_eq!(external_connectors(&d), set(&["DP-1"]));
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn launch_argv_safe() {
        assert_eq!(
            launch_argv("glaze-session --output {output}", "DP-1").unwrap(),
            vec!["glaze-session", "--output", "DP-1"]
        );
        assert!(launch_argv("x {output}", "DP-1; rm").is_none());
        assert!(launch_argv("", "DP-1").is_none());
    }
}
