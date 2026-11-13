//! Package backends: apt (read via apt-cache/dpkg-query, privileged steps via
//! pkexec) and Flatpak (CLI). Same model and command lines as the Kiln Store
//! app (kiln-apps/kiln-store, `kiln_store/backends.py`), so
//! the app can later become a thin client of this daemon.
//! PackageKit is a possible later backend (TODO(feature)).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Apt,
    Flatpak,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pkg {
    pub source: Source,
    pub pid: String,
    pub name: String,
    pub description: String,
    pub installed: bool,
    pub version: String,
    /// Flatpak remote.
    pub origin: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Install,
    Remove,
}

/// Valid Debian package name (policy 5.6.1): lowercase alnum start, then `[a-z0-9+.-]`, min 2.
pub fn valid_apt_name(s: &str) -> bool {
    s.len() >= 2
        && s.len() <= 100
        && s.chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '+' | '-' | '.'))
}

/// Flatpak application id: at least two dots, segments of `[A-Za-z0-9_-]`.
pub fn valid_flatpak_id(s: &str) -> bool {
    let segs: Vec<&str> = s.split('.').collect();
    s.len() <= 200
        && segs.len() >= 3
        && segs.iter().all(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
}

pub fn valid_remote(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

pub fn validate(p: &Pkg) -> Result<(), String> {
    let ok = match p.source {
        Source::Apt => valid_apt_name(&p.pid),
        Source::Flatpak => {
            valid_flatpak_id(&p.pid) && (p.origin.is_empty() || valid_remote(&p.origin))
        }
    };
    if ok {
        Ok(())
    } else {
        Err(format!("invalid package identifier {:?}", p.pid))
    }
}

/// argv for an action. Names are validated, so no option injection is possible
/// (and `--` guards apt anyway). Never goes through a shell.
pub fn command_for(a: Action, p: &Pkg) -> Result<Vec<String>, String> {
    validate(p)?;
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    Ok(match (p.source, a) {
        (Source::Apt, Action::Install) => {
            let mut v = s(&[
                "pkexec",
                "env",
                "DEBIAN_FRONTEND=noninteractive",
                "apt-get",
                "-y",
                "install",
                "--",
            ]);
            v.push(p.pid.clone());
            v
        }
        (Source::Apt, Action::Remove) => {
            let mut v = s(&[
                "pkexec",
                "env",
                "DEBIAN_FRONTEND=noninteractive",
                "apt-get",
                "-y",
                "remove",
                "--",
            ]);
            v.push(p.pid.clone());
            v
        }
        (Source::Flatpak, Action::Install) => {
            let mut v = s(&["flatpak", "install", "-y", "--noninteractive", "--system"]);
            v.push(if p.origin.is_empty() {
                "flathub".into()
            } else {
                p.origin.clone()
            });
            v.push(p.pid.clone());
            v
        }
        (Source::Flatpak, Action::Remove) => {
            let mut v = s(&["flatpak", "uninstall", "-y", "--noninteractive"]);
            v.push(p.pid.clone());
            v
        }
    })
}

/// Parse `apt-cache search` lines (`name - description`).
pub fn parse_apt_search(
    out: &str,
    installed: &std::collections::HashSet<String>,
    limit: usize,
) -> Vec<Pkg> {
    out.lines()
        .filter_map(|l| {
            let (name, desc) = l.split_once(" - ")?;
            let name = name.trim();
            valid_apt_name(name).then(|| Pkg {
                source: Source::Apt,
                pid: name.into(),
                name: name.into(),
                description: desc.trim().into(),
                installed: installed.contains(name),
                version: String::new(),
                origin: String::new(),
            })
        })
        .take(limit)
        .collect()
}

/// Parse `flatpak search --columns=application,name,description,version,remotes`.
pub fn parse_flatpak_search(out: &str, installed: &std::collections::HashSet<String>) -> Vec<Pkg> {
    out.lines()
        .filter_map(|l| {
            let c: Vec<&str> = l.split('\t').collect();
            if c.len() < 2 || !valid_flatpak_id(c[0].trim()) {
                return None;
            }
            let id = c[0].trim();
            Some(Pkg {
                source: Source::Flatpak,
                pid: id.into(),
                name: c[1].trim().into(),
                description: c.get(2).unwrap_or(&"").trim().into(),
                installed: installed.contains(id),
                version: c.get(3).unwrap_or(&"").trim().into(),
                origin: c
                    .get(4)
                    .unwrap_or(&"")
                    .split(',')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .into(),
            })
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
pub struct Job {
    pub id: u32,
    pub action: Action,
    pub pkg: Pkg,
    pub state: JobState,
    pub exit_code: Option<i32>,
    pub log_tail: Vec<String>,
}

/// One-at-a-time job queue (apt/dpkg lock makes parallel runs pointless).
#[derive(Debug, Default)]
pub struct Queue {
    next: u32,
    pub jobs: Vec<Job>,
}

const MAX_JOBS_KEPT: usize = 50;

impl Queue {
    pub fn enqueue(&mut self, a: Action, pkg: Pkg) -> Result<u32, String> {
        validate(&pkg)?;
        if self.jobs.iter().any(|j| {
            matches!(j.state, JobState::Queued | JobState::Running)
                && j.pkg.pid == pkg.pid
                && j.pkg.source == pkg.source
        }) {
            return Err("an operation for this package is already queued".into());
        }
        self.next += 1;
        self.jobs.push(Job {
            id: self.next,
            action: a,
            pkg,
            state: JobState::Queued,
            exit_code: None,
            log_tail: vec![],
        });
        let finished = self
            .jobs
            .iter()
            .filter(|j| !matches!(j.state, JobState::Queued | JobState::Running))
            .count();
        if finished > MAX_JOBS_KEPT {
            if let Some(i) = self
                .jobs
                .iter()
                .position(|j| !matches!(j.state, JobState::Queued | JobState::Running))
            {
                self.jobs.remove(i);
            }
        }
        Ok(self.next)
    }

    /// Take the next queued job if none is running.
    pub fn start_next(&mut self) -> Option<&Job> {
        if self.jobs.iter().any(|j| j.state == JobState::Running) {
            return None;
        }
        let j = self.jobs.iter_mut().find(|j| j.state == JobState::Queued)?;
        j.state = JobState::Running;
        Some(j)
    }

    pub fn log(&mut self, id: u32, line: &str) {
        if let Some(j) = self.jobs.iter_mut().find(|j| j.id == id) {
            j.log_tail.push(line.chars().take(300).collect());
            if j.log_tail.len() > 20 {
                j.log_tail.remove(0);
            }
        }
    }

    pub fn finish(&mut self, id: u32, code: Option<i32>) {
        if let Some(j) = self.jobs.iter_mut().find(|j| j.id == id) {
            j.exit_code = code;
            j.state = if code == Some(0) {
                JobState::Done
            } else {
                JobState::Failed
            };
        }
    }

    pub fn cancel(&mut self, id: u32) -> bool {
        match self
            .jobs
            .iter_mut()
            .find(|j| j.id == id && j.state == JobState::Queued)
        {
            Some(j) => {
                j.state = JobState::Cancelled;
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn apt(n: &str) -> Pkg {
        Pkg {
            source: Source::Apt,
            pid: n.into(),
            name: n.into(),
            description: String::new(),
            installed: false,
            version: String::new(),
            origin: String::new(),
        }
    }

    #[test]
    fn name_validation() {
        assert!(valid_apt_name("libfoo-dev"));
        assert!(valid_apt_name("g++"));
        assert!(!valid_apt_name("-rf"));
        assert!(!valid_apt_name("a b"));
        assert!(!valid_apt_name("Foo"));
        assert!(valid_flatpak_id("org.mozilla.firefox"));
        assert!(!valid_flatpak_id("firefox"));
        assert!(!valid_flatpak_id("org..x"));
        assert!(!valid_flatpak_id("org.x;rm"));
    }

    #[test]
    fn commands_match_kiln_store() {
        let c = command_for(Action::Install, &apt("cowsay")).unwrap();
        assert_eq!(
            c.join(" "),
            "pkexec env DEBIAN_FRONTEND=noninteractive apt-get -y install -- cowsay"
        );
        let f = Pkg {
            source: Source::Flatpak,
            pid: "org.gnome.Calculator".into(),
            origin: "flathub".into(),
            ..apt("x")
        };
        assert_eq!(
            command_for(Action::Install, &f).unwrap().join(" "),
            "flatpak install -y --noninteractive --system flathub org.gnome.Calculator"
        );
        assert!(command_for(Action::Remove, &apt("--purge")).is_err());
    }

    #[test]
    fn parsing() {
        let inst: HashSet<String> = ["cowsay".to_string()].into();
        let p = parse_apt_search(
            "cowsay - configurable talking cow\nnot a line\n-bad - x\n",
            &inst,
            10,
        );
        assert_eq!(p.len(), 1);
        assert!(p[0].installed);
        let f = parse_flatpak_search(
            "org.gnome.Calculator\tCalculator\tDo maths\t48.1\tflathub\nbad\n",
            &HashSet::new(),
        );
        assert_eq!(f[0].origin, "flathub");
        assert_eq!(f.len(), 1);
    }

    #[test]
    fn queue_state_machine() {
        let mut q = Queue::default();
        let a = q.enqueue(Action::Install, apt("aa")).unwrap();
        let b = q.enqueue(Action::Install, apt("bb")).unwrap();
        assert!(q.enqueue(Action::Remove, apt("aa")).is_err()); // duplicate
        assert_eq!(q.start_next().unwrap().id, a);
        assert!(q.start_next().is_none()); // one at a time
        assert!(!q.cancel(a)); // running cannot be cancelled here
        q.log(a, "working");
        q.finish(a, Some(0));
        assert_eq!(q.jobs[0].state, JobState::Done);
        assert!(q.cancel(b));
        assert!(q.start_next().is_none());
        assert!(q.enqueue(Action::Install, apt("aa")).is_ok()); // finished jobs do not block
    }

    #[test]
    fn failure_state() {
        let mut q = Queue::default();
        let a = q.enqueue(Action::Remove, apt("zz")).unwrap();
        q.start_next();
        q.finish(a, Some(100));
        assert_eq!(q.jobs[0].state, JobState::Failed);
    }
}
