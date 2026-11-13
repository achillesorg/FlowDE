//! Original "dynamic colour" implementation: tonal palettes from a seed colour.
//!
//! Approach (no Google code or tables): work in CIE L*a*b* (D65) / LCh as a
//! perceptual stand-in for HCT. Tone = L*. For each (hue, chroma, tone) the
//! largest chroma that stays inside sRGB, up to the requested chroma, is used.
//! A CAM16-based hue/chroma could replace the Lab step later without changing
//! the API (see docs/liquid-glass.md, "Dynamic colour").

use crate::color::Rgb;
use crate::model::Scheme;
use serde::{Deserialize, Serialize};

pub const TONES: [u8; 13] = [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 95, 99, 100];

const XN: f32 = 0.95047;
const YN: f32 = 1.0;
const ZN: f32 = 1.08883;

fn lin(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
fn gamma(c: f32) -> f32 {
    if c <= 0.0031308 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// sRGB -> (L*, a*, b*).
pub fn to_lab(c: Rgb) -> (f32, f32, f32) {
    let (r, g, b) = (lin(c.r), lin(c.g), lin(c.b));
    let x = 0.4124564 * r + 0.3575761 * g + 0.1804375 * b;
    let y = 0.2126729 * r + 0.7151522 * g + 0.0721750 * b;
    let z = 0.0193339 * r + 0.119192 * g + 0.9503041 * b;
    let f = |t: f32| {
        if t > 216.0 / 24389.0 {
            t.cbrt()
        } else {
            (24389.0 / 27.0 * t + 16.0) / 116.0
        }
    };
    let (fx, fy, fz) = (f(x / XN), f(y / YN), f(z / ZN));
    (116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz))
}

/// (L*, a*, b*) -> unclamped linear-then-gamma sRGB components.
fn from_lab_raw(l: f32, a: f32, b: f32) -> (f32, f32, f32) {
    let fy = (l + 16.0) / 116.0;
    let (fx, fz) = (fy + a / 500.0, fy - b / 200.0);
    let finv = |t: f32| {
        if t.powi(3) > 216.0 / 24389.0 {
            t.powi(3)
        } else {
            (116.0 * t - 16.0) * 27.0 / 24389.0
        }
    };
    let (x, y, z) = (finv(fx) * XN, finv(fy) * YN, finv(fz) * ZN);
    let r = 3.2404542 * x - 1.5371385 * y - 0.4985314 * z;
    let g = -0.969266 * x + 1.8760108 * y + 0.041556 * z;
    let bl = 0.0556434 * x - 0.2040259 * y + 1.0572252 * z;
    (enc(r), enc(g), enc(bl))
}

/// Gamma-encode; out-of-gamut linear values map outside [0,1] so callers can detect them.
fn enc(v: f32) -> f32 {
    if v < 0.0 {
        -1.0
    } else {
        gamma(v)
    }
}

fn in_gamut(l: f32, a: f32, b: f32) -> Option<Rgb> {
    let (r, g, bl) = from_lab_raw(l, a, b);
    let ok = |v: f32| (-0.0005..=1.0005).contains(&v);
    // from_lab_raw encodes out-of-gamut-negative as -1.0 and >1 as >1.0005
    if ok(r) && ok(g) && ok(bl) {
        Some(Rgb::new(
            r.clamp(0.0, 1.0),
            g.clamp(0.0, 1.0),
            bl.clamp(0.0, 1.0),
        ))
    } else {
        None
    }
}

/// Hue (degrees) and chroma of a colour in LCh.
pub fn hue_chroma(c: Rgb) -> (f32, f32) {
    let (_, a, b) = to_lab(c);
    (
        b.atan2(a).to_degrees().rem_euclid(360.0),
        (a * a + b * b).sqrt(),
    )
}

/// Colour at (hue, chroma, tone) with chroma reduced until in gamut.
pub fn hct(hue: f32, chroma: f32, tone: f32) -> Rgb {
    let tone = tone.clamp(0.0, 100.0);
    if tone <= 0.0 {
        return Rgb::new(0.0, 0.0, 0.0);
    }
    if tone >= 100.0 {
        return Rgb::new(1.0, 1.0, 1.0);
    }
    let h = hue.to_radians();
    let (ch, sh) = (h.cos(), h.sin());
    let (mut lo, mut hi) = (0.0f32, chroma.max(0.0));
    if let Some(c) = in_gamut(tone, hi * ch, hi * sh) {
        return c;
    }
    for _ in 0..24 {
        let mid = (lo + hi) / 2.0;
        if in_gamut(tone, mid * ch, mid * sh).is_some() {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    in_gamut(tone, lo * ch, lo * sh).unwrap_or(Rgb::new(0.5, 0.5, 0.5))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ContrastLevel {
    #[default]
    Standard,
    Medium,
    High,
}

#[derive(Debug, Clone)]
pub struct TonalPalette {
    hue: f32,
    chroma: f32,
}

impl TonalPalette {
    pub fn new(hue: f32, chroma: f32) -> Self {
        Self {
            hue: hue.rem_euclid(360.0),
            chroma,
        }
    }
    pub fn tone(&self, t: u8) -> Rgb {
        hct(self.hue, self.chroma, f32::from(t))
    }
}

/// Role name -> colour for one scheme. Role names follow the Material
/// colour-role vocabulary (primary, on_primary, ...).
pub fn scheme(seed: Rgb, mode: Scheme, contrast: ContrastLevel) -> Vec<(&'static str, Rgb)> {
    let (h, c) = hue_chroma(seed);
    let primary = TonalPalette::new(h, c.max(48.0));
    let secondary = TonalPalette::new(h, 16.0);
    let tertiary = TonalPalette::new(h + 60.0, 24.0);
    let neutral = TonalPalette::new(h, 4.0);
    let nvar = TonalPalette::new(h, 8.0);
    let error = TonalPalette::new(25.0, 84.0);
    let dark = mode == Scheme::Dark;
    // Tone shifts for contrast levels move accents toward the extremes.
    let k: i16 = match contrast {
        ContrastLevel::Standard => 0,
        ContrastLevel::Medium => 1,
        ContrastLevel::High => 2,
    };
    let t = |base: i16, toward_dark: bool| -> u8 {
        let d = k * 10;
        (if toward_dark { base - d } else { base + d }).clamp(0, 100) as u8
    };
    // light: accents move darker; dark: accents move lighter.
    let acc = |p: &TonalPalette, light: i16, darkv: i16| {
        if dark {
            p.tone(t(darkv, false))
        } else {
            p.tone(t(light, true))
        }
    };
    let on_acc = |p: &TonalPalette, light: i16, darkv: i16| {
        if dark {
            p.tone(t(darkv, true))
        } else {
            p.tone(t(light, false))
        }
    };
    let cont = |p: &TonalPalette, light: i16, darkv: i16| {
        if dark {
            p.tone(darkv as u8)
        } else {
            p.tone(light as u8)
        }
    };
    let n = |light: u8, darkv: u8| {
        if dark {
            neutral.tone(darkv)
        } else {
            neutral.tone(light)
        }
    };
    let nv = |light: u8, darkv: u8| {
        if dark {
            nvar.tone(darkv)
        } else {
            nvar.tone(light)
        }
    };
    vec![
        ("primary", acc(&primary, 40, 80)),
        ("on_primary", on_acc(&primary, 100, 20)),
        ("primary_container", cont(&primary, 90, 30)),
        ("on_primary_container", cont(&primary, 10, 90)),
        ("secondary", acc(&secondary, 40, 80)),
        ("on_secondary", on_acc(&secondary, 100, 20)),
        ("secondary_container", cont(&secondary, 90, 30)),
        ("tertiary", acc(&tertiary, 40, 80)),
        ("on_tertiary", on_acc(&tertiary, 100, 20)),
        ("error", acc(&error, 40, 80)),
        ("on_error", on_acc(&error, 100, 20)),
        ("surface", n(99, 6)),
        ("surface_container_low", n(96, 10)),
        ("surface_container", n(94, 12)),
        ("surface_container_high", n(92, 17)),
        (
            "on_surface",
            if k == 2 {
                if dark {
                    neutral.tone(100)
                } else {
                    neutral.tone(0)
                }
            } else {
                n(10, 90)
            },
        ),
        (
            "on_surface_variant",
            nv(if k >= 1 { 20 } else { 30 }, if k >= 1 { 90 } else { 80 }),
        ),
        (
            "outline",
            nv(if k >= 1 { 30 } else { 50 }, if k >= 1 { 80 } else { 60 }),
        ),
        ("outline_variant", nv(80, 30)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::contrast;

    fn lum(c: Rgb) -> f32 {
        c.luminance()
    }
    fn get(s: &[(&'static str, Rgb)], k: &str) -> Rgb {
        s.iter().find(|(n, _)| *n == k).unwrap().1
    }

    #[test]
    fn lab_roundtrip_via_hct() {
        let c = Rgb::from_hex("#3366cc").unwrap();
        let (l, a, b) = to_lab(c);
        let (h, ch) = hue_chroma(c);
        let r = hct(h, ch, l);
        assert!(
            (r.r - c.r).abs() < 0.02 && (r.g - c.g).abs() < 0.02 && (r.b - c.b).abs() < 0.02,
            "{r:?} vs {c:?} ({a},{b})"
        );
    }

    #[test]
    fn tone_extremes_and_monotonic() {
        let p = TonalPalette::new(260.0, 48.0);
        assert_eq!(p.tone(0), Rgb::new(0.0, 0.0, 0.0));
        assert_eq!(p.tone(100), Rgb::new(1.0, 1.0, 1.0));
        let mut prev = -1.0;
        for t in TONES {
            let l = lum(p.tone(t));
            assert!(l >= prev - 1e-4, "tone {t}");
            prev = l;
        }
    }

    #[test]
    fn in_gamut_always() {
        for hue in (0..360).step_by(15) {
            for t in TONES {
                let c = hct(hue as f32, 120.0, f32::from(t));
                for v in [c.r, c.g, c.b] {
                    assert!((0.0..=1.0).contains(&v));
                }
            }
        }
    }

    #[test]
    fn hue_preserved() {
        let seed = Rgb::from_hex("#d97757").unwrap();
        let (h, _) = hue_chroma(seed);
        let s = scheme(seed, Scheme::Light, ContrastLevel::Standard);
        let (ph, pc) = hue_chroma(get(&s, "primary"));
        assert!(pc > 20.0);
        let d = (ph - h).abs().min(360.0 - (ph - h).abs());
        assert!(d < 10.0, "hue drift {d}");
    }

    #[test]
    fn pairs_meet_contrast() {
        for seed in ["#d97757", "#0a84ff", "#2e7d32", "#ffd600", "#888888"] {
            let seed = Rgb::from_hex(seed).unwrap();
            for mode in [Scheme::Light, Scheme::Dark] {
                for (lvl, min) in [(ContrastLevel::Standard, 4.5), (ContrastLevel::High, 7.0)] {
                    let s = scheme(seed, mode, lvl);
                    for (a, b) in [
                        ("primary", "on_primary"),
                        ("surface", "on_surface"),
                        ("primary_container", "on_primary_container"),
                        ("error", "on_error"),
                    ] {
                        let r = contrast(lum(get(&s, a)), lum(get(&s, b)));
                        assert!(r >= min - 0.05, "{a}/{b} {mode:?} {lvl:?} = {r}");
                    }
                }
            }
        }
    }

    #[test]
    fn deterministic_and_dark_surface_is_dark() {
        let seed = Rgb::from_hex("#6750a4").unwrap();
        let a = scheme(seed, Scheme::Dark, ContrastLevel::Medium);
        let b = scheme(seed, Scheme::Dark, ContrastLevel::Medium);
        assert_eq!(a, b);
        assert!(lum(get(&a, "surface")) < 0.02);
        assert!(
            lum(get(
                &scheme(seed, Scheme::Light, ContrastLevel::Standard),
                "surface"
            )) > 0.9
        );
    }
}
