//! KilnDM logic: session catalogue, autologin policy, greeter state machine.
//!
//! SECURITY: a display manager is the one component that must run privileged
//! (PAM authentication and starting a session as another user). This crate
//! holds only unprivileged, testable logic. The PAM backend and session
//! launcher are TODO(feature) behind the [`Auth`] trait; until they exist the
//! daemon refuses logins with `unavailable` rather than pretending.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Session {
    /// File stem, e.g. `flowde`, `fede`.
    pub id: String,
    pub name: String,
    pub exec: String,
    pub kind: &'static str, // "wayland" | "x11"
    pub desktop_names: Vec<String>,
}

fn parse_session(id: &str, kind: &'static str, text: &str) -> Option<Session> {
    let mut in_entry = false;
    let mut kv = BTreeMap::new();
    for l in text.lines().map(str::trim) {
        if l.starts_with('[') {
            in_entry = l == "[Desktop Entry]";
        } else if in_entry {
            if let Some((k, v)) = l.split_once('=') {
                kv.entry(k.to_string()).or_insert_with(|| v.to_string());
            }
        }
    }
    if kv.get("Hidden").is_some_and(|v| v == "true")
        || kv.get("NoDisplay").is_some_and(|v| v == "true")
    {
        return None;
    }
    Some(Session {
        id: id.to_string(),
        name: kv.get("Name")?.clone(),
        exec: kv.get("Exec")?.clone(),
        kind,
        desktop_names: kv
            .get("DesktopNames")
            .map(|d| {
                d.split(';')
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default(),
    })
}

/// Scan `wayland-sessions` and `xsessions` under the given data dirs.
pub fn scan_sessions(data_dirs: &[&Path]) -> Vec<Session> {
    let mut out: Vec<Session> = vec![];
    for d in data_dirs {
        for (sub, kind) in [("wayland-sessions", "wayland"), ("xsessions", "x11")] {
            let Ok(rd) = std::fs::read_dir(d.join(sub)) else {
                continue;
            };
            let mut files: Vec<_> = rd.flatten().collect();
            files.sort_by_key(|e| e.file_name());
            for e in files {
                let p = e.path();
                if p.extension().is_none_or(|x| x != "desktop") {
                    continue;
                }
                let id = p
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                if out.iter().any(|s| s.id == id) || !valid_session_id(&id) {
                    continue;
                }
                if let Some(s) = std::fs::read_to_string(&p)
                    .ok()
                    .and_then(|t| parse_session(&id, kind, &t))
                {
                    out.push(s);
                }
            }
        }
    }
    out
}

pub fn valid_session_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// POSIX-ish login name.
pub fn valid_user(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b.len() <= 32
        && (b[0].is_ascii_lowercase() || b[0] == b'_')
        && b.iter().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'_' | b'-' | b'.')
        })
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub default_session: Option<String>,
    pub autologin_user: Option<String>,
    pub autologin_session: Option<String>,
    /// user -> last session id.
    pub last_session: BTreeMap<String, String>,
}

impl Config {
    pub fn validate(&self) -> Result<(), String> {
        for s in [&self.default_session, &self.autologin_session]
            .into_iter()
            .flatten()
        {
            if !valid_session_id(s) {
                return Err(format!("bad session id {s:?}"));
            }
        }
        if let Some(u) = &self.autologin_user {
            if !valid_user(u) {
                return Err(format!("bad user {u:?}"));
            }
            if u == "root" {
                return Err("autologin as root is refused".into());
            }
        }
        Ok(())
    }

