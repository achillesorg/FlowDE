//! Haptics: patterns, intensity scaling and backends.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pattern {
    Tap,
    Select,
    Success,
    Warning,
    Error,
    LongPress,
    Ringing,
}

impl Pattern {
    /// Vibration pulses as (on_ms, off_ms) at full intensity.
    pub fn pulses(self) -> &'static [(u32, u32)] {
        match self {
            Pattern::Tap => &[(12, 0)],
            Pattern::Select => &[(8, 0)],
            Pattern::Success => &[(15, 60), (25, 0)],
            Pattern::Warning => &[(40, 80), (40, 0)],
            Pattern::Error => &[(60, 60), (60, 60), (60, 0)],
            Pattern::LongPress => &[(30, 0)],
            Pattern::Ringing => &[(400, 300), (400, 0)],
        }
    }

    /// feedbackd event name (see the Feedback Theme Specification).
    pub fn feedbackd_event(self) -> &'static str {
        match self {
            Pattern::Tap | Pattern::Select => "button-pressed",
            Pattern::Success => "message-new-instant",
            Pattern::Warning => "message-new-instant",
            Pattern::Error => "battery-low",
            Pattern::LongPress => "button-pressed",
            Pattern::Ringing => "phone-incoming-call",
        }
    }
}

/// Scale pulse durations by intensity (0..1). Below 0.05 nothing is played.
pub fn scale(p: Pattern, intensity: f32) -> Vec<(u32, u32)> {
    if intensity < 0.05 {
        return vec![];
    }
    let i = intensity.clamp(0.0, 1.0);
    p.pulses()
        .iter()
        .map(|&(on, off)| (((on as f32) * (0.4 + 0.6 * i)).round().max(1.0) as u32, off))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    pub enabled: bool,
    pub intensity: f32,
    pub low_power: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            intensity: 1.0,
            low_power: false,
        }
    }
}

impl Settings {
    /// Effective intensity: 0 when disabled; halved in low-power mode except
    /// for Warning/Error/Ringing which must stay noticeable.
    pub fn effective(&self, p: Pattern) -> f32 {
        if !self.enabled {
            return 0.0;
        }
        let critical = matches!(p, Pattern::Warning | Pattern::Error | Pattern::Ringing);
        if self.low_power && !critical {
            self.intensity * 0.5
        } else {
            self.intensity
        }
    }
}

/// Sysfs timed-output vibrator (`activate`/`duration` files, e.g. LED-class
/// `vibrator` on Android-derived phones). Root path injectable for tests.
pub struct SysfsVibrator {
    pub dir: PathBuf,
}

impl SysfsVibrator {
    pub fn find(sys: &std::path::Path) -> Option<SysfsVibrator> {
        let d = sys.join("class/leds/vibrator");
        (d.join("duration").exists() && d.join("activate").exists())
            .then_some(SysfsVibrator { dir: d })
    }

    /// Write one pulse; returns after submitting (the kernel times it).
    pub fn pulse(&self, ms: u32) -> std::io::Result<()> {
        std::fs::write(self.dir.join("duration"), ms.to_string())?;
        std::fs::write(self.dir.join("activate"), "1")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaling() {
        assert!(scale(Pattern::Tap, 0.0).is_empty());
        let full = scale(Pattern::Error, 1.0);
        let half = scale(Pattern::Error, 0.5);
        assert_eq!(full.len(), 3);
        assert!(half[0].0 < full[0].0);
        assert!(scale(Pattern::Select, 0.1)[0].0 >= 1);
    }

    #[test]
    fn effective_intensity() {
        let mut s = Settings::default();
        assert_eq!(s.effective(Pattern::Tap), 1.0);
        s.low_power = true;
        assert_eq!(s.effective(Pattern::Tap), 0.5);
        assert_eq!(s.effective(Pattern::Error), 1.0);
        s.enabled = false;
        assert_eq!(s.effective(Pattern::Error), 0.0);
    }

    #[test]
    fn sysfs_pulse() {
        let d = std::env::temp_dir().join(format!("flow-hap-{}", std::process::id()));
        let v = d.join("class/leds/vibrator");
        std::fs::create_dir_all(&v).unwrap();
        assert!(SysfsVibrator::find(&d).is_none());
        std::fs::write(v.join("duration"), "").unwrap();
        std::fs::write(v.join("activate"), "").unwrap();
        let s = SysfsVibrator::find(&d).unwrap();
        s.pulse(25).unwrap();
        assert_eq!(std::fs::read_to_string(v.join("duration")).unwrap(), "25");
        assert_eq!(std::fs::read_to_string(v.join("activate")).unwrap(), "1");
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn feedbackd_names() {
        assert_eq!(Pattern::Ringing.feedbackd_event(), "phone-incoming-call");
    }
}
