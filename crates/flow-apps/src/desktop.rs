//! Minimal, strict `.desktop` parser (Desktop Entry Specification subset).

use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct DesktopEntry {
    /// Desktop file id, e.g. `org.gnome.Calculator`.
    pub id: String,
    pub name: String,
    pub generic_name: Option<String>,
    pub comment: Option<String>,
    pub exec: String,
    pub icon: Option<String>,
    pub categories: Vec<String>,
    pub keywords: Vec<String>,
    pub terminal: bool,
    pub no_display: bool,
    pub hidden: bool,
    pub only_show_in: Vec<String>,
    pub not_show_in: Vec<String>,
    pub try_exec: Option<String>,
    /// Working directory (`Path=` key).
    pub work_dir: Option<String>,
    /// Absolute path of the .desktop file (filled by the registry).
    pub path: Option<String>,
}

/// Unescape `\s \n \t \r \\` and `\;` (list separator handled by split).
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some(';') => out.push(';'),
            Some(o) => {
                out.push('\\');
                out.push(o)
            }
            None => out.push('\\'),
        }
    }
    out
}

fn split_list(s: &str) -> Vec<String> {
    let mut items = vec![];
    let mut cur = String::new();
    let mut esc = false;
    for c in s.chars() {
        if esc {
            cur.push(c);
            esc = false;
        } else if c == '\\' {
            esc = true;
            cur.push(c);
        } else if c == ';' {
            if !cur.is_empty() {
                items.push(unescape(&cur));
            }
            cur.clear();
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        items.push(unescape(&cur));
    }
    items
}

/// Parse; `locale` like `de_DE` picks `Name[de_DE]` > `Name[de]` > `Name`.
/// Returns None if there is no `[Desktop Entry]` group, or `Type` is not Application.
pub fn parse(id: &str, text: &str, locale: Option<&str>) -> Option<DesktopEntry> {
    let mut in_entry = false;
    let mut kv: HashMap<String, String> = HashMap::new();
    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            kv.entry(k.trim().to_string())
                .or_insert_with(|| v.trim_start().to_string());
        }
    }
    if kv.get("Type").map(String::as_str) != Some("Application") {
        return None;
    }
    let loc = |key: &str| -> Option<String> {
        if let Some(l) = locale {
            let l = l.split('.').next().unwrap_or(l);
            if let Some(v) = kv.get(&format!("{key}[{l}]")) {
                return Some(unescape(v));
            }
            if let Some((lang, _)) = l.split_once('_') {
                if let Some(v) = kv.get(&format!("{key}[{lang}]")) {
                    return Some(unescape(v));
                }
            }
        }
        kv.get(key).map(|v| unescape(v))
    };
    let b = |k: &str| kv.get(k).is_some_and(|v| v == "true");
    let list = |k: &str| kv.get(k).map(|v| split_list(v)).unwrap_or_default();
    Some(DesktopEntry {
        id: id.to_string(),
        name: loc("Name")?,
        generic_name: loc("GenericName"),
        comment: loc("Comment"),
        // Exec keeps raw escapes: quoting is resolved by exec::tokenize.
        exec: kv.get("Exec")?.clone(),
        icon: kv.get("Icon").cloned(),
        categories: list("Categories"),
        keywords: kv
            .get("Keywords")
            .map(|v| split_list(v))
            .unwrap_or_default(),
        terminal: b("Terminal"),
        no_display: b("NoDisplay"),
        hidden: b("Hidden"),
        only_show_in: list("OnlyShowIn"),
        not_show_in: list("NotShowIn"),
        try_exec: kv.get("TryExec").cloned(),
        work_dir: kv.get("Path").cloned(),
        path: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# c\n[Desktop Entry]\nType=Application\nName=Calc\nName[de]=Rechner\nName[de_AT]=Rechna\nExec=calc %U\nIcon=calc\nCategories=Utility;Math;\nKeywords=a\\;b;c;\nTerminal=false\n\n[Desktop Action x]\nName=Other\nExec=evil\n";

    #[test]
    fn basic() {
        let e = parse("calc.desktop", SAMPLE, None).unwrap();
        assert_eq!(e.name, "Calc");
        assert_eq!(e.exec, "calc %U");
        assert_eq!(e.categories, vec!["Utility", "Math"]);
        assert_eq!(e.keywords, vec!["a;b", "c"]);
    }

    #[test]
    fn locale_fallback() {
        assert_eq!(
            parse("c", SAMPLE, Some("de_AT.UTF-8")).unwrap().name,
            "Rechna"
        );
        assert_eq!(parse("c", SAMPLE, Some("de_DE")).unwrap().name, "Rechner");
        assert_eq!(parse("c", SAMPLE, Some("fr")).unwrap().name, "Calc");
    }

    #[test]
    fn rejects_non_apps_and_missing_exec() {
        assert!(parse("x", "[Desktop Entry]\nType=Link\nName=a\nURL=x", None).is_none());
        assert!(parse("x", "[Desktop Entry]\nType=Application\nName=a", None).is_none());
        assert!(parse("x", "garbage", None).is_none());
    }

    #[test]
    fn action_group_ignored() {
        let e = parse("c", SAMPLE, None).unwrap();
        assert_ne!(e.exec, "evil");
    }
}
