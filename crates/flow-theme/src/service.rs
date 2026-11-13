//! Stateful wrapper: preferences + environment + governor.

use crate::governor::Governor;
use crate::icons;
use crate::model::*;
use crate::resolve::target_quality;
use crate::themes::{self, AppOverride, ThemeKind, Tokens};
use crate::Rgb;
use flow_common::events::{PowerSnapshot, SensorSnapshot};
use serde_json::Value;

pub struct ThemeService {
    pub prefs: Preferences,
    pub env: Environment,
    /// Manual quality ceiling from config (`auto` = None).
    pub force_max: Option<Quality>,
    /// `appearance.scheme = auto`: scheme follows ambient light.
    auto: bool,
    /// `auto` or an icon pack id.
    pub icon_pack: String,
    pub overrides: std::collections::BTreeMap<String, AppOverride>,
    pub icons: icons::Registry,
    gov: Governor,
}

impl ThemeService {
    pub fn new(gpu: GpuClass) -> Self {
        let env = Environment {
            gpu,
            ..Default::default()
        };
        let prefs = Preferences::default();
        let initial = target_quality(&prefs, &env).0;
        Self {
            prefs,
            env,
            force_max: None,
            auto: false,
            icon_pack: "auto".into(),
            overrides: Default::default(),
            icons: icons::Registry::default(),
            gov: Governor::new(initial),
        }
    }

    /// Governed quality for `now` (unix s).
    pub fn quality(&mut self, now: u64) -> Quality {
        let (mut target, _) = target_quality(&self.prefs, &self.env);
        if let Some(m) = self.force_max {
            target = target.min(m);
        }
        self.gov.update(target, now)
    }

    /// Glass material (always resolved; used by glass consumers and tests).
    pub fn material(&mut self, now: u64) -> Material {
        let q = self.quality(now);
        crate::resolve::resolve(&self.prefs, &self.env, q)
    }

    /// Tokens and chosen icon pack for an app (None = global).
    pub fn tokens_for(&mut self, app: Option<&str>, now: u64) -> (Tokens, String) {
        let q = self.quality(now);
        let ov = app
            .and_then(|a| self.overrides.get(a))
            .cloned()
            .unwrap_or_default();
        let prefs = ov.apply(&self.prefs);
        let t = themes::resolve_theme(&prefs, &self.env, q);
        let wanted = ov
            .icon_pack
            .clone()
            .unwrap_or_else(|| self.icon_pack.clone());
        let wanted = if wanted == "auto" {
            icons::default_pack(prefs.theme).to_string()
        } else {
            wanted
        };
        let pack = self.icons.pick(&wanted, &["Adwaita"]).to_string();
        (t, pack)
    }

    pub fn tokens(&mut self, now: u64) -> Tokens {
        self.tokens_for(None, now).0
    }

    pub fn set_power(&mut self, p: PowerSnapshot) {
        self.env.power = p;
    }

    pub fn set_sensors(&mut self, s: SensorSnapshot) {
        self.env.sensors = s;
        if self.auto {
            self.prefs.scheme = self.auto_scheme();
        }
    }

    /// Apply a `settings.*` key. Returns true if it was a theme-relevant key.
    pub fn apply_setting(&mut self, key: &str, v: &Value) -> bool {
        match (key, v) {
            ("appearance.scheme", Value::String(s)) => {
                // `auto` follows ambient light: dark below ~20 lux.
                self.prefs.scheme = match s.as_str() {
                    "dark" => Scheme::Dark,
                    "light" => Scheme::Light,
                    _ => self.auto_scheme(),
                };
                self.auto = s == "auto";
            }
            ("appearance.theme", Value::String(s)) => match ThemeKind::parse(s) {
                Some(t) => self.prefs.theme = t,
                None => return false,
            },
            ("appearance.icon_pack", Value::String(s)) => self.icon_pack = s.clone(),
            ("appearance.contrast", Value::String(s)) => {
                self.prefs.contrast = match s.as_str() {
                    "medium" => crate::material_you::ContrastLevel::Medium,
                    "high" => crate::material_you::ContrastLevel::High,
                    _ => crate::material_you::ContrastLevel::Standard,
                }
            }
            ("appearance.seed", Value::String(s)) => {
                if let Some(c) = Rgb::from_hex(s) {
                    self.prefs.seed = Some(c);
                }
            }
            ("appearance.material", Value::String(s)) => {
                self.prefs.variant = if s == "clear" {
                    Variant::Clear
                } else {
                    Variant::Regular
                };
            }
            ("appearance.tinted", Value::Bool(b)) => self.prefs.tinted = *b,
            ("appearance.accent", Value::String(s)) => {
                if let Some(c) = Rgb::from_hex(s) {
                    self.prefs.accent = c;
                }
            }
            ("accessibility.reduce_transparency", Value::Bool(b)) => {
                self.prefs.a11y.reduce_transparency = *b
            }
            ("accessibility.increase_contrast", Value::Bool(b)) => {
                self.prefs.a11y.increase_contrast = *b
            }
            ("accessibility.reduce_motion", Value::Bool(b)) => self.prefs.a11y.reduce_motion = *b,
            _ => return false,
        }
        true
    }

