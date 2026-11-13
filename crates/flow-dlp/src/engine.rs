//! Engine (extractor + downloader) abstraction, the yt-dlp engine and a mock.

use crate::args::{build_args, bwrap_wrap, Options};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MediaInfo {
    pub id: String,
    pub title: String,
    pub uploader: String,
    pub duration_s: Option<f64>,
    pub is_playlist: bool,
    pub entries: usize,
    pub formats: usize,
    pub extractor: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Progress {
    pub status: String,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub speed_bps: Option<f64>,
    pub eta_s: Option<u64>,
}

impl Progress {
    pub fn fraction(&self) -> Option<f64> {
        self.total
            .filter(|t| *t > 0)
            .map(|t| (self.downloaded as f64 / t as f64).min(1.0))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum EngineError {
    NotInstalled(String),
    Cancelled,
    /// Non-zero exit; `retryable` guesses transient network failures.
    Failed {
        code: Option<i32>,
        message: String,
        retryable: bool,
    },
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineError::NotInstalled(p) => write!(f, "{p} is not installed"),
            EngineError::Cancelled => write!(f, "cancelled"),
            EngineError::Failed { message, .. } => write!(f, "{message}"),
        }
    }
}

/// An extractor/downloader. A future native Rust extractor implements this
/// same trait and is registered before the yt-dlp fallback.
pub trait Engine: Send + Sync {
    fn name(&self) -> &str;
    fn supports(&self, url: &str) -> bool;
    fn available(&self) -> bool;
    fn probe(&self, url: &str) -> Result<MediaInfo, EngineError>;
    /// Blocking download; calls `on_progress`; must stop promptly when `cancel` is set.
    fn download(
        &self,
        url: &str,
        o: &Options,
        cancel: &AtomicBool,
        on_progress: &mut dyn FnMut(Progress),
    ) -> Result<(), EngineError>;
}

/// Parse one `--progress-template` line (`FLOWDLP|status|dl|total|est|speed|eta`).
pub fn parse_progress(line: &str) -> Option<Progress> {
    let rest = line.trim().strip_prefix("FLOWDLP|")?;
    let f: Vec<&str> = rest.split('|').collect();
    if f.len() < 6 {
        return None;
    }
    let num = |s: &str| s.trim().parse::<f64>().ok();
    let total = num(f[2]).or_else(|| num(f[3])).map(|v| v as u64);
    Some(Progress {
        status: f[0].to_string(),
        downloaded: num(f[1]).unwrap_or(0.0) as u64,
        total,
        speed_bps: num(f[4]),
        eta_s: num(f[5]).map(|v| v as u64),
    })
}

/// Parse `--dump-json` output (one JSON document per line; playlists with
/// `--flat-playlist` give one line per entry).
pub fn parse_dump_json(out: &str) -> Option<MediaInfo> {
    let docs: Vec<serde_json::Value> = out
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    let first = docs.first()?;
    let s = |v: &serde_json::Value, k: &str| v[k].as_str().unwrap_or("").to_string();
    let playlist = docs.len() > 1 || first["_type"] == "playlist";
    Some(MediaInfo {
        id: s(first, "id"),
        title: if playlist && docs.len() > 1 {
            s(first, "playlist_title")
        } else {
            s(first, "title")
        },
        uploader: s(first, "uploader"),
        duration_s: first["duration"].as_f64(),
        is_playlist: playlist,
        entries: if playlist { docs.len() } else { 1 },
        formats: first["formats"].as_array().map_or(0, |a| a.len()),
        extractor: s(first, "extractor_key"),
    })
}

fn retryable(stderr: &str) -> bool {
    let l = stderr.to_lowercase();
    [
        "timed out",
        "connection reset",
        "temporary failure",
        "http error 5",
        "unable to download",
        "network is unreachable",
    ]
    .iter()
    .any(|p| l.contains(p))
}

pub struct YtDlpEngine {
    pub program: String,
    pub sandbox: bool,
    pub cookies_dir: PathBuf,
}

impl YtDlpEngine {
    pub fn new(cookies_dir: PathBuf, sandbox: bool) -> Self {
        Self {
            program: "yt-dlp".into(),
            sandbox,
            cookies_dir,
        }
    }

    fn find(&self) -> Option<PathBuf> {
        std::env::var_os("PATH").and_then(|p| {
            std::env::split_paths(&p)
                .map(|d| d.join(&self.program))
                .find(|c| c.is_file())
        })
    }

    fn command(&self, args: Vec<String>, o: Option<&Options>) -> Command {
        let argv = match o {
            Some(o) if self.sandbox => bwrap_wrap(
                &self.program,
                &args,
                &o.out_dir,
                o.cookies.as_ref().map(|_| self.cookies_dir.as_path()),
            ),
            _ => std::iter::once(self.program.clone()).chain(args).collect(),
        };
        let mut c = Command::new(&argv[0]);
        c.args(&argv[1..]).stdin(Stdio::null());
        c
    }
}

impl Engine for YtDlpEngine {
    fn name(&self) -> &str {
        "yt-dlp"
    }
    fn supports(&self, url: &str) -> bool {
        crate::args::validate_url(url).is_ok()
    }
    fn available(&self) -> bool {
        self.find().is_some()
    }

    fn probe(&self, url: &str) -> Result<MediaInfo, EngineError> {
        crate::args::validate_url(url).map_err(|m| EngineError::Failed {
            code: None,
            message: m,
            retryable: false,
        })?;
        if !self.available() {
            return Err(EngineError::NotInstalled(self.program.clone()));
        }
        let args: Vec<String> = [
            "--ignore-config",
            "--dump-json",
            "--flat-playlist",
            "--skip-download",
            "--no-warnings",
            "--no-exec",
            "--playlist-end",
            "50",
            "--",
            url,
        ]
        .map(String::from)
        .to_vec();
        let out = self
            .command(args, None)
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| EngineError::Failed {
                code: None,
                message: e.to_string(),
                retryable: false,
            })?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr).into_owned();
            return Err(EngineError::Failed {
                code: out.status.code(),
                retryable: retryable(&err),
                message: err.lines().last().unwrap_or("probe failed").to_string(),
            });
        }
        parse_dump_json(&String::from_utf8_lossy(&out.stdout)).ok_or(EngineError::Failed {
            code: None,
            message: "unparseable yt-dlp output".into(),
            retryable: false,
        })
    }

    fn download(
        &self,
        url: &str,
        o: &Options,
        cancel: &AtomicBool,
        on_progress: &mut dyn FnMut(Progress),
    ) -> Result<(), EngineError> {
        if !self.available() {
            return Err(EngineError::NotInstalled(self.program.clone()));
        }
        let args = build_args(url, o, &self.cookies_dir).map_err(|m| EngineError::Failed {
            code: None,
            message: m,
            retryable: false,
        })?;
        let mut child = self
            .command(args, Some(o))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| EngineError::Failed {
                code: None,
                message: e.to_string(),
                retryable: false,
            })?;
        let stdout = child.stdout.take().expect("piped");
        let stderr = child.stderr.take().expect("piped");
        let child = Arc::new(Mutex::new(child));
        // Watcher: kills the child promptly when cancelled even if it prints nothing.
        let done = Arc::new(AtomicBool::new(false));
        let (c2, d2) = (child.clone(), done.clone());
        let err_lines = Arc::new(Mutex::new(Vec::<String>::new()));
        let e2 = err_lines.clone();
        std::thread::scope(|s| {
            s.spawn(|| {
                while !d2.load(Ordering::Relaxed) {
                    if cancel.load(Ordering::Relaxed) {
                        let _ = c2.lock().unwrap().kill();
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(200));
                }
            });
            s.spawn(move || {
                for l in BufReader::new(stderr).lines().map_while(Result::ok) {
                    let mut v = e2.lock().unwrap();
                    v.push(l);
                    if v.len() > 20 {
                        v.remove(0);
                    }
                }
            });
            for l in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Some(p) = parse_progress(&l) {
                    on_progress(p);
                }
            }
            done.store(true, Ordering::Relaxed);
        });
        let status = child
            .lock()
            .unwrap()
            .wait()
            .map_err(|e| EngineError::Failed {
                code: None,
                message: e.to_string(),
                retryable: false,
            })?;
        if cancel.load(Ordering::Relaxed) {
            return Err(EngineError::Cancelled);
        }
        if status.success() {
            return Ok(());
        }
        let err = err_lines.lock().unwrap().join("\n");
        Err(EngineError::Failed {
            code: status.code(),
            retryable: retryable(&err),
            message: err.lines().last().unwrap_or("download failed").to_string(),
        })
    }
}