    /// Session to preselect for `user`: last used > default > first available.
    pub fn preselect<'a>(&self, user: &str, avail: &'a [Session]) -> Option<&'a Session> {
        [self.last_session.get(user), self.default_session.as_ref()]
            .into_iter()
            .flatten()
            .find_map(|id| avail.iter().find(|s| &s.id == id))
            .or_else(|| avail.first())
    }

    /// Autologin target, only if the session exists.
    pub fn autologin<'a>(&self, avail: &'a [Session]) -> Option<(&str, &'a Session)> {
        let u = self.autologin_user.as_deref()?;
        let id = self
            .autologin_session
            .as_ref()
            .or(self.default_session.as_ref())?;
        avail.iter().find(|s| &s.id == id).map(|s| (u, s))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Greeter {
    Idle,
    Authenticating { user: String },
    Starting { user: String, session: String },
    Running { user: String, session: String },
}

#[derive(Debug, PartialEq, Eq)]
pub enum AuthError {
    Failed,
    Unavailable(String),
}

/// Privileged authentication backend (PAM in production).
pub trait Auth {
    fn authenticate(&mut self, user: &str, secret: &str) -> Result<(), AuthError>;
}

/// Default until PAM is implemented: always unavailable.
pub struct NoAuth;
impl Auth for NoAuth {
    fn authenticate(&mut self, _u: &str, _s: &str) -> Result<(), AuthError> {
        Err(AuthError::Unavailable(
            "PAM backend not implemented (TODO(feature))".into(),
        ))
    }
}

pub const MAX_FAILS: u32 = 5;
pub const FAIL_DELAY_S: u64 = 3;

pub struct Machine {
    pub state: Greeter,
    fails: u32,
    blocked_until: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LoginError {
    Busy,
    BadInput(String),
    UnknownSession,
    Denied,
    Throttled(u64),
    Unavailable(String),
}

impl Default for Machine {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine {
    pub fn new() -> Self {
        Self {
            state: Greeter::Idle,
            fails: 0,
            blocked_until: 0,
        }
    }

    /// Authenticate and move to Starting. The caller then launches the session
    /// and reports back with `session_started` / `session_ended`.
    pub fn login(
        &mut self,
        auth: &mut dyn Auth,
        avail: &[Session],
        user: &str,
        secret: &str,
        session: &str,
        now: u64,
    ) -> Result<(), LoginError> {
        if self.state != Greeter::Idle {
            return Err(LoginError::Busy);
        }
        if !valid_user(user) || user == "root" {
            return Err(LoginError::BadInput("invalid user".into()));
        }
        if !avail.iter().any(|s| s.id == session) {
            return Err(LoginError::UnknownSession);
        }
        if now < self.blocked_until {
            return Err(LoginError::Throttled(self.blocked_until - now));
        }
        match auth.authenticate(user, secret) {
            Ok(()) => {
                self.fails = 0;
                self.state = Greeter::Starting {
                    user: user.into(),
                    session: session.into(),
                };
                Ok(())
            }
            Err(AuthError::Unavailable(m)) => Err(LoginError::Unavailable(m)),
            Err(AuthError::Failed) => {
                self.fails += 1;
                self.blocked_until = now + FAIL_DELAY_S * u64::from(self.fails.min(MAX_FAILS));
                Err(LoginError::Denied)
            }
        }
    }

    pub fn session_started(&mut self) {
        if let Greeter::Starting { user, session } = self.state.clone() {
            self.state = Greeter::Running { user, session };
        }
    }

    pub fn session_ended(&mut self) {
        self.state = Greeter::Idle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sess(id: &str) -> Session {
        Session {
            id: id.into(),
            name: id.into(),
            exec: id.into(),
            kind: "wayland",
            desktop_names: vec![],
        }
    }

    struct Fixed(&'static str);
    impl Auth for Fixed {
        fn authenticate(&mut self, _u: &str, s: &str) -> Result<(), AuthError> {
            if s == self.0 {
                Ok(())
            } else {
                Err(AuthError::Failed)
            }
        }
    }

    #[test]
    fn scan_and_precedence() {
        let d = std::env::temp_dir().join(format!("flow-dm-{}", std::process::id()));
        let w = d.join("wayland-sessions");
        std::fs::create_dir_all(&w).unwrap();
        std::fs::write(
            w.join("flowde.desktop"),
            "[Desktop Entry]\nName=FlowDE\nExec=flowde-session\nDesktopNames=FlowDE;\n",
        )
        .unwrap();
        std::fs::write(
            w.join("hidden.desktop"),
            "[Desktop Entry]\nName=H\nExec=h\nHidden=true\n",
        )
        .unwrap();
        std::fs::write(
            w.join("bad name.desktop"),
            "[Desktop Entry]\nName=B\nExec=b\n",
        )
        .unwrap();
        std::fs::write(
            w.join("fede.desktop"),
            "[Desktop Entry]\nName=FeDe\nExec=fede-session\n",
        )
        .unwrap();
        let s = scan_sessions(&[&d]);
        let ids: Vec<_> = s.iter().map(|x| x.id.as_str()).collect();
        assert_eq!(ids, vec!["fede", "flowde"]);
        assert_eq!(s[1].desktop_names, vec!["FlowDE"]);
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn config_rules() {
        let avail = vec![sess("fede"), sess("flowde")];
        let mut c = Config {
            default_session: Some("flowde".into()),
            ..Default::default()
        };
        assert_eq!(c.preselect("u", &avail).unwrap().id, "flowde");
        c.last_session.insert("u".into(), "fede".into());
        assert_eq!(c.preselect("u", &avail).unwrap().id, "fede");
        assert_eq!(c.preselect("other", &avail).unwrap().id, "flowde");
        c.autologin_user = Some("root".into());
        assert!(c.validate().is_err());
        c.autologin_user = Some("alice".into());
        c.autologin_session = Some("flowde".into());
        assert!(c.validate().is_ok());
        assert_eq!(c.autologin(&avail).unwrap().0, "alice");
        c.autologin_session = Some("gone".into());
        assert!(c.autologin(&avail).is_none());
        c.autologin_session = Some("../x".into());
        assert!(c.validate().is_err());
    }

    #[test]
    fn login_flow_and_throttle() {
        let avail = vec![sess("flowde")];
        let mut m = Machine::new();
        let mut a = Fixed("pw");
        assert_eq!(
            m.login(&mut a, &avail, "alice", "bad", "flowde", 100),
            Err(LoginError::Denied)
        );
        assert_eq!(
            m.login(&mut a, &avail, "alice", "pw", "flowde", 101),
            Err(LoginError::Throttled(2))
        );
        assert_eq!(
            m.login(&mut a, &avail, "alice", "pw", "nope", 200),
            Err(LoginError::UnknownSession)
        );
        assert_eq!(
            m.login(&mut a, &avail, "root", "pw", "flowde", 200),
            Err(LoginError::BadInput("invalid user".into()))
        );
        assert_eq!(
            m.login(&mut a, &avail, "alice", "pw", "flowde", 200),
            Ok(())
        );
        assert!(matches!(m.state, Greeter::Starting { .. }));
        assert_eq!(
            m.login(&mut a, &avail, "alice", "pw", "flowde", 201),
            Err(LoginError::Busy)
        );
        m.session_started();
        assert!(matches!(m.state, Greeter::Running { .. }));
        m.session_ended();
        assert_eq!(m.state, Greeter::Idle);
    }

    #[test]
    fn no_auth_is_honest() {
        let avail = vec![sess("flowde")];
        let mut m = Machine::new();
        let r = m.login(&mut NoAuth, &avail, "alice", "x", "flowde", 0);
        assert!(matches!(r, Err(LoginError::Unavailable(_))));
        assert_eq!(m.state, Greeter::Idle);
    }

    #[test]
    fn name_validation() {
        assert!(
            valid_user("alice_1") && !valid_user("Alice") && !valid_user("a b") && !valid_user("")
        );
        assert!(valid_session_id("fede-x.y") && !valid_session_id("a/b"));
    }
}
