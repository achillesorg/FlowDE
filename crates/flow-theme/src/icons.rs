//! Icon packs: independent of the interface theme, discovered from XDG icon
//! directories following the freedesktop Icon Theme Specification (index.theme,
//! `Inherits` fallback chain ending in hicolor).
//!
//! FlowDE extension keys in `index.theme` (all optional):
//! `X-Flow-Style=glass|line|filled|ink`, `X-Flow-License=<SPDX or text>`,
//! `X-Flow-Recolor=true` (monochrome glyphs that may be recoloured to text colour).

use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum IconStyle {
    Glass,
    Line,
    Filled,
    Ink,
}

impl IconStyle {
    pub fn parse(s: &str) -> IconStyle {
        match s {
            "glass" => IconStyle::Glass,
            "filled" => IconStyle::Filled,
            "ink" => IconStyle::Ink,
            _ => IconStyle::Line,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct IconDir {
    pub path: String,
    pub size: u32,
    pub min: u32,
    pub max: u32,
    pub scalable: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct IconPack {
    pub id: String,
    pub name: String,
    pub license: String,
    pub style: IconStyle,
    pub recolorable: bool,
    pub inherits: Vec<String>,
    /// Sizes available (scalable packs list 0).
    pub sizes: Vec<u32>,
    pub scalable: bool,
    #[serde(skip)]
    pub root: PathBuf,
    #[serde(skip)]
    pub dirs: Vec<IconDir>,
}

/// Search roots in precedence order.
pub fn search_roots() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share"));
    let mut v = vec![
        home.join(".icons"),
        data_home.join("icons"),
        data_home.join("flowde/icon-packs"),
    ];
    let dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    for d in dirs.split(':').filter(|d| !d.is_empty()) {
        v.push(Path::new(d).join("icons"));
        v.push(Path::new(d).join("flowde/icon-packs"));
    }
    v
}

type Ini = BTreeMap<String, BTreeMap<String, String>>;

fn parse_ini(text: &str) -> Ini {
    let mut out: Ini = BTreeMap::new();
    let mut cur = String::new();
    for l in text.lines() {
        let l = l.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        if let Some(s) = l.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            cur = s.to_string();
        } else if let Some((k, v)) = l.split_once('=') {
            out.entry(cur.clone())
                .or_default()
                .entry(k.trim().to_string())
                .or_insert_with(|| v.trim().to_string());
        }
    }
    out
}

pub fn parse_pack(id: &str, root: &Path, index: &str) -> Option<IconPack> {
    let ini = parse_ini(index);
    let main = ini.get("Icon Theme")?;
    let list = |k: &str| -> Vec<String> {
        main.get(k)
            .map(|v| {
                v.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut dirs = vec![];
    for d in list("Directories") {
        let Some(sec) = ini.get(&d) else { continue };
        let size: u32 = sec.get("Size").and_then(|s| s.parse().ok()).unwrap_or(0);
        let ty = sec.get("Type").map(String::as_str).unwrap_or("Threshold");
        let scalable = ty == "Scalable";
        let min = sec
            .get("MinSize")
            .and_then(|s| s.parse().ok())
            .unwrap_or(size);
        let max = sec
            .get("MaxSize")
            .and_then(|s| s.parse().ok())
            .unwrap_or(size);
        dirs.push(IconDir {
            path: d,
            size,
            min,
            max,
            scalable,
        });
    }
    let mut sizes: Vec<u32> = dirs
        .iter()
        .map(|d| if d.scalable { 0 } else { d.size })
        .collect();
    sizes.sort_unstable();
    sizes.dedup();
    Some(IconPack {
        id: id.to_string(),
        name: main.get("Name").cloned().unwrap_or_else(|| id.to_string()),
        license: main
            .get("X-Flow-License")
            .cloned()
            .unwrap_or_else(|| "unknown".into()),
        style: IconStyle::parse(
            main.get("X-Flow-Style")
                .map(String::as_str)
                .unwrap_or("line"),
        ),
        recolorable: main.get("X-Flow-Recolor").is_some_and(|v| v == "true"),
        inherits: list("Inherits"),
        scalable: dirs.iter().any(|d| d.scalable),
        sizes,
        root: root.to_path_buf(),
        dirs,
    })
}

#[derive(Debug, Default)]
pub struct Registry {
    pub packs: BTreeMap<String, IconPack>,
}

impl Registry {
    /// Earlier roots win for duplicate ids.
    pub fn discover(roots: &[PathBuf]) -> Registry {
        let mut r = Registry::default();
        for root in roots {
            let Ok(rd) = std::fs::read_dir(root) else {
                continue;
            };
            for e in rd.flatten() {
                let id = e.file_name().to_string_lossy().into_owned();
                if r.packs.contains_key(&id) {
                    continue;
                }
                if let Ok(text) = std::fs::read_to_string(e.path().join("index.theme")) {
                    if let Some(p) = parse_pack(&id, &e.path(), &text) {
                        r.packs.insert(id, p);
                    }
                }
            }
        }
        r
    }

    /// Fallback chain: pack, its Inherits (depth-first), then hicolor. No cycles.
    pub fn chain(&self, id: &str) -> Vec<&IconPack> {
        fn walk<'a>(r: &'a Registry, id: &str, out: &mut Vec<&'a IconPack>) {
            if out.iter().any(|p| p.id == id) {
                return;
            }
            if let Some(p) = r.packs.get(id) {
                out.push(p);
                for i in &p.inherits {
                    walk(r, i, out);
                }
            }
        }
        let mut out = vec![];
        walk(self, id, &mut out);
        walk(self, "hicolor", &mut out);
        out
    }

    /// Resolve an icon name to a file, honouring the chain. Prefers the
    /// closest size; scalable (svg) wins ties.
    pub fn lookup(&self, pack: &str, name: &str, size: u32) -> Option<PathBuf> {
        for p in self.chain(pack) {
            let mut best: Option<(u32, bool, PathBuf)> = None;
            for d in &p.dirs {
                for ext in ["svg", "png"] {
                    let f = p.root.join(&d.path).join(format!("{name}.{ext}"));
                    if !f.exists() {
                        continue;
                    }
                    let dist = if d.scalable || (d.min..=d.max).contains(&size) {
                        0
                    } else {
                        d.size.abs_diff(size)
                    };
                    let key = (dist, !d.scalable);
                    if best.as_ref().is_none_or(|b| key < (b.0, b.1)) {
                        best = Some((dist, !d.scalable, f));
                    }
                }
            }
            if let Some((_, _, f)) = best {
                return Some(f);
            }
        }
        None
    }

    /// Pick the pack to use: an installed requested id, else the first
    /// installed of `fallbacks`, else "hicolor".
    pub fn pick<'a>(&'a self, wanted: &'a str, fallbacks: &[&'a str]) -> &'a str {
        std::iter::once(wanted)
            .chain(fallbacks.iter().copied())
            .find(|id| self.packs.contains_key(*id))
            .unwrap_or("hicolor")
    }
}

/// Default pack id per interface theme (a preference, falls back if absent).
pub fn default_pack(theme: crate::themes::ThemeKind) -> &'static str {
    use crate::themes::ThemeKind::*;
    match theme {
        LiquidGlass => "flow-glass",
        Minimalist => "flow-line",
        MaterialYou => "flow-filled",
        AnthropicDesign => "fede-ink",
    }
}

