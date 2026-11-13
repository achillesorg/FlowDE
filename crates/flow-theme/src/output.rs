//! Output formats: flat token map, GTK CSS, CSS custom properties, JSON.

use crate::model::Material;
use std::collections::BTreeMap;

fn px(v: f32) -> String {
    format!("{:.1}px", v)
}

/// Flat `name -> string` tokens. Names use dots (`glass.blur_radius`).
/// This is what D-Bus `GetTokens` returns and what QML/Qt consumers read.
pub fn tokens(m: &Material) -> BTreeMap<String, String> {
    let mut t = BTreeMap::new();
    let mut put = |k: &str, v: String| {
        t.insert(k.to_string(), v);
    };
    put("quality", format!("{:?}", m.quality).to_lowercase());
    put("scheme", format!("{:?}", m.scheme).to_lowercase());
    put("variant", format!("{:?}", m.variant).to_lowercase());
    put("glass.blur_radius", px(m.blur_radius_px));
    put("glass.saturation", format!("{:.2}", m.saturation));
    put("glass.refraction", format!("{:.3}", m.refraction));
    put("glass.edge_displacement", px(m.edge_displacement_px));
    put("glass.bezel", px(m.bezel_px));
    put("glass.specular", format!("{:.3}", m.specular));
    put("glass.rim_angle", format!("{:.1}deg", m.rim_angle_deg));
    put("glass.rim_width", px(m.rim_width_px));
    put("glass.highlight_x", px(m.highlight_offset_px.0));
    put("glass.highlight_y", px(m.highlight_offset_px.1));
    put("glass.motion", m.motion.to_string());
    put("glass.tint", m.tint.css());
    put("glass.border", m.border.css());
    put("glass.shadow", m.shadow.css());
    put("text.label", m.label.css());
    put("text.label_secondary", m.label_secondary.css());
    put("radius.window", px(m.corners.window));
    put("radius.card", px(m.corners.card));
    put("radius.control", px(m.corners.control));
    put("radius.chip", px(m.corners.chip));
    put("contrast.ratio", format!("{:.2}", m.contrast_ratio));
    t
}

/// CSS custom properties (`--flow-glass-blur-radius`), usable in web views and
/// anything that understands `var()`.
pub fn css_vars(m: &Material) -> String {
    let mut s = String::from(":root {\n");
    for (k, v) in tokens(m) {
        s.push_str(&format!(
            "  --flow-{}: {};\n",
            k.replace(['.', '_'], "-"),
            v
        ));
    }
    s.push_str("}\n");
    s
}

/// GTK4 CSS. GTK has no custom properties (it uses `@define-color`) and no
/// backdrop-filter: blur/lensing are performed by the compositor using the
/// tokens, so this only styles the fill, border, radius, text and shadow.
pub fn gtk_css(m: &Material) -> String {
    let mut s = String::new();
    s.push_str(&format!("/* flow-theme: quality={:?} */\n", m.quality));
    s.push_str(&format!(
        "@define-color flow_glass_tint {};\n",
        m.tint.css()
    ));
    s.push_str(&format!(
        "@define-color flow_glass_border {};\n",
        m.border.css()
    ));
    s.push_str(&format!("@define-color flow_label {};\n", m.label.css()));
    s.push_str(&format!(
        "@define-color flow_label_secondary {};\n",
        m.label_secondary.css()
    ));
    s.push_str(&format!(
        ".flow-glass {{ background-color: @flow_glass_tint; border: 1px solid @flow_glass_border; border-radius: {}; color: @flow_label; box-shadow: 0 8px 24px {}; }}\n",
        px(m.corners.window),
        m.shadow.css()
    ));
    s.push_str(&format!(
        ".flow-glass-card {{ border-radius: {}; }}\n",
        px(m.corners.card)
    ));
    s.push_str(&format!(
        ".flow-glass-control {{ border-radius: {}; }}\n",
        px(m.corners.control)
    ));
    s.push_str(&format!(
        ".flow-glass-chip {{ border-radius: {}; }}\n",
        px(m.corners.chip)
    ));
    s.push_str(".flow-glass .dim-label { color: @flow_label_secondary; }\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use crate::resolve::resolve;

    fn mat() -> Material {
        resolve(
            &Preferences::default(),
            &Environment::default(),
            Quality::Full,
        )
    }

    #[test]
    fn tokens_complete() {
        let t = tokens(&mat());
        for k in [
            "glass.blur_radius",
            "glass.tint",
            "radius.card",
            "quality",
            "contrast.ratio",
        ] {
            assert!(t.contains_key(k), "{k}");
        }
        assert_eq!(t["quality"], "full");
    }

    #[test]
    fn css_vars_names() {
        let c = css_vars(&mat());
        assert!(c.contains("--flow-glass-blur-radius: 28.0px;"));
        assert!(c.contains("--flow-radius-card: 22.0px;"));
    }

    #[test]
    fn gtk_has_define_color_and_no_unsupported_props() {
        let c = gtk_css(&mat());
        assert!(c.contains("@define-color flow_glass_tint rgba("));
        assert!(!c.contains("backdrop-filter"));
        assert!(!c.contains("var("));
    }

    #[test]
    fn json_serialises() {
        let j = serde_json::to_value(mat()).unwrap();
        assert_eq!(j["quality"], "full");
        assert!(j["corners"]["window"].is_number());
    }
}