/// Scripted engine for tests: never touches the network or the disk.
pub struct MockEngine {
    pub steps: Vec<Progress>,
    pub fail_with: Option<EngineError>,
    pub info: MediaInfo,
}

impl Engine for MockEngine {
    fn name(&self) -> &str {
        "mock"
    }
    fn supports(&self, url: &str) -> bool {
        url.starts_with("https://mock.invalid/")
    }
    fn available(&self) -> bool {
        true
    }
    fn probe(&self, _url: &str) -> Result<MediaInfo, EngineError> {
        Ok(self.info.clone())
    }
    fn download(
        &self,
        _url: &str,
        _o: &Options,
        cancel: &AtomicBool,
        on_progress: &mut dyn FnMut(Progress),
    ) -> Result<(), EngineError> {
        for p in &self.steps {
            if cancel.load(Ordering::Relaxed) {
                return Err(EngineError::Cancelled);
            }
            on_progress(p.clone());
        }
        match &self.fail_with {
            Some(e) => Err(e.clone()),
            None => Ok(()),
        }
    }
}

/// Engines in priority order; first that supports the URL wins.
#[derive(Default)]
pub struct Engines(pub Vec<Box<dyn Engine>>);

impl Engines {
    pub fn pick(&self, url: &str) -> Option<&dyn Engine> {
        self.0
            .iter()
            .find(|e| e.supports(url) && e.available())
            .map(|b| b.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_lines() {
        let p = parse_progress("FLOWDLP|downloading|500|1000|NA|2048.5|12").unwrap();
        assert_eq!(
            (p.downloaded, p.total, p.eta_s),
            (500, Some(1000), Some(12))
        );
        assert_eq!(p.fraction(), Some(0.5));
        let q = parse_progress("FLOWDLP|downloading|10|NA|40|NA|NA").unwrap();
        assert_eq!(q.total, Some(40));
        assert!(q.speed_bps.is_none());
        assert!(parse_progress("[download] 50%").is_none());
        assert!(parse_progress("FLOWDLP|x|1").is_none());
    }

    #[test]
    fn dump_json_single_and_playlist() {
        let one = r#"{"id":"abc","title":"T","uploader":"U","duration":61.5,"formats":[{},{}],"extractor_key":"Youtube"}"#;
        let m = parse_dump_json(one).unwrap();
        assert_eq!(
            (m.title.as_str(), m.formats, m.is_playlist, m.entries),
            ("T", 2, false, 1)
        );
        let pl = format!("{one}\n{one}\n{one}");
        let m = parse_dump_json(&pl).unwrap();
        assert!(m.is_playlist && m.entries == 3);
        assert!(parse_dump_json("not json").is_none());
    }

    #[test]
    fn engine_selection_and_mock() {
        let e = Engines(vec![
            Box::new(MockEngine {
                steps: vec![],
                fail_with: None,
                info: MediaInfo::default(),
            }),
            Box::new(YtDlpEngine {
                program: "definitely-not-installed-xyz".into(),
                sandbox: false,
                cookies_dir: "/c".into(),
            }),
        ]);
        assert_eq!(e.pick("https://mock.invalid/x").unwrap().name(), "mock");
        assert!(e.pick("https://example.com/x").is_none()); // yt-dlp missing => no engine
    }

    #[test]
    fn missing_binary_reported_without_spawning() {
        let y = YtDlpEngine {
            program: "definitely-not-installed-xyz".into(),
            sandbox: false,
            cookies_dir: "/c".into(),
        };
        assert!(!y.available());
        assert_eq!(
            y.probe("https://example.com/x"),
            Err(EngineError::NotInstalled(
                "definitely-not-installed-xyz".into()
            ))
        );
        let cancel = AtomicBool::new(false);
        let r = y.download(
            "https://example.com/x",
            &Options::default(),
            &cancel,
            &mut |_| {},
        );
        assert!(matches!(r, Err(EngineError::NotInstalled(_))));
    }

    #[test]
    fn retry_heuristic() {
        assert!(retryable(
            "ERROR: Unable to download webpage: <urlopen error timed out>"
        ));
        assert!(!retryable("ERROR: Video unavailable"));
    }
}
