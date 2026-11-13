//! Interface themes. All four resolve into one [`Tokens`] structure whose
//! colour-role names match FeDe's `fede-design` `ThemeRole::key()` values, so
//! a shared design crate can replace this module later.

use crate::color::{Rgb, Rgba};
use crate::material_you::{self, ContrastLevel};
use crate::model::*;
use crate::resolve::resolve;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeKind {
    #[default]
    #[serde(rename = "liquid-glass")]
    LiquidGlass,
    #[serde(rename = "minimalist")]
    Minimalist,
    #[serde(rename = "material-you")]
    MaterialYou,
    #[serde(rename = "anthropic-design")]
    AnthropicDesign,
}

impl ThemeKind {
    pub const ALL: [ThemeKind; 4] = [
        Self::LiquidGlass,
        Self::Minimalist,
        Self::MaterialYou,
        Self::AnthropicDesign,
    ];
    pub fn id(self) -> &'static str {
        match self {
            Self::LiquidGlass => "liquid-glass",
            Self::Minimalist => "minimalist",
            Self::MaterialYou => "material-you",
            Self::AnthropicDesign => "anthropic-design",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.id() == s)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Geometry {
    pub radius_window: f32,
    pub radius_card: f32,
    pub radius_control: f32,
    pub hairline: f32,
    pub icon_stroke: f32,
    /// Backdrop blur; 0 for every non-glass theme.
    pub blur: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Shadow {
    pub dx: f32,
    pub dy: f32,
    pub blur: f32,
    pub color: Rgba,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Tokens {
    pub theme: ThemeKind,
    pub scheme: Scheme,
    /// Colour roles keyed like `fede-design` (`canvas`, `surface`, `accent`, ...).
    pub palette: BTreeMap<String, Rgba>,
    pub geometry: Geometry,
    pub shadow: Shadow,
    /// Present only for liquid-glass.
    pub material: Option<Material>,
    /// Effective cost tier. Non-glass themes are always Flat (no backdrop work).
    pub quality: Quality,
    pub motion: bool,
}

fn rgb(h: &str) -> Rgb {
    Rgb::from_hex(h).expect("static colour")
}

fn pal(items: &[(&str, Rgba)]) -> BTreeMap<String, Rgba> {
    items.iter().map(|(k, v)| (k.to_string(), *v)).collect()
}

/// FeDe "Clay" palette values (copied from fede-design; see docs/CREDITS.md).
fn clay(dark: bool) -> BTreeMap<String, Rgba> {
    let c = |h: &str| rgb(h).with_alpha(1.0);
    if dark {
        pal(&[
            ("canvas", c("#1F1E1C")),
            ("surface", c("#2F2C29")),
            ("surface_raised", c("#33302C")),
            ("surface_sunken", c("#24221F")),
            ("border", c("#4A4640")),
            ("text_primary", c("#F3EFE6")),
            ("text_secondary", c("#B3AC9F")),
            ("accent", c("#D97757")),
            ("accent_text", c("#1C1B19")),
            ("focus", c("#E8997B")),
            ("danger", c("#EE8A7C")),
            ("success", c("#8BC79A")),
            ("warning", c("#E0B35A")),
            ("shadow", rgb("#0E0B08").with_alpha(0.30)),
            ("highlight", rgb("#FFFFFF").with_alpha(0.06)),
            ("panel", c("#2B2926")),
            ("ink", c("#F3EFE6")),
            ("accent_soft", c("#4A3328")),
            ("accent_soft_text", c("#F4C9B4")),
        ])
    } else {
        pal(&[
            ("canvas", c("#F3EFE6")),
            ("surface", c("#FAF9F5")),
            ("surface_raised", c("#FCFBF7")),
            ("surface_sunken", c("#ECE7DB")),
            ("border", c("#CFC6B4")),
            ("text_primary", c("#2B2926")),
            ("text_secondary", c("#6B665C")),
            ("accent", c("#D97757")),
            ("accent_text", c("#2B1A12")),
            ("focus", c("#9A4222")),
            ("danger", c("#B03A2E")),
            ("success", c("#2F6B3F")),
            ("warning", c("#8A5A12")),
            ("shadow", rgb("#4A3620").with_alpha(0.14)),
            ("highlight", rgb("#FFFFFF").with_alpha(0.78)),
            ("panel", c("#F5F2EA")),
            ("ink", c("#3A3733")),
            ("accent_soft", c("#F2D5C6")),
            ("accent_soft_text", c("#7A3A22")),
        ])
    }
}

/// Black or white, whichever reads better on `bg`.
fn readable_on(bg: Rgb) -> Rgb {
    let l = bg.luminance();
    if crate::color::contrast(l, 0.0) >= crate::color::contrast(l, 1.0) {
        Rgb::new(0.0, 0.0, 0.0)
    } else {
        Rgb::new(1.0, 1.0, 1.0)
    }
}

fn minimalist(dark: bool, accent: Rgb) -> BTreeMap<String, Rgba> {
    let c = |h: &str| rgb(h).with_alpha(1.0);
    let acc = accent.with_alpha(1.0);
    let mut p = if dark {
        pal(&[
            ("canvas", c("#000000")),
            ("surface", c("#0E0E0E")),
            ("surface_raised", c("#181818")),
            ("surface_sunken", c("#050505")),
            ("border", c("#2A2A2A")),
            ("text_primary", c("#F5F5F5")),
            ("text_secondary", c("#A0A0A0")),
            ("accent_text", c("#000000")),
            ("focus", c("#FFFFFF")),
            ("danger", c("#FF6B6B")),
            ("success", c("#5FD38A")),
            ("warning", c("#F2C14E")),
            ("shadow", rgb("#000000").with_alpha(0.0)),
            ("highlight", rgb("#FFFFFF").with_alpha(0.0)),
            ("panel", c("#0A0A0A")),
            ("ink", c("#F5F5F5")),
            ("accent_soft", c("#1E1E1E")),
            ("accent_soft_text", c("#F5F5F5")),
        ])
    } else {
        pal(&[
            ("canvas", c("#FFFFFF")),
            ("surface", c("#FAFAFA")),
            ("surface_raised", c("#FFFFFF")),
            ("surface_sunken", c("#F0F0F0")),
            ("border", c("#E0E0E0")),
            ("text_primary", c("#111111")),
            ("text_secondary", c("#5E5E5E")),
            ("accent_text", c("#FFFFFF")),
            ("focus", c("#000000")),
            ("danger", c("#B3261E")),
            ("success", c("#1E7B3A")),
            ("warning", c("#8A5A00")),
            ("shadow", rgb("#000000").with_alpha(0.0)),
            ("highlight", rgb("#FFFFFF").with_alpha(0.0)),
            ("panel", c("#F7F7F7")),
            ("ink", c("#111111")),
            ("accent_soft", c("#EDEDED")),
            ("accent_soft_text", c("#111111")),
        ])
    };
    p.insert("accent".into(), acc);
    p.insert("accent_text".into(), readable_on(accent).with_alpha(1.0));
    p
}

fn material_you_palette(
    prefs: &Preferences,
    seed: Rgb,
    level: ContrastLevel,
) -> BTreeMap<String, Rgba> {
    let s = material_you::scheme(seed, prefs.scheme, level);
    let g = |k: &str| {
        s.iter()
            .find(|(n, _)| *n == k)
            .map(|(_, c)| c.with_alpha(1.0))
            .unwrap_or_else(|| Rgb::new(0.5, 0.5, 0.5).with_alpha(1.0))
    };
    let dark = prefs.scheme == Scheme::Dark;
    pal(&[
        ("canvas", g("surface")),
        ("surface", g("surface_container")),
        ("surface_raised", g("surface_container_high")),
        ("surface_sunken", g("surface_container_low")),
        ("border", g("outline_variant")),
        ("text_primary", g("on_surface")),
        ("text_secondary", g("on_surface_variant")),
        ("accent", g("primary")),
        ("accent_text", g("on_primary")),
        ("focus", g("primary")),
        ("danger", g("error")),
        ("success", g("tertiary")),
        ("warning", g("secondary")),
        (
            "shadow",
            Rgb::new(0.0, 0.0, 0.0).with_alpha(if dark { 0.35 } else { 0.15 }),
        ),
        ("highlight", Rgb::new(1.0, 1.0, 1.0).with_alpha(0.0)),
        ("panel", g("surface_container")),
        ("ink", g("on_surface")),
        ("accent_soft", g("primary_container")),
        ("accent_soft_text", g("on_primary_container")),
    ])
}

/// Palette derived from a glass material (labels and tint over backdrop).
fn glass_palette(m: &Material, accent: Rgb) -> BTreeMap<String, Rgba> {
    let tint = m.tint;
    let solid = |a: f32| Rgba { a, ..tint };
    pal(&[
        ("canvas", Rgba { a: 1.0, ..tint }),
        ("surface", tint),
        ("surface_raised", solid((tint.a + 0.1).min(1.0))),
        ("surface_sunken", solid((tint.a * 0.6).max(0.05))),
        ("border", m.border),
        ("text_primary", m.label),
        ("text_secondary", m.label_secondary),
        ("accent", accent.with_alpha(1.0)),
        ("accent_text", readable_on(accent).with_alpha(1.0)),
        ("focus", accent.with_alpha(1.0)),
        ("danger", Rgb::new(1.0, 0.27, 0.23).with_alpha(1.0)),
        ("success", Rgb::new(0.19, 0.82, 0.35).with_alpha(1.0)),
        ("warning", Rgb::new(1.0, 0.62, 0.04).with_alpha(1.0)),
        ("shadow", m.shadow),
        (
            "highlight",
            Rgb::new(1.0, 1.0, 1.0).with_alpha(m.specular * 0.5),
        ),
        ("panel", tint),
        ("ink", m.label),
        ("accent_soft", accent.with_alpha(0.22)),
        ("accent_soft_text", m.label),
    ])
}

/// Resolve any theme. `quality` is only consulted for liquid-glass.
pub fn resolve_theme(prefs: &Preferences, env: &Environment, quality: Quality) -> Tokens {
    let dark = prefs.scheme == Scheme::Dark;
    let level = if prefs.a11y.increase_contrast {
        ContrastLevel::High
    } else {
        prefs.contrast
    };
    let motion_ok = !prefs.a11y.reduce_motion;
    match prefs.theme {
        ThemeKind::LiquidGlass => {
            let m = resolve(prefs, env, quality);
            let palette = glass_palette(&m, prefs.accent);
            let g = Geometry {
                radius_window: m.corners.window,
                radius_card: m.corners.card,
                radius_control: m.corners.control,
                hairline: 1.0,
                icon_stroke: 1.5,
                blur: m.blur_radius_px,
            };
            let shadow = Shadow {
                dx: 0.0,
                dy: 8.0,
                blur: if quality == Quality::Flat { 0.0 } else { 24.0 },
                color: m.shadow,
            };
            Tokens {
                theme: prefs.theme,
                scheme: prefs.scheme,
                palette,
                geometry: g,
                shadow,
                quality: m.quality,
                motion: m.motion,
                material: Some(m),
            }
        }
        ThemeKind::Minimalist => flat_tokens(
            prefs,
            minimalist(dark, prefs.accent),
            8.0,
            0.0,
            motion_ok,
            1.0,
        ),
        ThemeKind::MaterialYou => {
            let seed = prefs.seed.unwrap_or(prefs.accent);
            flat_tokens(
                prefs,
                material_you_palette(prefs, seed, level),
                16.0,
                1.0,
                motion_ok,
                1.0,
            )
        }
        ThemeKind::AnthropicDesign => {
            // Flat cut paper: hairlines, 5 px controls, zero-blur offset shadow.
            let mut t = flat_tokens(prefs, clay(dark), 5.0, 1.0, motion_ok, 1.25);
            t.shadow = Shadow {
                dx: 0.0,
                dy: 1.0,
                blur: 0.0,
                color: t.palette["shadow"],
            };
            t
        }
    }
}

fn flat_tokens(
    prefs: &Preferences,
    palette: BTreeMap<String, Rgba>,
    radius: f32,
    hairline_default: f32,
    motion: bool,
    stroke: f32,
) -> Tokens {
    let hairline = if prefs.a11y.increase_contrast {
        2.0
    } else {
        hairline_default.max(0.0)
    };
    let shadow_c = palette["shadow"];
    Tokens {
        theme: prefs.theme,
        scheme: prefs.scheme,
        palette,
        geometry: Geometry {
            radius_window: radius * 1.6,
            radius_card: radius * 1.2,
            radius_control: radius,
            hairline,
            icon_stroke: stroke,
            blur: 0.0,
        },
        shadow: Shadow {
            dx: 0.0,
            dy: 1.0,
            blur: 0.0,
            color: shadow_c,
        },
        material: None,
        quality: Quality::Flat,
        motion,
    }
}

/// Flat string tokens for D-Bus / QML / CSS-var output.
pub fn flat(t: &Tokens) -> BTreeMap<String, String> {
    let mut m = match &t.material {
        Some(mat) => crate::output::tokens(mat),
        None => BTreeMap::new(),
    };
    m.insert("theme".into(), t.theme.id().into());
    m.insert("scheme".into(), format!("{:?}", t.scheme).to_lowercase());
    m.insert("quality".into(), format!("{:?}", t.quality).to_lowercase());
    for (k, v) in &t.palette {
        m.insert(format!("color.{k}"), v.css());
    }
    let g = &t.geometry;
    m.insert("radius.window".into(), format!("{:.1}px", g.radius_window));
    m.insert("radius.card".into(), format!("{:.1}px", g.radius_card));
    m.insert(
        "radius.control".into(),
        format!("{:.1}px", g.radius_control),
    );
    m.insert("stroke.hairline".into(), format!("{:.2}px", g.hairline));
    m.insert("stroke.icon".into(), format!("{:.2}px", g.icon_stroke));
    m.insert("blur.radius".into(), format!("{:.1}px", g.blur));
    m.insert(
        "shadow".into(),
        format!(
            "{:.0}px {:.0}px {:.0}px {}",
            t.shadow.dx,
            t.shadow.dy,
            t.shadow.blur,
            t.shadow.color.css()
        ),
    );
    m.insert("motion".into(), t.motion.to_string());
    m
}

pub fn gtk_css(t: &Tokens) -> String {
    let mut s = format!("/* flow-theme: {} {:?} */\n", t.theme.id(), t.scheme);
    for (k, v) in &t.palette {
        s.push_str(&format!("@define-color flow_{k} {};\n", v.css()));
    }
    s.push_str(&format!(
        ".flow-surface {{ background-color: @flow_surface; color: @flow_text_primary; border: {:.2}px solid @flow_border; border-radius: {:.1}px; box-shadow: {:.0}px {:.0}px {:.0}px {}; }}\n",
        t.geometry.hairline, t.geometry.radius_window, t.shadow.dx, t.shadow.dy, t.shadow.blur, t.shadow.color.css()
    ));
    s.push_str(&format!(
        ".flow-card {{ border-radius: {:.1}px; }}\n.flow-control {{ border-radius: {:.1}px; }}\n",
        t.geometry.radius_card, t.geometry.radius_control
    ));
    s.push_str(".flow-accent { background-color: @flow_accent; color: @flow_accent_text; }\n");
    s
}

pub fn css_vars(t: &Tokens) -> String {
    let mut s = String::from(":root {\n");
    for (k, v) in flat(t) {
        s.push_str(&format!(
            "  --flow-{}: {};\n",
            k.replace(['.', '_'], "-"),
            v
        ));
    }
    s.push_str("}\n");
    s
}

/// Per-app override: any field left None inherits the global preference.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppOverride {
    pub theme: Option<ThemeKind>,
    pub scheme: Option<Scheme>,
    pub icon_pack: Option<String>,
}

impl AppOverride {
    pub fn apply(&self, base: &Preferences) -> Preferences {
        let mut p = *base;
        if let Some(t) = self.theme {
            p.theme = t;
        }
        if let Some(s) = self.scheme {
            p.scheme = s;
        }
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::contrast;

    fn env() -> Environment {
        Environment {
            gpu: GpuClass::High,
            ..Default::default()
        }
    }

    fn with(theme: ThemeKind, scheme: Scheme) -> Tokens {
        resolve_theme(
            &Preferences {
                theme,
                scheme,
                ..Default::default()
            },
            &env(),
            Quality::Full,
        )
    }

    #[test]
    fn all_themes_have_all_fede_roles() {
        const ROLES: [&str; 19] = [
            "canvas",
            "surface",
            "surface_raised",
            "surface_sunken",
            "border",
            "text_primary",
            "text_secondary",
            "accent",
            "accent_text",
            "focus",
            "danger",
            "success",
            "warning",
            "shadow",
            "highlight",
            "panel",
            "ink",
            "accent_soft",
            "accent_soft_text",
        ];
        for t in ThemeKind::ALL {
            for s in [Scheme::Light, Scheme::Dark] {
                let tk = with(t, s);
                for r in ROLES {
                    assert!(tk.palette.contains_key(r), "{t:?} {s:?} missing {r}");
                }
            }
        }
    }

    #[test]
    fn only_glass_uses_blur_and_material() {
        assert!(with(ThemeKind::LiquidGlass, Scheme::Light)
            .material
            .is_some());
        for t in [
            ThemeKind::Minimalist,
            ThemeKind::MaterialYou,
            ThemeKind::AnthropicDesign,
        ] {
            let tk = with(t, Scheme::Light);
            assert!(
                tk.material.is_none() && tk.geometry.blur == 0.0 && tk.quality == Quality::Flat
            );
            assert_eq!(tk.shadow.blur, 0.0);
        }
    }

    #[test]
    fn anthropic_matches_fede_clay() {
        let l = with(ThemeKind::AnthropicDesign, Scheme::Light);
        assert_eq!(l.palette["accent"].hex(), "#d97757ff");
        assert_eq!(l.palette["canvas"].hex(), "#f3efe6ff");
        assert_eq!(l.geometry.radius_control, 5.0);
        let d = with(ThemeKind::AnthropicDesign, Scheme::Dark);
        assert_eq!(d.palette["canvas"].hex(), "#1f1e1cff");
    }

    #[test]
    fn text_contrast_on_flat_themes() {
        for t in [
            ThemeKind::Minimalist,
            ThemeKind::MaterialYou,
            ThemeKind::AnthropicDesign,
        ] {
            for s in [Scheme::Light, Scheme::Dark] {
                let tk = with(t, s);
                let bg = Rgb::new(
                    tk.palette["surface"].r,
                    tk.palette["surface"].g,
                    tk.palette["surface"].b,
                )
                .luminance();
                let fg = |k: &str| {
                    let c = tk.palette[k];
                    Rgb::new(c.r, c.g, c.b).luminance()
                };
                assert!(contrast(bg, fg("text_primary")) >= 4.5, "{t:?} {s:?}");
                let acc = fg("accent");
                assert!(
                    contrast(acc, fg("accent_text")) >= 4.0,
                    "{t:?} {s:?} accent"
                );
            }
        }
    }

    #[test]
    fn material_you_follows_seed() {
        let red = resolve_theme(
            &Preferences {
                theme: ThemeKind::MaterialYou,
                seed: Some(rgb("#d32f2f")),
                ..Default::default()
            },
            &env(),
            Quality::Full,
        );
        let blue = resolve_theme(
            &Preferences {
                theme: ThemeKind::MaterialYou,
                seed: Some(rgb("#1976d2")),
                ..Default::default()
            },
            &env(),
            Quality::Full,
        );
        assert!(red.palette["accent"].r > red.palette["accent"].b);
        assert!(blue.palette["accent"].b > blue.palette["accent"].r);
    }

    #[test]
    fn increase_contrast_thickens_hairline() {
        let mut p = Preferences {
            theme: ThemeKind::AnthropicDesign,
            ..Default::default()
        };
        p.a11y.increase_contrast = true;
        assert_eq!(
            resolve_theme(&p, &env(), Quality::Full).geometry.hairline,
            2.0
        );
    }

    #[test]
    fn per_app_override_and_parse() {
        let base = Preferences::default();
        let o = AppOverride {
            theme: Some(ThemeKind::MaterialYou),
            scheme: Some(Scheme::Dark),
            icon_pack: None,
        };
        let p = o.apply(&base);
        assert_eq!((p.theme, p.scheme), (ThemeKind::MaterialYou, Scheme::Dark));
        assert_eq!(AppOverride::default().apply(&base), base);
        assert_eq!(
            ThemeKind::parse("anthropic-design"),
            Some(ThemeKind::AnthropicDesign)
        );
        assert_eq!(ThemeKind::parse("x"), None);
    }

    #[test]
    fn outputs() {
        let t = with(ThemeKind::AnthropicDesign, Scheme::Light);
        assert!(gtk_css(&t).contains("@define-color flow_accent rgba(217, 119, 87"));
        assert!(css_vars(&t).contains("--flow-color-accent:"));
        assert_eq!(flat(&t)["theme"], "anthropic-design");
        let g = flat(&with(ThemeKind::LiquidGlass, Scheme::Light));
        assert!(g.contains_key("glass.blur_radius") && g.contains_key("color.surface"));
    }
}
