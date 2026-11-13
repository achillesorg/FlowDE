//! D-Bus proxies (system bus). All calls are best-effort: callers must treat
//! errors as "service absent".

use zbus::proxy;

#[proxy(
    interface = "org.freedesktop.UPower.Device",
    default_service = "org.freedesktop.UPower",
    default_path = "/org/freedesktop/UPower/devices/DisplayDevice"
)]
pub trait UPowerDisplay {
    #[zbus(property)]
    fn percentage(&self) -> zbus::Result<f64>;
    /// 1 charging, 2 discharging, 3 empty, 4 fully charged, 5/6 pending.
    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn time_to_empty(&self) -> zbus::Result<i64>;
    #[zbus(property)]
    fn is_present(&self) -> zbus::Result<bool>;
}

#[proxy(
    interface = "net.hadess.PowerProfiles",
    default_service = "net.hadess.PowerProfiles",
    default_path = "/net/hadess/PowerProfiles"
)]
pub trait PowerProfiles {
    #[zbus(property)]
    fn active_profile(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn set_active_profile(&self, p: &str) -> zbus::Result<()>;
    #[zbus(property)]
    fn performance_degraded(&self) -> zbus::Result<String>;
}

#[proxy(
    interface = "org.freedesktop.login1.Manager",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1"
)]
pub trait Login1 {
    fn suspend(&self, interactive: bool) -> zbus::Result<()>;
    fn can_suspend(&self) -> zbus::Result<String>;
}

#[proxy(
    interface = "org.freedesktop.login1.Session",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1/session/auto"
)]
pub trait Login1Session {
    fn set_brightness(&self, subsystem: &str, name: &str, brightness: u32) -> zbus::Result<()>;
}

/// UPower state number -> (charging, label).
pub fn upower_state(s: u32) -> (bool, &'static str) {
    match s {
        1 => (true, "charging"),
        2 => (false, "discharging"),
        3 => (false, "empty"),
        4 => (false, "full"),
        5 => (false, "pending-charge"),
        6 => (false, "pending-discharge"),
        _ => (false, "unknown"),
    }
}

/// Valid power-profiles-daemon profile names.
pub const PROFILES: &[&str] = &["power-saver", "balanced", "performance"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states() {
        assert_eq!(upower_state(1), (true, "charging"));
        assert_eq!(upower_state(4).1, "full");
        assert_eq!(upower_state(99).1, "unknown");
        assert!(PROFILES.contains(&"balanced"));
    }
}
