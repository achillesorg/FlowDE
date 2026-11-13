//! Validation and argv construction for yt-dlp. Only an allowlist of options
//! is ever passed; user text never reaches an option position.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    /// Preset: best | audio | 720p | 1080p | custom (use `format`).
    pub preset: String,
    pub format: Option<String>,
    pub playlist: bool,
    pub subtitles: Vec<String>,
    pub thumbnail: bool,
    pub metadata: bool,
    /// Cookie file name inside `<config>/flowde/cookies/` (no path separators).
    pub cookies: Option<String>,
    /// Absolute output directory.
    pub out_dir: PathBuf,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            preset: "best".into(),
            format: None,
            playlist: false,
            subtitles: vec![],
            thumbnail: false,
            metadata: true,
            cookies: None,
            out_dir: PathBuf::from("/tmp"),
        }
    }
}

/// http(s) only, no whitespace/control characters, bounded length, no leading dash.
pub fn validate_url(u: &str) -> Result<(), String> {
    if u.len() > 2000 || u.starts_with('-') {
        return Err("bad url".into());
    }
    let rest = u
        .strip_prefix("https://")
        .or_else(|| u.strip_prefix("http://"))
        .ok_or("only http(s) urls are accepted")?;
    if rest.is_empty() || rest.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("bad url".into());
    }
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    if host.is_empty() || host.contains('@') {
        return Err("url must have a host and no userinfo".into());
    }
    Ok(())
}

pub fn validate_format(f: &str) -> Result<(), String> {
    let ok = !f.is_empty()
        && f.len() <= 120
        && !f.starts_with('-')
        && f.chars()
            .all(|c| c.is_ascii_alphanumeric() || "[]()+*/,.=<>!&_-~^|".contains(c));
    if ok {
        Ok(())
    } else {
        Err("invalid format selector".into())
    }
}

pub fn validate_lang(l: &str) -> bool {
    !l.is_empty()
        && l.len() <= 20
        && l.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '*')
}

fn preset_format(p: &str) -> Option<&'static str> {
    match p {
        "best" => Some("bv*+ba/b"),
        "audio" => Some("ba/b"),
        "720p" => Some("bv*[height<=720]+ba/b[height<=720]"),
        "1080p" => Some("bv*[height<=1080]+ba/b[height<=1080]"),
        _ => None,
    }
}

/// Progress template; fields are `NA` when unknown.
pub const PROGRESS_TEMPLATE: &str =
    "download:FLOWDLP|%(progress.status)s|%(progress.downloaded_bytes)s|%(progress.total_bytes)s|%(progress.total_bytes_estimate)s|%(progress.speed)s|%(progress.eta)s";

/// Build yt-dlp argv (without the program name). `cookies_dir` is where
/// cookie files live.
pub fn build_args(url: &str, o: &Options, cookies_dir: &Path) -> Result<Vec<String>, String> {
    validate_url(url)?;
    if !o.out_dir.is_absolute()
        || o.out_dir
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("out_dir must be absolute without '..'".into());
    }
    let mut a: Vec<String> = [
        "--ignore-config",
        "--newline",
        "--no-colors",
        "--no-warnings",
        "--continue",
        "--part",
        "--no-exec",
    ]
    .map(String::from)
    .to_vec();
    a.push("--progress-template".into());
    a.push(PROGRESS_TEMPLATE.into());
    let fmt = match (&o.format, preset_format(&o.preset)) {
        (Some(f), _) => {
            validate_format(f)?;
            f.clone()
        }
        (None, Some(p)) => p.to_string(),
        (None, None) => return Err(format!("unknown preset {:?}", o.preset)),
    };
    a.push("-f".into());
    a.push(fmt);
    if o.preset == "audio" && o.format.is_none() {
        a.extend(["-x", "--audio-format", "best"].map(String::from));
    }
    a.push(
        if o.playlist {
            "--yes-playlist"
        } else {
            "--no-playlist"
        }
        .into(),
    );
    if !o.subtitles.is_empty() {
        if !o.subtitles.iter().all(|l| validate_lang(l)) {
            return Err("invalid subtitle language".into());
        }
        a.extend([
            "--write-subs".to_string(),
            "--sub-langs".to_string(),
            o.subtitles.join(","),
        ]);
    }
    if o.thumbnail {
        a.extend(["--write-thumbnail", "--embed-thumbnail"].map(String::from));
    }
    if o.metadata {
        a.push("--embed-metadata".into());
    }
    if let Some(c) = &o.cookies {
        if c.is_empty()
            || c.contains('/')
            || c.contains('\\')
            || c.starts_with('.')
            || c.len() > 100
        {
            return Err("invalid cookie file name".into());
        }
        a.push("--cookies".into());
        a.push(cookies_dir.join(c).to_string_lossy().into_owned());
    }
    // `%` is yt-dlp's template escape: double it inside the directory part.
    let dir = o.out_dir.to_string_lossy().replace('%', "%%");
    a.push("-o".into());
    a.push(format!("{dir}/%(title).200B [%(id)s].%(ext)s"));
    a.push("--".into());
    a.push(url.to_string());
    Ok(a)
}

