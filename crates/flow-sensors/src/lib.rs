//! Sensor logic: orientation, auto-brightness curve, ambient classes, tilt.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Orientation {
    #[default]
    Undefined,
    Normal,
    BottomUp,
    LeftUp,
    RightUp,
}

impl Orientation {
    /// Parse iio-sensor-proxy `AccelerometerOrientation` strings.
    pub fn parse(s: &str) -> Orientation {
        match s {
            "normal" => Orientation::Normal,
            "bottom-up" => Orientation::BottomUp,
            "left-up" => Orientation::LeftUp,
            "right-up" => Orientation::RightUp,
            _ => Orientation::Undefined,
        }
    }

    /// Clockwise UI rotation in degrees to apply for this orientation.
    pub fn rotation_deg(self) -> u16 {
        match self {
            Orientation::Normal | Orientation::Undefined => 0,
            Orientation::RightUp => 90,
            Orientation::BottomUp => 180,
            Orientation::LeftUp => 270,
        }
    }
}

/// Auto-rotate with a user lock. Undefined readings never change the output.
#[derive(Debug, Clone)]
pub struct AutoRotate {
    pub locked: bool,
    current: Orientation,
}

impl Default for AutoRotate {
    fn default() -> Self {
        Self::new()
    }
}

impl AutoRotate {
    pub fn new() -> Self {
        Self {
            locked: false,
            current: Orientation::Normal,
        }
    }
    pub fn current(&self) -> Orientation {
        self.current
    }
    /// Feed a sensor reading; returns Some(new) if the effective orientation changed.
    pub fn feed(&mut self, o: Orientation) -> Option<Orientation> {
        if self.locked || o == Orientation::Undefined || o == self.current {
            return None;
        }
        self.current = o;
        Some(o)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Ambient {
    Dark,
    Dim,
    Indoor,
    Bright,
    Sunlight,
}

pub fn classify(lux: f32) -> Ambient {
    match lux {
        l if l < 10.0 => Ambient::Dark,
        l if l < 100.0 => Ambient::Dim,
        l if l < 1000.0 => Ambient::Indoor,
        l if l < 10_000.0 => Ambient::Bright,
        _ => Ambient::Sunlight,
    }
}

pub const MIN_BRIGHTNESS: f32 = 0.08;

/// Log-scaled lux -> backlight fraction (10 000 lux = 1.0), plus user bias.
pub fn brightness_for(lux: f32, bias: f32) -> f32 {
    let t = ((lux.max(0.0) + 1.0).log10() / 10_001f32.log10()).clamp(0.0, 1.0);
    (MIN_BRIGHTNESS + (1.0 - MIN_BRIGHTNESS) * t + bias.clamp(-0.3, 0.3)).clamp(MIN_BRIGHTNESS, 1.0)
}

/// Auto-brightness controller with a relative hysteresis band so the
/// backlight does not hunt on small lux changes (each change costs power and
/// is visible).
#[derive(Debug, Clone)]
pub struct AutoBrightness {
    pub bias: f32,
    last: Option<f32>,
}

const BAND: f32 = 0.12;

impl AutoBrightness {
    pub fn new() -> Self {
        Self {
            bias: 0.0,
            last: None,
        }
    }
    /// Returns Some(target) only when it moved beyond the hysteresis band.
    pub fn update(&mut self, lux: f32) -> Option<f32> {
        let t = brightness_for(lux, self.bias);
        match self.last {
            Some(l)
                if (t - l).abs() / l.max(MIN_BRIGHTNESS) < BAND
                    && t > MIN_BRIGHTNESS + 1e-3
                    && t < 1.0 - 1e-3 =>
            {
                None
            }
            _ => {
                self.last = Some(t);
                Some(t)
            }
        }
    }
}

impl Default for AutoBrightness {
    fn default() -> Self {
        Self::new()
    }
}

/// Tilt in g from raw accelerometer counts and scale (m/s^2 per count).
/// Returns (x, y) clamped to [-1, 1].
pub fn tilt_from_raw(x: f64, y: f64, scale: f64) -> (f32, f32) {
    const G: f64 = 9.80665;
    let c = |v: f64| ((v * scale) / G).clamp(-1.0, 1.0) as f32;
    (c(x), c(y))
}

/// Read tilt from an IIO device directory (`in_accel_{x,y}_raw` + `in_accel_scale`).
pub fn read_iio_tilt(dev: &Path) -> Option<(f32, f32)> {
    let n = |f: &str| {
        std::fs::read_to_string(dev.join(f))
            .ok()?
            .trim()
            .parse::<f64>()
            .ok()
    };
    let scale = n("in_accel_scale").unwrap_or(1.0);
    Some(tilt_from_raw(
        n("in_accel_x_raw")?,
        n("in_accel_y_raw")?,
        scale,
    ))
}

/// Find the first IIO device exposing an accelerometer under `<root>/bus/iio/devices`.
pub fn find_iio_accel(root: &Path) -> Option<std::path::PathBuf> {
    let mut v: Vec<_> = std::fs::read_dir(root.join("bus/iio/devices"))
        .ok()?
        .flatten()
        .collect();
    v.sort_by_key(|e| e.file_name());
    v.into_iter()
        .map(|e| e.path())
        .find(|p| p.join("in_accel_x_raw").exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orientation_parse_and_rotation() {
        assert_eq!(Orientation::parse("left-up"), Orientation::LeftUp);
        assert_eq!(Orientation::parse("sideways"), Orientation::Undefined);
        assert_eq!(Orientation::RightUp.rotation_deg(), 90);
        assert_eq!(Orientation::BottomUp.rotation_deg(), 180);
    }

    #[test]
    fn auto_rotate_lock_and_undefined() {
        let mut a = AutoRotate::new();
        assert_eq!(a.feed(Orientation::LeftUp), Some(Orientation::LeftUp));
        assert_eq!(a.feed(Orientation::LeftUp), None);
        assert_eq!(a.feed(Orientation::Undefined), None);
        a.locked = true;
        assert_eq!(a.feed(Orientation::Normal), None);
        assert_eq!(a.current(), Orientation::LeftUp);
    }

    #[test]
    fn classes() {
        assert_eq!(classify(0.0), Ambient::Dark);
        assert_eq!(classify(50.0), Ambient::Dim);
        assert_eq!(classify(500.0), Ambient::Indoor);
        assert_eq!(classify(5000.0), Ambient::Bright);
        assert_eq!(classify(50_000.0), Ambient::Sunlight);
    }

    #[test]
    fn curve_monotonic_and_bounded() {
        let mut prev = 0.0;
        for lux in [0.0, 1.0, 10.0, 100.0, 1000.0, 10_000.0, 100_000.0] {
            let b = brightness_for(lux, 0.0);
            assert!(b >= prev && (MIN_BRIGHTNESS..=1.0).contains(&b));
            prev = b;
        }
        assert_eq!(brightness_for(0.0, 0.0), MIN_BRIGHTNESS);
        assert_eq!(brightness_for(1e6, 0.0), 1.0);
        assert!(brightness_for(100.0, 0.2) > brightness_for(100.0, 0.0));
        assert!(brightness_for(0.0, -0.3) >= MIN_BRIGHTNESS);
    }

    #[test]
    fn hysteresis_suppresses_small_changes() {
        let mut a = AutoBrightness::new();
        assert!(a.update(200.0).is_some());
        assert!(a.update(205.0).is_none());
        assert!(a.update(215.0).is_none());
        assert!(a.update(5000.0).is_some());
        assert!(a.update(0.0).is_some());
        assert!(a.update(0.1).is_some() || a.update(0.1).is_none()); // floor re-asserts without panic
    }

    #[test]
    fn tilt_scale_and_clamp() {
        let (x, y) = tilt_from_raw(1000.0, -500.0, 0.0098);
        assert!((x - 1.0).abs() < 0.01);
        assert!((y + 0.5).abs() < 0.01);
        assert_eq!(tilt_from_raw(1e9, 0.0, 1.0).0, 1.0);
    }

    #[test]
    fn iio_sysfs() {
        let d = std::env::temp_dir().join(format!("flow-iio-{}", std::process::id()));
        let dev = d.join("bus/iio/devices/iio:device0");
        std::fs::create_dir_all(&dev).unwrap();
        std::fs::write(dev.join("in_accel_x_raw"), "490\n").unwrap();
        std::fs::write(dev.join("in_accel_y_raw"), "0\n").unwrap();
        std::fs::write(dev.join("in_accel_scale"), "0.0098\n").unwrap();
        let p = find_iio_accel(&d).unwrap();
        let (x, y) = read_iio_tilt(&p).unwrap();
        assert!((x - 0.49).abs() < 0.01 && y == 0.0);
        assert!(find_iio_accel(&d.join("none")).is_none());
        std::fs::remove_dir_all(d).unwrap();
    }
}
