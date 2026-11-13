use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rgb {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgb {
    pub const fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    /// `#rrggbb` (alpha, if present, is ignored).
    pub fn from_hex(s: &str) -> Option<Rgb> {
        let h = s.strip_prefix('#')?;
        if !(h.len() == 6 || h.len() == 8) || !h.is_ascii() {
            return None;
        }
        let p = |i: usize| {
            u8::from_str_radix(&h[i..i + 2], 16)
                .ok()
                .map(|v| f32::from(v) / 255.0)
        };
        Some(Rgb::new(p(0)?, p(2)?, p(4)?))
    }

    pub fn mix(self, other: Rgb, t: f32) -> Rgb {
        let t = t.clamp(0.0, 1.0);
        Rgb::new(
            self.r + (other.r - self.r) * t,
            self.g + (other.g - self.g) * t,
            self.b + (other.b - self.b) * t,
        )
    }

    pub fn with_alpha(self, a: f32) -> Rgba {
        Rgba {
            r: self.r,
            g: self.g,
            b: self.b,
            a: a.clamp(0.0, 1.0),
        }
    }

    /// WCAG relative luminance.
    pub fn luminance(self) -> f32 {
        fn lin(c: f32) -> f32 {
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * lin(self.r) + 0.7152 * lin(self.g) + 0.0722 * lin(self.b)
    }
}

/// WCAG contrast ratio between two luminances.
pub fn contrast(l1: f32, l2: f32) -> f32 {
    let (hi, lo) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
    (hi + 0.05) / (lo + 0.05)
}

fn byte(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

impl Rgba {
    pub fn css(&self) -> String {
        format!(
            "rgba({}, {}, {}, {:.3})",
            byte(self.r),
            byte(self.g),
            byte(self.b),
            self.a
        )
    }
    pub fn hex(&self) -> String {
        format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            byte(self.r),
            byte(self.g),
            byte(self.b),
            byte(self.a)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_parse() {
        let c = Rgb::from_hex("#ff8000").unwrap();
        assert_eq!(byte(c.r), 255);
        assert_eq!(byte(c.g), 128);
        assert!(Rgb::from_hex("ff8000").is_none());
        assert!(Rgb::from_hex("#ggg000").is_none());
        assert!(Rgb::from_hex("#é1234").is_none());
    }

    #[test]
    fn luminance_and_contrast() {
        assert!((Rgb::new(1.0, 1.0, 1.0).luminance() - 1.0).abs() < 1e-4);
        assert!(Rgb::new(0.0, 0.0, 0.0).luminance().abs() < 1e-6);
        assert!((contrast(1.0, 0.0) - 21.0).abs() < 1e-3);
    }

    #[test]
    fn css_format() {
        assert_eq!(
            Rgb::new(1.0, 0.0, 0.0).with_alpha(0.5).css(),
            "rgba(255, 0, 0, 0.500)"
        );
        assert_eq!(Rgb::new(1.0, 0.0, 0.0).with_alpha(1.0).hex(), "#ff0000ff");
    }
}