/// Wrap a command in bubblewrap: read-only system, private /tmp and $HOME,
/// only `out_dir` (and cookies dir, read-only) visible, network shared.
pub fn bwrap_wrap(
    program: &str,
    args: &[String],
    out_dir: &Path,
    cookies_dir: Option<&Path>,
) -> Vec<String> {
    let mut v: Vec<String> = [
        "bwrap",
        "--unshare-all",
        "--share-net",
        "--die-with-parent",
        "--new-session",
        "--proc",
        "/proc",
        "--dev",
        "/dev",
        "--tmpfs",
        "/tmp",
    ]
    .map(String::from)
    .to_vec();
    for d in [
        "/usr",
        "/etc/ssl",
        "/etc/resolv.conf",
        "/etc/hosts",
        "/etc/nsswitch.conf",
        "/etc/ca-certificates",
    ] {
        v.extend(["--ro-bind-try".to_string(), d.to_string(), d.to_string()]);
    }
    for (l, t) in [
        ("/lib", "usr/lib"),
        ("/lib64", "usr/lib64"),
        ("/bin", "usr/bin"),
    ] {
        v.extend(["--symlink".to_string(), t.to_string(), l.to_string()]);
    }
    let o = out_dir.to_string_lossy().into_owned();
    v.extend(["--bind".to_string(), o.clone(), o]);
    if let Some(c) = cookies_dir {
        let c = c.to_string_lossy().into_owned();
        v.extend(["--ro-bind-try".to_string(), c.clone(), c]);
    }
    v.push("--".into());
    v.push(program.into());
    v.extend(args.iter().cloned());
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn o() -> Options {
        Options {
            out_dir: PathBuf::from("/home/u/Downloads"),
            ..Default::default()
        }
    }

    #[test]
    fn url_rules() {
        assert!(validate_url("https://example.com/watch?v=1").is_ok());
        assert!(validate_url("http://example.com").is_ok());
        for bad in [
            "file:///etc/passwd",
            "-o evil",
            "https://",
            "https://u:p@host/x",
            "https://a b",
            "ftp://x/y",
            "https://x/\n--exec",
        ] {
            assert!(validate_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn args_are_allowlisted_and_terminated() {
        let a = build_args("https://example.com/v", &o(), Path::new("/c")).unwrap();
        assert!(a.contains(&"--no-exec".to_string()) && a.contains(&"--ignore-config".to_string()));
        let n = a.len();
        assert_eq!(a[n - 2], "--");
        assert_eq!(a[n - 1], "https://example.com/v");
        assert!(a
            .iter()
            .all(|x| x != "--exec" && x != "--cookies-from-browser"));
        assert!(a.contains(&"--no-playlist".to_string()));
    }

    #[test]
    fn presets_and_custom_format() {
        let mut op = o();
        op.preset = "audio".into();
        let a = build_args("https://e.com/x", &op, Path::new("/c")).unwrap();
        assert!(a.contains(&"-x".to_string()));
        op.preset = "weird".into();
        assert!(build_args("https://e.com/x", &op, Path::new("/c")).is_err());
        op.format = Some("bv*[height<=480]+ba".into());
        assert!(build_args("https://e.com/x", &op, Path::new("/c")).is_ok());
        op.format = Some("b; rm -rf".into());
        assert!(build_args("https://e.com/x", &op, Path::new("/c")).is_err());
        op.format = Some("--exec".into());
        assert!(build_args("https://e.com/x", &op, Path::new("/c")).is_err());
    }

    #[test]
    fn cookies_subs_outdir() {
        let mut op = o();
        op.cookies = Some("site.txt".into());
        op.subtitles = vec!["en".into(), "pt-BR".into()];
        let a = build_args("https://e.com/x", &op, Path::new("/c")).unwrap();
        let i = a.iter().position(|x| x == "--cookies").unwrap();
        assert_eq!(a[i + 1], "/c/site.txt");
        assert!(a.contains(&"en,pt-BR".to_string()));
        op.cookies = Some("../x".into());
        assert!(build_args("https://e.com/x", &op, Path::new("/c")).is_err());
        op.cookies = None;
        op.subtitles = vec!["en;x".into()];
        assert!(build_args("https://e.com/x", &op, Path::new("/c")).is_err());
        op.subtitles = vec![];
        op.out_dir = PathBuf::from("relative");
        assert!(build_args("https://e.com/x", &op, Path::new("/c")).is_err());
        op.out_dir = PathBuf::from("/a/../etc");
        assert!(build_args("https://e.com/x", &op, Path::new("/c")).is_err());
        op.out_dir = PathBuf::from("/a/100%");
        let a = build_args("https://e.com/x", &op, Path::new("/c")).unwrap();
        assert!(a.iter().any(|x| x.starts_with("/a/100%%/%(title)")));
    }

    #[test]
    fn bwrap_shape() {
        let v = bwrap_wrap(
            "yt-dlp",
            &["--x".into()],
            Path::new("/out"),
            Some(Path::new("/c")),
        );
        assert_eq!(v[0], "bwrap");
        assert!(v.contains(&"--unshare-all".to_string()) && v.contains(&"--share-net".to_string()));
        let i = v.iter().position(|x| x == "--").unwrap();
        assert_eq!(&v[i + 1..], ["yt-dlp", "--x"]);
        assert!(v.windows(3).any(|w| w == ["--bind", "/out", "/out"]));
    }
}
