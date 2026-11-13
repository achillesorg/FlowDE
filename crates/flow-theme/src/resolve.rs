//! Parameter resolution: preferences + environment -> [`Material`].

use crate::color::{contrast, Rgb};
use crate::model::*;
use flow_common::events::Thermal;

const LIGHT_TINT: Rgb = Rgb::new(0.98, 0.98, 1.0);
const DARK_TINT: Rgb = Rgb::new(0.11, 0.11, 0.13);
const LIGHT_LABEL: Rgb = Rgb::new(0.07, 0.07, 0.08);
const DARK_LABEL: Rgb = Rgb::new(1.0, 1.0, 1.0);
/// Light comes from the upper left.
const BASE_RIM_ANGLE: f32 = 315.0;
const TILT_CLAMP_G: f32 = 0.5;
const MAX_HIGHLIGHT_PX: f32 = 6.0;

/// Base (Full quality) parameters per variant.
struct Base {
    blur: f32,
    sat: f32,
    refraction: f32,
    edge: f32,
    bezel: f32,
    spec: f32,
    rim_w: f32,
    alpha_light: f32,
    alpha_dark: f32,
}

fn base(v: Variant) -> Base {
    match v {
        Variant::Regular => Base {
            blur: 28.0,
            sat: 1.6,
            refraction: 0.35,
            edge: 10.0,
            bezel: 14.0,
            spec: 0.55,
            rim_w: 1.5,
            alpha_light: 0.55,
            alpha_dark: 0.45,
        },
        Variant::Clear => Base {
            blur: 12.0,
            sat: 1.3,
            refraction: 0.5,
            edge: 14.0,
            bezel: 18.0,
            spec: 0.7,
            rim_w: 1.5,
            alpha_light: 0.12,
            alpha_dark: 0.10,
        },
    }
}

/// Quality the hardware/battery/thermal/accessibility state *allows*
/// (before hysteresis), with reasons.
pub fn target_quality(prefs: &Preferences, env: &Environment) -> (Quality, Vec<String>) {
    let mut q = Quality::Full;
    let mut why = vec![];
    let mut cap = |limit: Quality, reason: &str| {
        if limit < q {
            q = limit;
        }
        why.push(reason.to_string());
    };
    if prefs.a11y.reduce_transparency {
        cap(Quality::Flat, "accessibility: reduce transparency");
    }
    if prefs.a11y.increase_contrast {
        cap(Quality::BlurOnly, "accessibility: increase contrast");
    }
    match env.gpu {
        GpuClass::None => cap(Quality::Flat, "no usable GPU"),
        GpuClass::Low => cap(Quality::BlurOnly, "low-end GPU"),
        _ => {}
    }
    match env.power.thermal {
        Thermal::Hot | Thermal::Critical => cap(Quality::Flat, "thermal: hot"),
        Thermal::Warm => cap(Quality::BlurOnly, "thermal: warm"),
        Thermal::Normal => {}
    }
    if env.power.low_power {
        cap(Quality::BlurOnly, "low power mode");
    }
    if let (Some(p), false) = (env.power.battery_pct, env.power.charging) {
        if p <= 10.0 {
            cap(Quality::Flat, "battery <= 10%");
        } else if p <= 20.0 {
            cap(Quality::BlurOnly, "battery <= 20%");
        }
    }
    (q, why)
}

/// Ambient brightness factor 0 (dark room) .. 1 (direct sun), log scaled.
pub fn bright_factor(lux: f32) -> f32 {
    ((lux.max(0.0) + 1.0).log10() / 4.5).clamp(0.0, 1.0)
}

