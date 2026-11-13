//! Pure policy: thermal hysteresis, low-power mode, idle -> action.

use flow_common::events::Thermal;
use serde::{Deserialize, Serialize};

/// Thermal classifier with 3 degC hysteresis when cooling.
#[derive(Debug, Clone)]
pub struct ThermalTracker {
    band: Thermal,
}

const HYST: f32 = 3.0;

fn lower_bound(b: Thermal) -> f32 {
    match b {
        Thermal::Normal => f32::NEG_INFINITY,
        Thermal::Warm => 45.0,
        Thermal::Hot => 55.0,
        Thermal::Critical => 65.0,
    }
}

impl Default for ThermalTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl ThermalTracker {
    pub fn new() -> Self {
        Self {
            band: Thermal::Normal,
        }
    }
    pub fn band(&self) -> Thermal {
        self.band
    }
    pub fn update(&mut self, temp_c: f32) -> Thermal {
        let up = crate::sysfs::thermal_band(temp_c);
        if rank(up) >= rank(self.band) || temp_c < lower_bound(self.band) - HYST {
            self.band = up;
        }
        self.band
    }
}

fn rank(t: Thermal) -> u8 {
    match t {
        Thermal::Normal => 0,
        Thermal::Warm => 1,
        Thermal::Hot => 2,
        Thermal::Critical => 3,
    }
}

/// Low power mode state: automatic on battery threshold, with manual override.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LowPowerSetting {
    #[default]
    Auto,
    On,
    Off,
}

#[derive(Debug, Clone)]
pub struct LowPower {
    pub setting: LowPowerSetting,
    pub threshold_pct: f32,
    auto_active: bool,
}

impl LowPower {
    pub fn new(threshold_pct: f32) -> Self {
        Self {
            setting: LowPowerSetting::Auto,
            threshold_pct,
            auto_active: false,
        }
    }

    /// Returns whether low-power mode is active after this reading.
    /// Auto: enters at <= threshold on battery; leaves when charging or
    /// battery >= threshold + 5 (hysteresis).
    pub fn update(&mut self, battery_pct: Option<f32>, charging: bool) -> bool {
        match (battery_pct, charging) {
            (Some(p), false) if p <= self.threshold_pct => self.auto_active = true,
            (Some(p), _) if p >= self.threshold_pct + 5.0 => self.auto_active = false,
            (_, true) | (None, _) => self.auto_active = false,
            _ => {}
        }
        self.active()
    }

    pub fn active(&self) -> bool {
        match self.setting {
            LowPowerSetting::On => true,
            LowPowerSetting::Off => false,
            LowPowerSetting::Auto => self.auto_active,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IdleAction {
    None,
    Dim,
    ScreenOff,
    Suspend,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct IdleConfig {
    pub dim_after_s: u64,
    pub screen_off_after_s: u64,
    /// 0 disables auto-suspend.
    pub suspend_after_s: u64,
    /// Timers are multiplied by this in low-power mode.
    pub low_power_factor: f32,
    /// Do not auto-suspend while charging.
    pub no_suspend_on_ac: bool,
}

impl Default for IdleConfig {
    fn default() -> Self {
        Self {
            dim_after_s: 20,
            screen_off_after_s: 30,
            suspend_after_s: 300,
            low_power_factor: 0.5,
            no_suspend_on_ac: true,
        }
    }
}

impl IdleConfig {
    fn scale(&self, v: u64, low_power: bool) -> u64 {
        if low_power {
            ((v as f32) * self.low_power_factor).round().max(1.0) as u64
        } else {
            v
        }
    }

    /// Escalating action for `idle_s` seconds of inactivity.
    pub fn action(&self, idle_s: u64, low_power: bool, on_ac: bool, inhibited: bool) -> IdleAction {
        if inhibited {
            return IdleAction::None;
        }
        let suspend = self.suspend_after_s != 0 && !(on_ac && self.no_suspend_on_ac);
        if suspend && idle_s >= self.scale(self.suspend_after_s, low_power) {
            IdleAction::Suspend
        } else if idle_s >= self.scale(self.screen_off_after_s, low_power) {
            IdleAction::ScreenOff
        } else if idle_s >= self.scale(self.dim_after_s, low_power) {
            IdleAction::Dim
        } else {
            IdleAction::None
        }
    }

    /// Seconds until the next threshold above `idle_s` (for timer scheduling).
    pub fn next_threshold(&self, idle_s: u64, low_power: bool, on_ac: bool) -> Option<u64> {
        let mut t = vec![
            self.scale(self.dim_after_s, low_power),
            self.scale(self.screen_off_after_s, low_power),
        ];
        if self.suspend_after_s != 0 && !(on_ac && self.no_suspend_on_ac) {
            t.push(self.scale(self.suspend_after_s, low_power));
        }
        t.into_iter()
            .filter(|x| *x > idle_s)
            .min()
            .map(|x| x - idle_s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thermal_hysteresis() {
        let mut t = ThermalTracker::new();
        assert_eq!(t.update(40.0), Thermal::Normal);
        assert_eq!(t.update(46.0), Thermal::Warm);
        assert_eq!(t.update(44.0), Thermal::Warm); // within hysteresis
        assert_eq!(t.update(41.5), Thermal::Normal);
        assert_eq!(t.update(70.0), Thermal::Critical);
        assert_eq!(t.update(63.0), Thermal::Critical); // 65-3=62 not yet
        assert_eq!(t.update(61.0), Thermal::Hot);
    }

    #[test]
    fn low_power_auto_with_hysteresis() {
        let mut l = LowPower::new(20.0);
        assert!(!l.update(Some(30.0), false));
        assert!(l.update(Some(20.0), false));
        assert!(l.update(Some(22.0), false)); // still on
        assert!(!l.update(Some(25.0), false));
        assert!(l.update(Some(10.0), false));
        assert!(!l.update(Some(10.0), true)); // charging leaves
        assert!(!l.update(None, false));
    }

    #[test]
    fn low_power_manual_override() {
        let mut l = LowPower::new(20.0);
        l.setting = LowPowerSetting::On;
        assert!(l.update(Some(90.0), true));
        l.setting = LowPowerSetting::Off;
        assert!(!l.update(Some(5.0), false));
    }

    #[test]
    fn idle_escalation() {
        let c = IdleConfig::default();
        assert_eq!(c.action(5, false, false, false), IdleAction::None);
        assert_eq!(c.action(20, false, false, false), IdleAction::Dim);
        assert_eq!(c.action(30, false, false, false), IdleAction::ScreenOff);
        assert_eq!(c.action(300, false, false, false), IdleAction::Suspend);
        assert_eq!(c.action(300, false, true, false), IdleAction::ScreenOff); // AC: no suspend
        assert_eq!(c.action(999, false, false, true), IdleAction::None); // inhibited
    }

    #[test]
    fn low_power_halves_timers() {
        let c = IdleConfig::default();
        assert_eq!(c.action(10, true, false, false), IdleAction::Dim);
        assert_eq!(c.action(150, true, false, false), IdleAction::Suspend);
    }

    #[test]
    fn suspend_disabled_and_next_threshold() {
        let c = IdleConfig {
            suspend_after_s: 0,
            ..Default::default()
        };
        assert_eq!(c.action(10_000, false, false, false), IdleAction::ScreenOff);
        assert_eq!(c.next_threshold(0, false, false), Some(20));
        assert_eq!(c.next_threshold(25, false, false), Some(5));
        assert_eq!(c.next_threshold(30, false, false), None);
    }
}
