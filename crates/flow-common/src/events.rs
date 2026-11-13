//! Typed snapshots exchanged between daemons as IPC event payloads.
//! Producers: flow-power, flow-sensors. Consumers: flow-theme, shell.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Thermal {
    #[default]
    Normal,
    Warm,
    Hot,
    Critical,
}

/// Event name `power.changed`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PowerSnapshot {
    /// 0..=100, None when there is no battery.
    pub battery_pct: Option<f32>,
    pub charging: bool,
    pub thermal: Thermal,
    pub low_power: bool,
}

/// Event name `sensors.changed`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SensorSnapshot {
    /// Ambient light in lux, None when no sensor.
    pub lux: Option<f32>,
    /// Device tilt in g units (x right, y up), None when no accelerometer.
    pub tilt: Option<(f32, f32)>,
}

pub const EV_POWER: &str = "power.changed";
pub const EV_SENSORS: &str = "sensors.changed";
pub const EV_SETTINGS: &str = "settings.changed";
pub const EV_NOTIFY: &str = "notify.changed";
pub const EV_LOCK: &str = "lock.changed";