    fn auto_scheme(&self) -> Scheme {
        match self.env.sensors.lux {
            Some(l) if l < 20.0 => Scheme::Dark,
            _ => Scheme::Light,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flow_common::events::Thermal;
    use serde_json::json;

    #[test]
    fn settings_map_to_prefs() {
        let mut t = ThemeService::new(GpuClass::High);
        assert!(t.apply_setting("appearance.scheme", &json!("dark")));
        assert!(t.apply_setting("appearance.material", &json!("clear")));
        assert!(t.apply_setting("accessibility.reduce_motion", &json!(true)));
        assert!(t.apply_setting("appearance.accent", &json!("#ff0000")));
        assert!(!t.apply_setting("display.brightness", &json!(0.5)));
        assert_eq!(t.prefs.scheme, Scheme::Dark);
        assert_eq!(t.prefs.variant, Variant::Clear);
        assert!(t.prefs.a11y.reduce_motion);
        assert_eq!(t.prefs.accent.r, 1.0);
    }

    #[test]
    fn auto_scheme_follows_lux() {
        let mut t = ThemeService::new(GpuClass::High);
        t.apply_setting("appearance.scheme", &json!("auto"));
        t.set_sensors(SensorSnapshot {
            lux: Some(2.0),
            tilt: None,
        });
        assert_eq!(t.prefs.scheme, Scheme::Dark);
        t.set_sensors(SensorSnapshot {
            lux: Some(300.0),
            tilt: None,
        });
        assert_eq!(t.prefs.scheme, Scheme::Light);
        t.apply_setting("appearance.scheme", &json!("dark"));
        t.set_sensors(SensorSnapshot {
            lux: Some(300.0),
            tilt: None,
        });
        assert_eq!(t.prefs.scheme, Scheme::Dark);
    }

    #[test]
    fn theme_switch_icon_pack_and_overrides() {
        let mut t = ThemeService::new(GpuClass::High);
        assert_eq!(t.tokens(0).theme, ThemeKind::LiquidGlass);
        t.apply_setting("appearance.theme", &json!("anthropic-design"));
        let (tk, pack) = t.tokens_for(None, 1);
        assert_eq!(tk.theme, ThemeKind::AnthropicDesign);
        assert_eq!(pack, "hicolor"); // glaze-ink not installed in an empty registry
        t.overrides.insert(
            "org.kiln.Store".into(),
            AppOverride {
                theme: Some(ThemeKind::MaterialYou),
                ..Default::default()
            },
        );
        assert_eq!(
            t.tokens_for(Some("org.kiln.Store"), 2).0.theme,
            ThemeKind::MaterialYou
        );
        assert_eq!(
            t.tokens_for(Some("other"), 2).0.theme,
            ThemeKind::AnthropicDesign
        );
        assert!(!t.apply_setting("appearance.theme", &json!("nope")));
    }

    #[test]
    fn governor_applied_and_force_max() {
        let mut t = ThemeService::new(GpuClass::High);
        assert_eq!(t.material(0).quality, Quality::Full);
        t.set_power(PowerSnapshot {
            thermal: Thermal::Hot,
            ..Default::default()
        });
        assert_eq!(t.material(1).quality, Quality::Flat);
        t.set_power(PowerSnapshot::default());
        assert_eq!(t.material(2).quality, Quality::Flat); // dwell
        assert_eq!(t.material(13).quality, Quality::BlurOnly);
        let mut u = ThemeService::new(GpuClass::High);
        u.force_max = Some(Quality::BlurOnly);
        assert_eq!(u.material(0).quality, Quality::BlurOnly);
    }
}
