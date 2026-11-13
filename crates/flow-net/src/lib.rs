//! Network policy logic: airplane mode and data-usage accounting.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Radios {
    pub wifi: bool,
    pub cellular: bool,
}

/// Airplane mode remembers which radios were on and restores exactly those.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Airplane {
    pub active: bool,
    saved: Option<Radios>,
}

impl Airplane {
    /// Enter airplane mode: returns the radio state to apply (all off).
    pub fn enable(&mut self, current: Radios) -> Radios {
        if !self.active {
            self.saved = Some(current);
            self.active = true;
        }
        Radios::default()
    }

    /// Leave: returns the radios to restore (what was on before).
    pub fn disable(&mut self) -> Radios {
        let r = self.saved.take().unwrap_or(Radios {
            wifi: true,
            cellular: true,
        });
        self.active = false;
        r
    }

    /// While active the user may re-enable Wi-Fi alone (common phone behaviour):
    /// remember that choice so leaving airplane mode keeps it.
    pub fn user_enabled_wifi(&mut self) {
        if let Some(s) = &mut self.saved {
            s.wifi = true;
        }
    }
}

/// Per-interface cumulative byte counters with reboot/reset detection.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    /// iface -> accumulated bytes (rx, tx) across resets.
    pub total: BTreeMap<String, (u64, u64)>,
    /// iface -> last raw counter reading.
    last: BTreeMap<String, (u64, u64)>,
    /// Counters since `period_start` (unix s), e.g. billing cycle.
    pub period_start: u64,
    period_base: BTreeMap<String, (u64, u64)>,
}

impl Usage {
    /// Record a reading. If a counter went down (reboot, iface reset) the new
    /// value is treated as fresh traffic since the reset.
    pub fn record(&mut self, iface: &str, rx: u64, tx: u64) {
        let last = self.last.get(iface).copied();
        let (drx, dtx) = match last {
            Some((lr, lt)) => (
                if rx >= lr { rx - lr } else { rx },
                if tx >= lt { tx - lt } else { tx },
            ),
            None => (0, 0), // first sighting: no baseline, count nothing
        };
        self.last.insert(iface.to_string(), (rx, tx));
        let t = self.total.entry(iface.to_string()).or_default();
        t.0 += drx;
        t.1 += dtx;
    }

    /// Start a new billing period now.
    pub fn reset_period(&mut self, now: u64) {
        self.period_start = now;
        self.period_base = self.total.clone();
    }

    pub fn period(&self, iface: &str) -> (u64, u64) {
        let t = self.total.get(iface).copied().unwrap_or_default();
        let b = self.period_base.get(iface).copied().unwrap_or_default();
        (t.0.saturating_sub(b.0), t.1.saturating_sub(b.1))
    }
}

/// Classify an interface by name: cellular modems vs wifi vs other (excluding lo/virtual).
pub fn iface_kind(name: &str) -> Option<&'static str> {
    if name == "lo"
        || name.starts_with("veth")
        || name.starts_with("docker")
        || name.starts_with("br-")
        || name.starts_with("virbr")
    {
        return None;
    }
    if name.starts_with("wwan")
        || name.starts_with("rmnet")
        || name.starts_with("ccmni")
        || name.starts_with("usb")
    {
        Some("cellular")
    } else if name.starts_with("wl") {
        Some("wifi")
    } else {
        Some("other")
    }
}

/// Read `<root>/class/net/*/statistics/{rx,tx}_bytes`.
pub fn read_counters(root: &Path) -> Vec<(String, u64, u64)> {
    let Ok(rd) = std::fs::read_dir(root.join("class/net")) else {
        return vec![];
    };
    let mut v: Vec<_> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            iface_kind(&name)?;
            let st = e.path().join("statistics");
            let n = |f: &str| {
                std::fs::read_to_string(st.join(f))
                    .ok()?
                    .trim()
                    .parse::<u64>()
                    .ok()
            };
            Some((name, n("rx_bytes")?, n("tx_bytes")?))
        })
        .collect();
    v.sort();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn airplane_restores_previous() {
        let mut a = Airplane::default();
        let applied = a.enable(Radios {
            wifi: true,
            cellular: false,
        });
        assert_eq!(applied, Radios::default());
        assert!(a.active);
        // double enable must not overwrite the saved state
        a.enable(Radios::default());
        assert_eq!(
            a.disable(),
            Radios {
                wifi: true,
                cellular: false
            }
        );
        assert!(!a.active);
    }

    #[test]
    fn airplane_user_wifi_choice_kept() {
        let mut a = Airplane::default();
        a.enable(Radios {
            wifi: false,
            cellular: true,
        });
        a.user_enabled_wifi();
        assert_eq!(
            a.disable(),
            Radios {
                wifi: true,
                cellular: true
            }
        );
    }

    #[test]
    fn usage_deltas_and_reset_detection() {
        let mut u = Usage::default();
        u.record("wlan0", 1000, 500); // baseline
        u.record("wlan0", 1600, 700);
        assert_eq!(u.total["wlan0"], (600, 200));
        u.record("wlan0", 50, 10); // reboot: counters restart
        assert_eq!(u.total["wlan0"], (650, 210));
        u.reset_period(99);
        u.record("wlan0", 150, 10);
        assert_eq!(u.period("wlan0"), (100, 0));
        assert_eq!(u.period("none"), (0, 0));
    }

    #[test]
    fn iface_classes_and_sysfs() {
        assert_eq!(iface_kind("wlan0"), Some("wifi"));
        assert_eq!(iface_kind("wwan0"), Some("cellular"));
        assert_eq!(iface_kind("lo"), None);
        let d = std::env::temp_dir().join(format!("flow-net-{}", std::process::id()));
        let s = d.join("class/net/wlan0/statistics");
        std::fs::create_dir_all(&s).unwrap();
        std::fs::write(s.join("rx_bytes"), "10\n").unwrap();
        std::fs::write(s.join("tx_bytes"), "20\n").unwrap();
        std::fs::create_dir_all(d.join("class/net/lo/statistics")).unwrap();
        assert_eq!(read_counters(&d), vec![("wlan0".to_string(), 10, 20)]);
        std::fs::remove_dir_all(d).unwrap();
    }
}
