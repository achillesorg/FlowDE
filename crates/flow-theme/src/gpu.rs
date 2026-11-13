//! GPU capability classification from the DRM kernel driver name.

use crate::model::GpuClass;

/// Map a kernel DRM driver name to a coarse class. Unknown drivers are Low
/// (safe: blur only) rather than guessing high.
pub fn class_from_driver(driver: &str) -> GpuClass {
    match driver {
        "i915" | "xe" | "amdgpu" | "radeon" | "nouveau" | "nvidia" => GpuClass::High,
        "msm" | "panfrost" | "panthor" | "v3d" | "asahi" | "etnaviv-gc7000" | "tegra" => {
            GpuClass::Mid
        }
        "lima" | "vc4" | "etnaviv" | "virtio_gpu" | "vgem" | "bochs-drm" | "simpledrm"
        | "simple-framebuffer" => GpuClass::Low,
        _ => GpuClass::Low,
    }
}

/// Best class among `/sys/class/drm/card*/device/driver` symlink targets.
/// `root` is normally `/sys/class/drm`.
pub fn detect(root: &std::path::Path) -> GpuClass {
    let mut best: Option<GpuClass> = None;
    if let Ok(rd) = std::fs::read_dir(root) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            // cardN only, not cardN-HDMI-A-1 connectors
            if !name.starts_with("card") || name.contains('-') {
                continue;
            }
            if let Ok(link) = std::fs::read_link(e.path().join("device/driver")) {
                if let Some(d) = link.file_name().and_then(|f| f.to_str()) {
                    let c = class_from_driver(d);
                    best = Some(best.map_or(c, |b| b.max(c)));
                }
            }
        }
    }
    best.unwrap_or(GpuClass::None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes() {
        assert_eq!(class_from_driver("amdgpu"), GpuClass::High);
        assert_eq!(class_from_driver("panfrost"), GpuClass::Mid);
        assert_eq!(class_from_driver("virtio_gpu"), GpuClass::Low);
        assert_eq!(class_from_driver("weird"), GpuClass::Low);
    }

    #[cfg(unix)]
    #[test]
    fn detect_from_sysfs_layout() {
        let d = std::env::temp_dir().join(format!("flow-gpu-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let drv = d.join("drivers/panfrost");
        std::fs::create_dir_all(&drv).unwrap();
        let card = d.join("drm/card0/device");
        std::fs::create_dir_all(&card).unwrap();
        std::os::unix::fs::symlink(&drv, card.join("driver")).unwrap();
        std::fs::create_dir_all(d.join("drm/card0-HDMI-A-1")).unwrap();
        assert_eq!(detect(&d.join("drm")), GpuClass::Mid);
        assert_eq!(detect(&d.join("nonexistent")), GpuClass::None);
        std::fs::remove_dir_all(d).unwrap();
    }
}