/// Candidate packs documented for users (none are bundled or downloaded here).
pub const CANDIDATES: &[(&str, &str)] = &[
    (
        "flow-glass",
        "original FlowDE glass pack (future work, MIT)",
    ),
    (
        "fede-ink",
        "FeDe ink-outline set by the same author (user's own work)",
    ),
    (
        "Adwaita",
        "GNOME Adwaita icons, LGPL-3.0 / CC-BY-SA-3.0 (fallback)",
    ),
    ("hicolor", "freedesktop mandatory fallback theme, GPL-2.0+"),
    (
        "Papirus",
        "optional third-party, GPL-3.0; user installs it themselves",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("flow-icons-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn mk(root: &Path, id: &str, index: &str, files: &[&str]) {
        let d = root.join(id);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("index.theme"), index).unwrap();
        for f in files {
            let p = d.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, "x").unwrap();
        }
    }

    const INK: &str = "[Icon Theme]\nName=Ink\nInherits=hicolor\nDirectories=scalable/apps,24x24/apps\nX-Flow-Style=ink\nX-Flow-License=MIT\nX-Flow-Recolor=true\n[scalable/apps]\nSize=48\nType=Scalable\nMinSize=16\nMaxSize=512\n[24x24/apps]\nSize=24\nType=Fixed\n";
    const HI: &str =
        "[Icon Theme]\nName=Hicolor\nDirectories=48x48/apps\n[48x48/apps]\nSize=48\nType=Fixed\n";

    #[test]
    fn parse_metadata() {
        let p = parse_pack("ink", Path::new("/x"), INK).unwrap();
        assert_eq!(p.name, "Ink");
        assert_eq!(p.style, IconStyle::Ink);
        assert!(p.recolorable && p.scalable);
        assert_eq!(p.sizes, vec![0, 24]);
        assert_eq!(p.license, "MIT");
        assert!(parse_pack("x", Path::new("/x"), "garbage").is_none());
    }

    #[test]
    fn chain_and_lookup_fallback() {
        let r = tmp("a");
        mk(&r, "ink", INK, &["scalable/apps/files.svg"]);
        mk(
            &r,
            "hicolor",
            HI,
            &["48x48/apps/files.png", "48x48/apps/other.png"],
        );
        let reg = Registry::discover(std::slice::from_ref(&r));
        let ids: Vec<_> = reg.chain("ink").iter().map(|p| p.id.clone()).collect();
        assert_eq!(ids, vec!["ink", "hicolor"]);
        assert!(reg
            .lookup("ink", "files", 32)
            .unwrap()
            .to_string_lossy()
            .ends_with("scalable/apps/files.svg"));
        assert!(reg
            .lookup("ink", "other", 32)
            .unwrap()
            .to_string_lossy()
            .ends_with("48x48/apps/other.png"));
        assert!(reg.lookup("ink", "missing", 32).is_none());
        std::fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn inherits_cycle_safe_and_pick() {
        let r = tmp("c");
        mk(
            &r,
            "a",
            "[Icon Theme]\nName=A\nInherits=b\nDirectories=\n",
            &[],
        );
        mk(
            &r,
            "b",
            "[Icon Theme]\nName=B\nInherits=a\nDirectories=\n",
            &[],
        );
        let reg = Registry::discover(std::slice::from_ref(&r));
        assert_eq!(reg.chain("a").len(), 2);
        assert_eq!(reg.pick("flow-glass", &["b"]), "b");
        assert_eq!(reg.pick("nope", &[]), "hicolor");
        std::fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn earlier_root_wins() {
        let a = tmp("w1");
        let b = tmp("w2");
        mk(&a, "p", "[Icon Theme]\nName=First\nDirectories=\n", &[]);
        mk(&b, "p", "[Icon Theme]\nName=Second\nDirectories=\n", &[]);
        let reg = Registry::discover(&[a.clone(), b.clone()]);
        assert_eq!(reg.packs["p"].name, "First");
        std::fs::remove_dir_all(a).unwrap();
        std::fs::remove_dir_all(b).unwrap();
    }

    #[test]
    fn theme_defaults() {
        use crate::themes::ThemeKind;
        assert_eq!(default_pack(ThemeKind::AnthropicDesign), "fede-ink");
        assert_eq!(default_pack(ThemeKind::LiquidGlass), "flow-glass");
    }
}
