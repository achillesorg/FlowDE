//! FlowDE liquid-glass material: parameter model, resolution and output.
//!
//! Original implementation. No Apple code, assets or measured Apple
//! constants are used; all numbers here are FlowDE design choices that can be
//! tuned (see docs/liquid-glass.md).

pub mod color;
pub mod governor;
pub mod model;
pub mod output;
pub mod resolve;

pub use color::{Rgb, Rgba};
pub use governor::Governor;
pub use model::*;
pub use resolve::{resolve, target_quality};

pub mod gpu;
pub mod service;

pub mod material_you;

pub mod icons;
pub mod themes;