/// Resolve the material at a given (already governed) `quality`.
pub fn resolve(prefs: &Preferences, env: &Environment, quality: Quality) -> Material {
    let (_, mut reasons) = target_quality(prefs, env);
    let b = base(prefs.variant);
    let dark = prefs.scheme == Scheme::Dark;
    let a11y = prefs.a11y;

    // Ambient light: bright light needs more opaque tint and a calmer rim.
    let bf = env.sensors.lux.map(bright_factor).unwrap_or(0.4);
    let in_dark_room = env.sensors.lux.is_some_and(|l| l < 10.0);

    // ---- tint ----
    let mut tint_rgb = if dark { DARK_TINT } else { LIGHT_TINT };
    let mut alpha = if dark { b.alpha_dark } else { b.alpha_light };
    if prefs.tinted {
        let w = if prefs.variant == Variant::Clear {
            0.25
        } else {
            0.35
        };
        tint_rgb = tint_rgb.mix(prefs.accent, w);
        alpha += 0.05;
    }
    alpha += 0.20 * bf * if bf > 0.5 { 1.0 } else { 0.0 };
    if a11y.increase_contrast {
        alpha = alpha.max(0.85);
        reasons.push("increase contrast: tint >= 0.85".into());
    }

    // ---- quality-dependent geometry ----
    let (mut blur, mut refraction, mut edge, mut spec, motion_ok) = match quality {
        Quality::Full => (b.blur, b.refraction, b.edge, b.spec, true),
        Quality::BlurOnly => (b.blur * 0.6, 0.0, 0.0, b.spec * 0.45, false),
        Quality::Flat => (0.0, 0.0, 0.0, 0.0, false),
    };
    if quality == Quality::Flat {
        alpha = alpha.max(0.94);
    }
    // Ambient adaptation of the highlight.
    spec *= 1.0 - 0.5 * bf;
    if in_dark_room {
        spec *= 0.8;
    }
    if a11y.increase_contrast {
        spec = 0.0;
        refraction = 0.0;
        edge = 0.0;
    }
    blur = blur.max(0.0);

    // ---- label colours & contrast-driven alpha ----
    let label_rgb = if dark { DARK_LABEL } else { LIGHT_LABEL };
    let label_l = label_rgb.luminance();
    let tint_l = tint_rgb.luminance();
    let content_l = env.content_luma.unwrap_or(0.5).clamp(0.0, 1.0);
    let target = if a11y.increase_contrast || bf > 0.8 {
        7.0
    } else {
        4.5
    };
    let backdrop = |a: f32| {
        a * tint_l
            + (1.0 - a)
                * if quality == Quality::Flat {
                    tint_l
                } else {
                    content_l
                }
    };
    let mut ratio = contrast(label_l, backdrop(alpha));
    let mut raised = false;
    while ratio < target && alpha < 0.95 {
        alpha = (alpha + 0.025).min(0.95);
        ratio = contrast(label_l, backdrop(alpha));
        raised = true;
    }
    if raised {
        reasons.push(format!("tint raised to {alpha:.2} for contrast {target}"));
    }

    // ---- motion highlight ----
    let motion = !a11y.reduce_motion && motion_ok && env.sensors.tilt.is_some();
    let (tx, ty) = env.sensors.tilt.unwrap_or((0.0, 0.0));
    let (tx, ty) = (
        tx.clamp(-TILT_CLAMP_G, TILT_CLAMP_G),
        ty.clamp(-TILT_CLAMP_G, TILT_CLAMP_G),
    );
    let (rim_angle, off) = if motion {
        let k = MAX_HIGHLIGHT_PX / TILT_CLAMP_G;
        (
            (BASE_RIM_ANGLE - tx * 40.0).rem_euclid(360.0),
            (-tx * k, ty * k),
        )
    } else {
        (BASE_RIM_ANGLE, (0.0, 0.0))
    };

    // ---- border / shadow ----
    let border_alpha = if a11y.increase_contrast {
        0.6
    } else if quality == Quality::Flat {
        0.18
    } else {
        0.25 + 0.25 * spec
    };
    let border_rgb = if dark {
        Rgb::new(1.0, 1.0, 1.0)
    } else {
        Rgb::new(1.0, 1.0, 1.0).mix(LIGHT_LABEL, 0.35)
    };
    let shadow_a = if quality == Quality::Flat {
        0.12
    } else if dark {
        0.35
    } else {
        0.18
    };

    let window = 32.0;
    let card = concentric(window, 10.0);
    let control = concentric(card, 8.0);
    Material {
        quality,
        scheme: prefs.scheme,
        variant: prefs.variant,
        blur_radius_px: blur,
        saturation: if quality == Quality::Flat { 1.0 } else { b.sat },
        refraction,
        edge_displacement_px: edge,
        bezel_px: if edge > 0.0 { b.bezel } else { 0.0 },
        specular: spec.clamp(0.0, 1.0),
        rim_angle_deg: rim_angle,
        rim_width_px: if spec > 0.0 { b.rim_w } else { 1.0 },
        highlight_offset_px: off,
        motion,
        tint: tint_rgb.with_alpha(alpha),
        border: border_rgb.with_alpha(border_alpha),
        label: label_rgb.with_alpha(1.0),
        label_secondary: label_rgb.with_alpha(0.62),
        shadow: Rgb::new(0.0, 0.0, 0.0).with_alpha(shadow_a),
        corners: Corners {
            window,
            card,
            control,
            chip: capsule(32.0),
        },
        contrast_ratio: ratio,
        reasons,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flow_common::events::{PowerSnapshot, SensorSnapshot};

    fn env() -> Environment {
        Environment {
            gpu: GpuClass::High,
            ..Default::default()
        }
    }

    fn q(prefs: &Preferences, env: &Environment) -> Quality {
        target_quality(prefs, env).0
    }

    #[test]
    fn full_on_good_hardware() {
        assert_eq!(q(&Preferences::default(), &env()), Quality::Full);
    }

    #[test]
    fn ladder_gpu() {
        let p = Preferences::default();
        assert_eq!(
            q(
                &p,
                &Environment {
                    gpu: GpuClass::Low,
                    ..env()
                }
            ),
            Quality::BlurOnly
        );
        assert_eq!(
            q(
                &p,
                &Environment {
                    gpu: GpuClass::None,
                    ..env()
                }
            ),
            Quality::Flat
        );
        assert_eq!(
            q(
                &p,
                &Environment {
                    gpu: GpuClass::Mid,
                    ..env()
                }
            ),
            Quality::Full
        );
    }

    #[test]
    fn ladder_battery_thermal() {
        let p = Preferences::default();
        let mut e = env();
        e.power = PowerSnapshot {
            battery_pct: Some(15.0),
            charging: false,
            ..Default::default()
        };
        assert_eq!(q(&p, &e), Quality::BlurOnly);
        e.power.battery_pct = Some(8.0);
        assert_eq!(q(&p, &e), Quality::Flat);
        e.power.charging = true; // charging lifts the battery cap
        assert_eq!(q(&p, &e), Quality::Full);
        e.power.thermal = Thermal::Warm;
        assert_eq!(q(&p, &e), Quality::BlurOnly);
        e.power.thermal = Thermal::Hot; // thermal applies even when charging
        assert_eq!(q(&p, &e), Quality::Flat);
        let mut e = env();
        e.power.low_power = true;
        assert_eq!(q(&p, &e), Quality::BlurOnly);
    }

    #[test]
    fn accessibility_caps() {
        let mut p = Preferences::default();
        p.a11y.reduce_transparency = true;
        assert_eq!(q(&p, &env()), Quality::Flat);
        let mut p = Preferences::default();
        p.a11y.increase_contrast = true;
        assert_eq!(q(&p, &env()), Quality::BlurOnly);
    }

    #[test]
    fn full_has_lensing_blur_only_does_not_flat_is_opaque() {
        let p = Preferences::default();
        let e = env();
        let full = resolve(&p, &e, Quality::Full);
        assert!(
            full.refraction > 0.0 && full.edge_displacement_px > 0.0 && full.blur_radius_px > 20.0
        );
        let blur = resolve(&p, &e, Quality::BlurOnly);
        assert_eq!(blur.refraction, 0.0);
        assert_eq!(blur.edge_displacement_px, 0.0);
        assert!(blur.blur_radius_px > 0.0 && blur.blur_radius_px < full.blur_radius_px);
        let flat = resolve(&p, &e, Quality::Flat);
        assert_eq!(flat.blur_radius_px, 0.0);
        assert_eq!(flat.specular, 0.0);
        assert!(flat.tint.a >= 0.94);
    }

    #[test]
    fn clear_is_more_transparent_than_regular() {
        let e = Environment {
            content_luma: Some(0.9),
            ..env()
        };
        let reg = resolve(&Preferences::default(), &e, Quality::Full);
        let clr = resolve(
            &Preferences {
                variant: Variant::Clear,
                ..Default::default()
            },
            &e,
            Quality::Full,
        );
        assert!(clr.tint.a < reg.tint.a);
        assert!(clr.refraction > reg.refraction);
        assert!(clr.blur_radius_px < reg.blur_radius_px);
    }

    #[test]
    fn dark_over_bright_content_raises_tint_for_contrast() {
        let p = Preferences {
            scheme: Scheme::Dark,
            ..Default::default()
        };
        let bright = resolve(
            &p,
            &Environment {
                content_luma: Some(0.9),
                ..env()
            },
            Quality::Full,
        );
        let dim = resolve(
            &p,
            &Environment {
                content_luma: Some(0.05),
                ..env()
            },
            Quality::Full,
        );
        assert!(bright.tint.a > dim.tint.a);
        assert!(bright.contrast_ratio >= 4.5 - 1e-3);
        assert!(dim.contrast_ratio >= 4.5);
    }

    #[test]
    fn sunlight_calms_specular_and_raises_tint() {
        let p = Preferences::default();
        let dim = resolve(
            &p,
            &Environment {
                sensors: SensorSnapshot {
                    lux: Some(200.0),
                    tilt: None,
                },
                ..env()
            },
            Quality::Full,
        );
        let sun = resolve(
            &p,
            &Environment {
                sensors: SensorSnapshot {
                    lux: Some(50_000.0),
                    tilt: None,
                },
                ..env()
            },
            Quality::Full,
        );
        assert!(sun.specular < dim.specular);
        assert!(sun.tint.a > dim.tint.a);
        assert!(sun.contrast_ratio >= 7.0 - 1e-3);
    }

    #[test]
    fn increase_contrast_is_opaque_and_bordered() {
        let mut p = Preferences::default();
        p.a11y.increase_contrast = true;
        let m = resolve(&p, &env(), Quality::BlurOnly);
        assert!(m.tint.a >= 0.85);
        assert_eq!(m.specular, 0.0);
        assert!(m.border.a >= 0.6);
        assert!(m.contrast_ratio >= 7.0 - 1e-3);
    }

    #[test]
    fn tilt_moves_highlight_only_with_motion() {
        let mut e = env();
        e.sensors.tilt = Some((0.3, -0.2));
        let p = Preferences::default();
        let m = resolve(&p, &e, Quality::Full);
        assert!(m.motion);
        assert!(m.highlight_offset_px.0 < 0.0 && m.highlight_offset_px.1 < 0.0);
        assert!((m.rim_angle_deg - 315.0).abs() > 1.0);
        // clamped
        e.sensors.tilt = Some((9.0, 9.0));
        let m = resolve(&p, &e, Quality::Full);
        assert!(m.highlight_offset_px.0.abs() <= MAX_HIGHLIGHT_PX + 1e-4);
        // reduce motion
        let mut p2 = p;
        p2.a11y.reduce_motion = true;
        let m = resolve(&p2, &e, Quality::Full);
        assert!(!m.motion);
        assert_eq!(m.highlight_offset_px, (0.0, 0.0));
        assert_eq!(m.rim_angle_deg, 315.0);
        // no motion below Full
        let m = resolve(&p, &e, Quality::BlurOnly);
        assert!(!m.motion);
    }

    #[test]
    fn tinted_shifts_towards_accent() {
        let p = Preferences {
            tinted: true,
            accent: Rgb::new(1.0, 0.0, 0.0),
            ..Default::default()
        };
        let t = resolve(&p, &env(), Quality::Full).tint;
        let n = resolve(&Preferences::default(), &env(), Quality::Full).tint;
        assert!(t.r > t.g && t.g < n.g);
    }

    #[test]
    fn concentric_corners() {
        assert_eq!(concentric(32.0, 10.0), 22.0);
        assert_eq!(concentric(4.0, 10.0), 0.0);
        let m = resolve(&Preferences::default(), &env(), Quality::Full);
        assert!(m.corners.window > m.corners.card && m.corners.card > m.corners.control);
        assert_eq!(m.corners.chip, 16.0);
    }

    #[test]
    fn reasons_reported() {
        let mut e = env();
        e.power.low_power = true;
        let m = resolve(&Preferences::default(), &e, Quality::BlurOnly);
        assert!(m.reasons.iter().any(|r| r.contains("low power")));
    }
}
