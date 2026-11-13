//! Direct sysfs readers (root path injectable for tests).

use flow_common::events::Thermal;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Battery {
    pub name: String,
    pub percent: f32,
    pub status: String,
    pub charging: bool,
    /// Instantaneous power draw in watts, if reported.
    pub power_w: Option<f32>,
}

fn read(p: &Path) -> Option<String> {
    std::fs::read_to_string(p)
        .ok()
        .map(|s| s.trim().to_string())
}

fn num(p: &Path) -> Option<f64> {
    read(p)?.parse().ok()
}

/// Read the first battery under `<root>/class/power_supply` (type=Battery, system scope).
pub fn battery(root: &Path) -> Option<Battery> {
    let dir = root.join("class/power_supply");
    let mut names: Vec<_> = std::fs::read_dir(dir).ok()?.flatten().collect();
    names.sort_by_key(|e| e.file_name());
    for e in names {
        let p = e.path();
        if read(&p.join("type")).as_deref() != Some("Battery") {
            continue;
        }
        if read(&p.join("scope")).as_deref() == Some("Device") {
            continue; // peripheral battery (mouse, headset)
        }
        let percent = num(&p.join("capacity"))? as f32;
        let status = read(&p.join("status")).unwrap_or_else(|| "Unknown".into());
        let charging = status == "Charging";
        let power_w = num(&p.join("power_now"))
            .map(|uw| (uw / 1e6) as f32)
            .or_else(|| {
                let v = num(&p.join("voltage_now"))?;
                let i = num(&p.join("current_now"))?;
                Some((v * i / 1e12) as f32)
            });
        return Some(Battery {
            name: e.file_name().to_string_lossy().into_owned(),
            percent,
            status,
            charging,
            power_w,
        });
    }
    None
}

/// True if any mains/USB supply reports online.
pub fn ac_online(root: &Path) -> bool {
    let Ok(rd) = std::fs::read_dir(root.join("class/power_supply")) else {
        return false;
    };
    rd.flatten().any(|e| {
        let p = e.path();
        matches!(
            read(&p.join("type")).as_deref(),
            Some("Mains" | "USB" | "USB_PD" | "USB_C")
        ) && read(&p.join("online")).as_deref() == Some("1")
    })
}

/// Hottest thermal zone in degrees C.
pub fn max_temp_c(root: &Path) -> Option<f32> {
    let rd = std::fs::read_dir(root.join("class/thermal")).ok()?;
    rd.flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("thermal_zone"))
        .filter_map(|e| num(&e.path().join("temp")))
        .map(|m| (m / 1000.0) as f32)
        .filter(|t| (-40.0..150.0).contains(t))
        .fold(None, |a: Option<f32>, t| Some(a.map_or(t, |x| x.max(t))))
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Backlight {
    pub name: String,
    pub value: u32,
    pub max: u32,
}

impl Backlight {
    pub fn fraction(&self) -> f32 {
        if self.max == 0 {
            0.0
        } else {
            self.value as f32 / self.max as f32
        }
    }
}

/// First backlight (prefers `firmware`<`platform`<`raw`? we prefer the one with the largest range).
pub fn backlight(root: &Path) -> Option<Backlight> {
    let rd = std::fs::read_dir(root.join("class/backlight")).ok()?;
    rd.flatten()
        .filter_map(|e| {
            let p = e.path();
            Some(Backlight {
                name: e.file_name().to_string_lossy().into_owned(),
                value: num(&p.join("brightness"))? as u32,
                max: num(&p.join("max_brightness"))? as u32,
            })
        })
        .max_by_key(|b| b.max)
}

/// Map a hysteresis-free temperature to a thermal band.
pub fn thermal_band(c: f32) -> Thermal {
    if c >= 65.0 {
        Thermal::Critical
    } else if c >= 55.0 {
        Thermal::Hot
    } else if c >= 45.0 {
        Thermal::Warm
    } else {
        Thermal::Normal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("flow-pw-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn w(root: &Path, rel: &str, v: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, v).unwrap();
    }

    #[test]
    fn battery_and_ac() {
        let r = tmp("bat");
        w(&r, "class/power_supply/BAT0/type", "Battery\n");
        w(&r, "class/power_supply/BAT0/capacity", "57\n");
        w(&r, "class/power_supply/BAT0/status", "Charging\n");
        w(&r, "class/power_supply/BAT0/voltage_now", "4000000\n");
        w(&r, "class/power_supply/BAT0/current_now", "500000\n");
        w(&r, "class/power_supply/hid/type", "Battery\n");
        w(&r, "class/power_supply/hid/scope", "Device\n");
        w(&r, "class/power_supply/AC/type", "Mains\n");
        w(&r, "class/power_supply/AC/online", "1\n");
        let b = battery(&r).unwrap();
        assert_eq!(b.name, "BAT0");
        assert_eq!(b.percent, 57.0);
        assert!(b.charging);
        assert!((b.power_w.unwrap() - 2.0).abs() < 1e-3);
        assert!(ac_online(&r));
        std::fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn no_battery_is_none() {
        let r = tmp("nob");
        assert!(battery(&r).is_none());
        assert!(!ac_online(&r));
        assert!(max_temp_c(&r).is_none());
        assert!(backlight(&r).is_none());
        std::fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn thermal_and_backlight() {
        let r = tmp("th");
        w(&r, "class/thermal/thermal_zone0/temp", "41000\n");
        w(&r, "class/thermal/thermal_zone1/temp", "56250\n");
        w(&r, "class/thermal/thermal_zone2/temp", "-274000\n"); // bogus
        w(&r, "class/thermal/cooling_device0/temp", "99000\n");
        assert_eq!(max_temp_c(&r), Some(56.25));
        w(&r, "class/backlight/intel/brightness", "50\n");
        w(&r, "class/backlight/intel/max_brightness", "200\n");
        let b = backlight(&r).unwrap();
        assert_eq!(b.fraction(), 0.25);
        std::fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn bands() {
        assert_eq!(thermal_band(30.0), Thermal::Normal);
        assert_eq!(thermal_band(45.0), Thermal::Warm);
        assert_eq!(thermal_band(60.0), Thermal::Hot);
        assert_eq!(thermal_band(80.0), Thermal::Critical);
    }
}
