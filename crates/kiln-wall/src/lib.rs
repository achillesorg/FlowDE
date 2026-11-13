//! Wallpaper logic: config model, rotation, image decoding (PPM, PNG) and the
//! dominant-colour seed used by the MaterialYou theme. JPEG decoding is a
//! TODO(feature) (would add a decoder dependency).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FitMode {
    #[default]
    Fill,
    Fit,
    Center,
    Stretch,
    Tile,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Entry {
    pub light: Option<PathBuf>,
    /// Falls back to `light` when absent.
    pub dark: Option<PathBuf>,
    pub mode: FitMode,
}

impl Entry {
    pub fn for_scheme(&self, dark: bool) -> Option<&Path> {
        if dark {
            self.dark.as_deref().or(self.light.as_deref())
        } else {
            self.light.as_deref().or(self.dark.as_deref())
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Rotation {
    pub paths: Vec<PathBuf>,
    pub interval_min: u32,
    pub index: usize,
}

impl Rotation {
    pub fn enabled(&self) -> bool {
        self.interval_min > 0 && self.paths.len() > 1
    }
    /// Advance and return the new current path.
    pub fn advance(&mut self) -> Option<&Path> {
        if self.paths.is_empty() {
            return None;
        }
        self.index = (self.index + 1) % self.paths.len();
        self.paths.get(self.index).map(PathBuf::as_path)
    }
    pub fn current(&self) -> Option<&Path> {
        self.paths
            .get(self.index % self.paths.len().max(1))
            .map(PathBuf::as_path)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Output name ("*" = default for all outputs).
    pub outputs: BTreeMap<String, Entry>,
    pub rotation: Rotation,
}

impl Config {
    pub fn entry_for(&self, output: &str) -> Option<&Entry> {
        self.outputs.get(output).or_else(|| self.outputs.get("*"))
    }
}

/// Only regular files with a known image extension, absolute, no `..`.
pub fn validate_path(p: &Path) -> Result<(), String> {
    if !p.is_absolute()
        || p.components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("path must be absolute without '..'".into());
    }
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !["png", "jpg", "jpeg", "ppm", "webp", "avif", "svg"].contains(&ext.as_str()) {
        return Err("unsupported image type".into());
    }
    Ok(())
}

pub struct Image {
    pub w: usize,
    pub h: usize,
    /// RGB8, row-major.
    pub rgb: Vec<u8>,
}

pub const MAX_DIM: usize = 16_384;

pub fn decode_ppm(data: &[u8]) -> Result<Image, String> {
    let mut pos = 0;
    let mut tok = || -> Option<String> {
        while pos < data.len() && (data[pos].is_ascii_whitespace() || data[pos] == b'#') {
            if data[pos] == b'#' {
                while pos < data.len() && data[pos] != b'\n' {
                    pos += 1;
                }
            } else {
                pos += 1;
            }
        }
        let s = pos;
        while pos < data.len() && !data[pos].is_ascii_whitespace() {
            pos += 1;
        }
        (pos > s).then(|| String::from_utf8_lossy(&data[s..pos]).into_owned())
    };
    if tok().as_deref() != Some("P6") {
        return Err("not a P6 ppm".into());
    }
    let w: usize = tok().and_then(|t| t.parse().ok()).ok_or("bad width")?;
    let h: usize = tok().and_then(|t| t.parse().ok()).ok_or("bad height")?;
    let max: usize = tok().and_then(|t| t.parse().ok()).ok_or("bad maxval")?;
    if max != 255 || w == 0 || h == 0 || w > MAX_DIM || h > MAX_DIM {
        return Err("unsupported ppm".into());
    }
    pos += 1; // single whitespace after maxval
    let need = w * h * 3;
    let px = data.get(pos..pos + need).ok_or("truncated ppm")?;
    Ok(Image {
        w,
        h,
        rgb: px.to_vec(),
    })
}

pub fn decode_png(data: &[u8]) -> Result<Image, String> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(data));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = dec.read_info().map_err(|e| e.to_string())?;
    let (w, h) = (r.info().width as usize, r.info().height as usize);
    if w == 0 || h == 0 || w > MAX_DIM || h > MAX_DIM || w * h > 64_000_000 {
        return Err("image too large".into());
    }
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let n = match info.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        _ => return Err("unsupported png colour type".into()),
    };
    let mut rgb = Vec::with_capacity(w * h * 3);
    for px in buf[..info.buffer_size()].chunks_exact(n) {
        match n {
            1 | 2 => rgb.extend([px[0]; 3]),
            _ => rgb.extend(&px[..3]),
        }
    }
    Ok(Image { w, h, rgb })
}

pub fn decode(path: &Path) -> Result<Image, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.len() > 64 << 20 {
        return Err("file too large".into());
    }
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => decode_png(&data),
        Some("ppm") => decode_ppm(&data),
        _ => Err("decoder not available for this format (TODO(feature): jpeg/webp)".into()),
    }
}

fn hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let (r, g, b) = (
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
    );
    let mx = r.max(g).max(b);
    let mn = r.min(g).min(b);
    let d = mx - mn;
    let h = if d == 0.0 {
        0.0
    } else if mx == r {
        60.0 * (((g - b) / d).rem_euclid(6.0))
    } else if mx == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, if mx == 0.0 { 0.0 } else { d / mx }, mx)
}

