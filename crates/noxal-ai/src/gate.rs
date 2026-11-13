//! Resource gating: only run when battery, thermal and RAM allow.

use flow_common::events::{PowerSnapshot, Thermal};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub struct GateConfig {
    pub min_battery_pct: f32,
    pub allow_in_low_power: bool,
    /// RAM that must stay free on top of the model's need.
    pub ram_headroom_mb: u64,
    pub idle_unload_s: u64,
}

impl Default for GateConfig {
    fn default() -> Self {
        Self {
            min_battery_pct: 30.0,
            allow_in_low_power: false,
            ram_headroom_mb: 512,
            idle_unload_s: 120,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Denied {
    Battery(u32),
    Thermal,
    LowPower,
    Ram { need_mb: u64, free_mb: u64 },
}

impl std::fmt::Display for Denied {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Denied::Battery(p) => write!(f, "battery too low ({p}%)"),
            Denied::Thermal => write!(f, "device too warm"),
            Denied::LowPower => write!(f, "low power mode is on"),
            Denied::Ram { need_mb, free_mb } => write!(
                f,
                "not enough free memory (need {need_mb} MB, have {free_mb} MB)"
            ),
        }
    }
}

/// `power` = None when flow-power is unreachable: only RAM is checked then.
pub fn check(
    cfg: &GateConfig,
    power: Option<&PowerSnapshot>,
    free_ram_mb: u64,
    model_ram_mb: u64,
) -> Result<(), Denied> {
    if let Some(p) = power {
        if matches!(p.thermal, Thermal::Warm | Thermal::Hot | Thermal::Critical) {
            return Err(Denied::Thermal);
        }
        if p.low_power && !cfg.allow_in_low_power && !p.charging {
            return Err(Denied::LowPower);
        }
        if let (Some(b), false) = (p.battery_pct, p.charging) {
            if b < cfg.min_battery_pct {
                return Err(Denied::Battery(b as u32));
            }
        }
    }
    let need = model_ram_mb + cfg.ram_headroom_mb;
    if free_ram_mb < need {
        return Err(Denied::Ram {
            need_mb: need,
            free_mb: free_ram_mb,
        });
    }
    Ok(())
}

/// MemAvailable from /proc/meminfo text, in MB.
pub fn mem_available_mb(meminfo: &str) -> Option<u64> {
    meminfo
        .lines()
        .find_map(|l| l.strip_prefix("MemAvailable:"))
        .and_then(|v| v.split_whitespace().next()?.parse::<u64>().ok())
        .map(|kb| kb / 1024)
}

/// Unload bookkeeping.
#[derive(Debug, Default)]
pub struct IdleUnloader {
    last_used: u64,
    loaded: bool,
}

impl IdleUnloader {
    pub fn used(&mut self, now: u64) {
        self.last_used = now;
        self.loaded = true;
    }
    /// True exactly once when the idle timeout passes.
    pub fn should_unload(&mut self, now: u64, idle_s: u64) -> bool {
        if self.loaded && now >= self.last_used + idle_s {
            self.loaded = false;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(b: f32, charging: bool, th: Thermal, lp: bool) -> PowerSnapshot {
        PowerSnapshot {
            battery_pct: Some(b),
            charging,
            thermal: th,
            low_power: lp,
        }
    }

    #[test]
    fn gating_rules() {
        let c = GateConfig::default();
        assert_eq!(
            check(
                &c,
                Some(&snap(80.0, false, Thermal::Normal, false)),
                4000,
                2000
            ),
            Ok(())
        );
        assert_eq!(
            check(
                &c,
                Some(&snap(20.0, false, Thermal::Normal, false)),
                4000,
                2000
            ),
            Err(Denied::Battery(20))
        );
        assert_eq!(
            check(
                &c,
                Some(&snap(20.0, true, Thermal::Normal, false)),
                4000,
                2000
            ),
            Ok(())
        ); // charging
        assert_eq!(
            check(
                &c,
                Some(&snap(80.0, false, Thermal::Warm, false)),
                4000,
                2000
            ),
            Err(Denied::Thermal)
        );
        assert_eq!(
            check(
                &c,
                Some(&snap(80.0, false, Thermal::Normal, true)),
                4000,
                2000
            ),
            Err(Denied::LowPower)
        );
        assert_eq!(
            check(
                &c,
                Some(&snap(80.0, false, Thermal::Normal, false)),
                2400,
                2000
            ),
            Err(Denied::Ram {
                need_mb: 2512,
                free_mb: 2400
            })
        );
        assert_eq!(check(&c, None, 4000, 2000), Ok(()));
        assert!(Denied::Thermal.to_string().contains("warm"));
    }

    #[test]
    fn meminfo() {
        assert_eq!(
            mem_available_mb("MemTotal: 1 kB\nMemAvailable:    2097152 kB\n"),
            Some(2048)
        );
        assert_eq!(mem_available_mb("nothing"), None);
    }

    #[test]
    fn unload_once() {
        let mut u = IdleUnloader::default();
        assert!(!u.should_unload(1000, 10));
        u.used(100);
        assert!(!u.should_unload(105, 10));
        assert!(u.should_unload(110, 10));
        assert!(!u.should_unload(200, 10));
    }
}
