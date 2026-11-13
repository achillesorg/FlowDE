//! Inputs and outputs of material resolution.

use crate::color::{Rgb, Rgba};
use flow_common::events::{PowerSnapshot, SensorSnapshot};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Scheme {
    #[default]
    Light,
    Dark,
}

/// Regular: frosted, legible anywhere. Clear: highly transparent, for use over
/// media-rich backdrops; relies on lensing and rim light for definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Variant {
    #[default]
    Regular,
    Clear,
}

/// Quality ladder. Ordering: Flat < BlurOnly < Full.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    /// Solid fill, no backdrop sampling.
    Flat,
    /// Backdrop blur + tint + static rim; no lensing.
    BlurOnly,
    /// Blur + refraction/edge lensing + specular rim + motion highlight.
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Accessibility {
    pub reduce_transparency: bool,
    pub increase_contrast: bool,
    pub reduce_motion: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum GpuClass {
    None,
    Low,
    Mid,
    #[default]
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub theme: crate::themes::ThemeKind,
    pub contrast: crate::material_you::ContrastLevel,
    /// Seed for dynamic colour (wallpaper dominant colour); falls back to `accent`.
    pub seed: Option<Rgb>,
    pub scheme: Scheme,
    pub variant: Variant,
    /// Tint glass towards the accent colour.
    pub tinted: bool,
    pub accent: Rgb,
    pub a11y: Accessibility,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            theme: Default::default(),
            contrast: Default::default(),
            seed: None,
            scheme: Scheme::Light,
            variant: Variant::Regular,
            tinted: false,
            accent: Rgb::new(10.0 / 255.0, 132.0 / 255.0, 1.0),
            a11y: Accessibility::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Environment {
    pub gpu: GpuClass,
    pub power: PowerSnapshot,
    pub sensors: SensorSnapshot,
    /// Estimated relative luminance (0..1) of what is behind the glass,
    /// supplied by the shell when known.
    pub content_luma: Option<f32>,
}

/// Concentric corner radii: inner = outer - inset (never negative).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Corners {
    pub window: f32,
    pub card: f32,
    pub control: f32,
    pub chip: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Material {
    pub quality: Quality,
    pub scheme: Scheme,
    pub variant: Variant,
    /// Backdrop blur radius in logical px (0 when Flat).
    pub blur_radius_px: f32,
    /// Backdrop saturation multiplier.
    pub saturation: f32,
    /// Lens strength 0..1 (0 unless Full).
    pub refraction: f32,
    /// Max edge displacement of the backdrop sample, px.
    pub edge_displacement_px: f32,
    /// Width of the lensing bezel, px.
    pub bezel_px: f32,
    /// Specular rim intensity 0..1.
    pub specular: f32,
    /// Light direction, degrees clockwise from "up".
    pub rim_angle_deg: f32,
    pub rim_width_px: f32,
    /// Extra highlight offset from device tilt, px (0 when motion is off).
    pub highlight_offset_px: (f32, f32),
    pub motion: bool,
    pub tint: Rgba,
    pub border: Rgba,
    pub label: Rgba,
    pub label_secondary: Rgba,
    pub shadow: Rgba,
    pub corners: Corners,
    /// Predicted label/backdrop contrast after adaptation.
    pub contrast_ratio: f32,
    /// Why the quality/adaptation ended up as it did (for debugging UIs).
    pub reasons: Vec<String>,
}

/// Concentric corner: inner radius for a child inset by `inset` inside `outer`.
pub fn concentric(outer: f32, inset: f32) -> f32 {
    (outer - inset).max(0.0)
}

/// Capsule radius for a control of the given height.
pub fn capsule(height: f32) -> f32 {
    height / 2.0
}