/// Dominant colour as `#rrggbb`: hue histogram weighted by saturation, ignoring
/// near-black/white/grey pixels. Greyscale images return their mean grey.
/// Samples at most ~40k pixels.
pub fn dominant_seed(img: &Image) -> String {
    let total = img.w * img.h;
    let step = (total / 40_000).max(1);
    let mut bins = [(0f32, 0f64, 0f64, 0f64); 24]; // weight, sum r, g, b
    let (mut mean, mut n) = (0f64, 0f64);
    for i in (0..total).step_by(step) {
        let (r, g, b) = (img.rgb[i * 3], img.rgb[i * 3 + 1], img.rgb[i * 3 + 2]);
        mean += (f64::from(r) + f64::from(g) + f64::from(b)) / 3.0;
        n += 1.0;
        let (h, s, v) = hsv(r, g, b);
        if s < 0.25 || !(0.2..=0.95).contains(&v) {
            continue;
        }
        let bin = ((h / 15.0) as usize).min(23);
        let w = s * (1.0 - (v - 0.6).abs());
        bins[bin].0 += w;
        bins[bin].1 += f64::from(r) * f64::from(w);
        bins[bin].2 += f64::from(g) * f64::from(w);
        bins[bin].3 += f64::from(b) * f64::from(w);
    }
    match bins.iter().max_by(|a, b| a.0.total_cmp(&b.0)) {
        Some(b) if b.0 > 0.0 => {
            let w = f64::from(b.0);
            format!(
                "#{:02x}{:02x}{:02x}",
                (b.1 / w).round() as u8,
                (b.2 / w).round() as u8,
                (b.3 / w).round() as u8
            )
        }
        _ => {
            let g = if n > 0.0 {
                (mean / n).round() as u8
            } else {
                128
            };
            format!("#{g:02x}{g:02x}{g:02x}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: usize, h: usize, c: [u8; 3]) -> Image {
        Image {
            w,
            h,
            rgb: (0..w * h).flat_map(|_| c).collect(),
        }
    }

    #[test]
    fn seed_picks_saturated_dominant_hue() {
        let mut img = solid(100, 100, [200, 200, 200]); // grey background
        for i in 0..4000 {
            img.rgb[i * 3..i * 3 + 3].copy_from_slice(&[40, 90, 200]); // blue region
        }
        for i in 4000..4500 {
            img.rgb[i * 3..i * 3 + 3].copy_from_slice(&[200, 40, 40]); // smaller red region
        }
        assert_eq!(dominant_seed(&img), "#285ac8");
    }

    #[test]
    fn grey_image_gets_grey_seed() {
        assert_eq!(dominant_seed(&solid(10, 10, [100, 100, 100])), "#646464");
        assert_eq!(dominant_seed(&solid(10, 10, [255, 255, 255])), "#ffffff");
    }

    #[test]
    fn ppm_decode() {
        let mut d = b"P6\n# c\n2 1\n255\n".to_vec();
        d.extend([1, 2, 3, 4, 5, 6]);
        let i = decode_ppm(&d).unwrap();
        assert_eq!((i.w, i.h, i.rgb.len()), (2, 1, 6));
        assert!(decode_ppm(b"P6\n2 1\n255\n\x01").is_err());
        assert!(decode_ppm(b"P5\n").is_err());
        assert!(decode_ppm(b"P6\n99999999 1\n255\n").is_err());
    }

    #[test]
    fn png_roundtrip() {
        let mut out = vec![];
        {
            let mut enc = png::Encoder::new(&mut out, 2, 2);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().unwrap();
            w.write_image_data(&[255, 0, 0, 255, 0, 0, 255, 0, 0, 255, 0, 0])
                .unwrap();
        }
        let i = decode_png(&out).unwrap();
        assert_eq!((i.w, i.h), (2, 2));
        assert_eq!(&i.rgb[..3], &[255, 0, 0]);
        assert!(decode_png(b"garbage").is_err());
    }

    #[test]
    fn entry_variants_and_rotation() {
        let e = Entry {
            light: Some("/a.png".into()),
            dark: None,
            mode: FitMode::Fill,
        };
        assert_eq!(e.for_scheme(true), Some(Path::new("/a.png")));
        let mut c = Config::default();
        c.outputs.insert("*".into(), e);
        assert!(c.entry_for("DP-1").is_some());
        let mut r = Rotation {
            paths: vec!["/1.png".into(), "/2.png".into(), "/3.png".into()],
            interval_min: 10,
            index: 0,
        };
        assert!(r.enabled());
        assert_eq!(r.advance(), Some(Path::new("/2.png")));
        r.advance();
        assert_eq!(r.advance(), Some(Path::new("/1.png"))); // wraps
        assert!(!Rotation::default().enabled());
        assert!(Rotation::default().current().is_none());
    }

    #[test]
    fn path_validation() {
        assert!(validate_path(Path::new("/home/u/a.PNG")).is_ok());
        assert!(validate_path(Path::new("rel.png")).is_err());
        assert!(validate_path(Path::new("/a/../b.png")).is_err());
        assert!(validate_path(Path::new("/a/b.sh")).is_err());
    }
}
