//! Registry of installed applications plus running-instance tracking.

use crate::desktop::{self, DesktopEntry};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub struct Registry {
    pub entries: BTreeMap<String, DesktopEntry>,
}

/// `$XDG_DATA_HOME/applications` then each `$XDG_DATA_DIRS`/applications.
pub fn search_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share"));
    let dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    let mut v = vec![data_home.join("applications")];
    v.extend(
        dirs.split(':')
            .filter(|d| !d.is_empty())
            .map(|d| Path::new(d).join("applications")),
    );
    v
}

fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, PathBuf)>, depth: u8) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() && depth < 4 {
            walk(&p, base, out, depth + 1);
        } else if p.extension().is_some_and(|x| x == "desktop") {
            // id: relative path with '/' replaced by '-'
            if let Ok(rel) = p.strip_prefix(base) {
                out.push((rel.to_string_lossy().replace('/', "-"), p.clone()));
            }
        }
    }
}

impl Registry {
    /// Scan directories in precedence order (earlier wins).
    pub fn scan(dirs: &[PathBuf], locale: Option<&str>, desktop_name: &str) -> Registry {
        let mut r = Registry::default();
        let mut hidden: Vec<String> = vec![];
        for d in dirs {
            let mut files = vec![];
            walk(d, d, &mut files, 0);
            files.sort();
            for (id, path) in files {
                if r.entries.contains_key(&id) || hidden.contains(&id) {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let Some(mut e) = desktop::parse(&id, &text, locale) else {
                    continue;
                };
                if e.hidden {
                    hidden.push(id);
                    continue;
                }
                if !e.only_show_in.is_empty() && !e.only_show_in.iter().any(|x| x == desktop_name) {
                    e.no_display = true;
                }
                if e.not_show_in.iter().any(|x| x == desktop_name) {
                    e.no_display = true;
                }
                let abs = path.to_string_lossy().into_owned();
                r.entries.insert(
                    id,
                    DesktopEntry {
                        path: Some(abs),
                        ..e
                    },
                );
            }
        }
        r
    }

    /// Visible entries matching `query` (name, generic name, keywords), case-insensitive.
    pub fn search(&self, query: &str) -> Vec<&DesktopEntry> {
        let q = query.to_lowercase();
        let mut v: Vec<(&DesktopEntry, u8)> = self
            .entries
            .values()
            .filter(|e| !e.no_display)
            .filter_map(|e| {
                let name = e.name.to_lowercase();
                if q.is_empty() {
                    Some((e, 3))
                } else if name.starts_with(&q) {
                    Some((e, 0))
                } else if name.contains(&q) {
                    Some((e, 1))
                } else if e
                    .generic_name
                    .iter()
                    .chain(e.keywords.iter())
                    .any(|k| k.to_lowercase().contains(&q))
                {
                    Some((e, 2))
                } else {
                    None
                }
            })
            .collect();
        v.sort_by(|a, b| {
            a.1.cmp(&b.1)
                .then(a.0.name.to_lowercase().cmp(&b.0.name.to_lowercase()))
        });
        v.into_iter().map(|(e, _)| e).collect()
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RunState {
    Running,
    Exited,
}

#[derive(Debug, Clone, Serialize)]
pub struct Instance {
    pub app: String,
    pub pid: u32,
    pub started: u64,
    pub state: RunState,
    pub exit_code: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("flow-apps-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn w(d: &Path, f: &str, c: &str) {
        let p = d.join(f);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, c).unwrap();
    }

    #[test]
    fn precedence_hidden_and_filters() {
        let a = tmp("a");
        let b = tmp("b");
        w(
            &a,
            "x.desktop",
            "[Desktop Entry]\nType=Application\nName=UserX\nExec=x\n",
        );
        w(
            &b,
            "x.desktop",
            "[Desktop Entry]\nType=Application\nName=SysX\nExec=x\n",
        );
        w(
            &b,
            "y.desktop",
            "[Desktop Entry]\nType=Application\nName=Y\nExec=y\n",
        );
        w(
            &a,
            "y.desktop",
            "[Desktop Entry]\nType=Application\nName=Y\nExec=y\nHidden=true\n",
        );
        w(
            &b,
            "sub/z.desktop",
            "[Desktop Entry]\nType=Application\nName=Zed\nExec=z\nOnlyShowIn=GNOME;\n",
        );
        w(
            &b,
            "n.desktop",
            "[Desktop Entry]\nType=Application\nName=NoDisp\nExec=n\nNoDisplay=true\n",
        );
        let r = Registry::scan(&[a.clone(), b.clone()], None, "FlowDE");
        assert_eq!(r.entries["x.desktop"].name, "UserX");
        assert!(!r.entries.contains_key("y.desktop"));
        assert!(r.entries.contains_key("sub-z.desktop"));
        let names: Vec<_> = r.search("").iter().map(|e| e.name.clone()).collect();
        assert_eq!(names, vec!["UserX"]);
        std::fs::remove_dir_all(a).unwrap();
        std::fs::remove_dir_all(b).unwrap();
    }

    #[test]
    fn search_ranking() {
        let d = tmp("s");
        w(
            &d,
            "a.desktop",
            "[Desktop Entry]\nType=Application\nName=Files\nExec=a\n",
        );
        w(
            &d,
            "b.desktop",
            "[Desktop Entry]\nType=Application\nName=My Files\nExec=b\n",
        );
        w(
            &d,
            "c.desktop",
            "[Desktop Entry]\nType=Application\nName=Nautilus\nKeywords=files;\nExec=c\n",
        );
        let r = Registry::scan(std::slice::from_ref(&d), None, "FlowDE");
        let n: Vec<_> = r.search("file").iter().map(|e| e.name.clone()).collect();
        assert_eq!(n, vec!["Files", "My Files", "Nautilus"]);
        std::fs::remove_dir_all(d).unwrap();
    }
}
