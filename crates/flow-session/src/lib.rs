//! Supervisor logic: unit specs, dependency order, restart backoff, crash-loop cap.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Restart {
    #[default]
    OnFailure,
    Always,
    No,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Unit {
    /// Binary and control-socket name.
    pub name: String,
    pub after: Vec<String>,
    pub restart: Restart,
    /// Started at session start (false = on demand via `session.start`).
    pub autostart: bool,
}

fn u(name: &str, after: &[&str], autostart: bool) -> Unit {
    Unit {
        name: name.into(),
        after: after.iter().map(|s| s.to_string()).collect(),
        restart: Restart::OnFailure,
        autostart,
    }
}

pub fn default_units() -> Vec<Unit> {
    vec![
        u("flow-settings", &[], true),
        u("kiln-notifs", &[], true),
        u("flow-power", &[], true),
        u("flow-sensors", &[], true),
        u("flow-haptics", &["flow-power"], true),
        u("flow-net", &[], true),
        u("kiln-lock", &[], true),
        u("flow-apps", &[], true),
        u(
            "flow-theme",
            &["flow-settings", "flow-power", "flow-sensors"],
            true,
        ),
        u("kiln-wall", &["flow-settings", "flow-theme"], true),
        u("flow-clock", &[], true),
        u("flow-calendar", &[], true),
        u("flow-media", &[], true),
        u("kiln-desktop-mode", &["flow-power"], true),
        u("flow-camera", &[], false),
        u("flow-store", &[], false),
        u("flow-dlp", &[], false),
        u("noxal-ai", &["flow-apps", "flow-settings"], false),
    ]
}

/// Remove disabled units and drop them from the remaining units' `after`
/// lists (a disabled dependency is not an error).
pub fn without(units: Vec<Unit>, disabled: &[String]) -> Vec<Unit> {
    units
        .into_iter()
        .filter(|u| !disabled.contains(&u.name))
        .map(|mut u| {
            u.after.retain(|a| !disabled.contains(a));
            u
        })
        .collect()
}

/// Topological start order (Kahn). Errors on cycles or unknown dependencies.
pub fn order(units: &[Unit]) -> Result<Vec<String>, String> {
    let names: Vec<&str> = units.iter().map(|u| u.name.as_str()).collect();
    for u in units {
        for a in &u.after {
            if !names.contains(&a.as_str()) {
                return Err(format!("{} depends on unknown unit {a}", u.name));
            }
        }
    }
    let mut done: Vec<String> = vec![];
    let mut left: Vec<&Unit> = units.iter().collect();
    while !left.is_empty() {
        let (ready, rest): (Vec<&Unit>, Vec<&Unit>) = left
            .into_iter()
            .partition(|u| u.after.iter().all(|a| done.contains(a)));
        if ready.is_empty() {
            return Err(format!(
                "dependency cycle among: {}",
                rest.iter()
                    .map(|u| u.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        done.extend(ready.iter().map(|u| u.name.clone()));
        left = rest;
    }
    Ok(done)
}

/// All units needed (transitively) to start `name`, in start order.
pub fn closure(units: &[Unit], name: &str) -> Result<Vec<String>, String> {
    let all = order(units)?;
    let mut need = vec![name.to_string()];
    let mut i = 0;
    while i < need.len() {
        let cur = need[i].clone();
        let unit = units
            .iter()
            .find(|x| x.name == cur)
            .ok_or_else(|| format!("unknown unit {cur}"))?;
        for a in &unit.after {
            if !need.contains(a) {
                need.push(a.clone());
            }
        }
        i += 1;
    }
    Ok(all.into_iter().filter(|n| need.contains(n)).collect())
}

#[derive(Debug, Clone)]
pub struct Backoff {
    pub initial: Duration,
    pub max: Duration,
    /// A run at least this long resets the delay.
    pub stable_after: Duration,
    current: Duration,
}

impl Default for Backoff {
    fn default() -> Self {
        Self::new(
            Duration::from_millis(500),
            Duration::from_secs(30),
            Duration::from_secs(60),
        )
    }
}

impl Backoff {
    pub fn new(initial: Duration, max: Duration, stable_after: Duration) -> Self {
        Self {
            initial,
            max,
            stable_after,
            current: initial,
        }
    }
    /// Delay before restarting after a run of `ran_for`.
    pub fn next_delay(&mut self, ran_for: Duration) -> Duration {
        if ran_for >= self.stable_after {
            self.current = self.initial;
        }
        let d = self.current;
        self.current = (self.current * 2).min(self.max);
        d
    }
}

pub const CRASH_WINDOW_S: u64 = 60;
pub const CRASH_LIMIT: usize = 5;
pub const GIVE_UP_S: u64 = 300;

/// Crash-loop breaker: more than CRASH_LIMIT exits within CRASH_WINDOW_S pauses restarts for GIVE_UP_S.
#[derive(Debug, Default, Clone)]
pub struct CrashLoop {
    times: Vec<u64>,
}

impl CrashLoop {
    /// Record a crash at `now`; returns Some(pause_s) when the breaker trips.
    pub fn record(&mut self, now: u64) -> Option<u64> {
        self.times.retain(|t| now < t + CRASH_WINDOW_S);
        self.times.push(now);
        if self.times.len() > CRASH_LIMIT {
            self.times.clear();
            Some(GIVE_UP_S)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum State {
    Stopped,
    Running,
    Backoff,
    GaveUp,
}

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub name: String,
    pub state: State,
    pub pid: Option<u32>,
    pub restarts: u32,
    pub last_exit: Option<i32>,
}

/// Should the unit be restarted after exiting with `code` (None = signal)?
pub fn wants_restart(r: Restart, code: Option<i32>, stopped_by_user: bool) -> bool {
    if stopped_by_user {
        return false;
    }
    match r {
        Restart::No => false,
        Restart::Always => true,
        Restart::OnFailure => code != Some(0),
    }
}

pub type Statuses = BTreeMap<String, Status>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_graph_valid_and_ordered() {
        let units = default_units();
        let o = order(&units).unwrap();
        assert_eq!(o.len(), units.len());
        let pos = |n: &str| o.iter().position(|x| x == n).unwrap();
        assert!(pos("flow-settings") < pos("flow-theme"));
        assert!(pos("flow-theme") < pos("kiln-wall"));
        assert!(pos("flow-power") < pos("flow-haptics"));
    }

    #[test]
    fn disabled_dependencies_are_pruned() {
        let units = without(
            default_units(),
            &["flow-power".to_string(), "flow-sensors".to_string()],
        );
        assert!(order(&units).is_ok());
        let h = units.iter().find(|u| u.name == "flow-haptics").unwrap();
        assert!(h.after.is_empty());
        assert!(units.iter().all(|u| u.name != "flow-power"));
    }

    #[test]
    fn cycle_and_unknown_detected() {
        let a = vec![u("a", &["b"], true), u("b", &["a"], true)];
        assert!(order(&a).unwrap_err().contains("cycle"));
        let b = vec![u("a", &["zzz"], true)];
        assert!(order(&b).unwrap_err().contains("unknown"));
    }

    #[test]
    fn closure_pulls_dependencies() {
        let c = closure(&default_units(), "kiln-wall").unwrap();
        assert_eq!(
            c,
            vec![
                "flow-settings",
                "flow-power",
                "flow-sensors",
                "flow-theme",
                "kiln-wall"
            ]
        );
        assert!(closure(&default_units(), "nope").is_err());
        assert_eq!(
            closure(&default_units(), "flow-net").unwrap(),
            vec!["flow-net"]
        );
    }

    #[test]
    fn backoff_doubles_caps_and_resets() {
        let mut b = Backoff::default();
        let s = Duration::from_secs;
        assert_eq!(b.next_delay(s(1)), Duration::from_millis(500));
        assert_eq!(b.next_delay(s(1)), s(1));
        assert_eq!(b.next_delay(s(1)), s(2));
        for _ in 0..10 {
            b.next_delay(s(1));
        }
        assert_eq!(b.next_delay(s(1)), s(30));
        assert_eq!(b.next_delay(s(120)), Duration::from_millis(500)); // stable run resets
    }

    #[test]
    fn crash_loop_breaker() {
        let mut c = CrashLoop::default();
        for t in 0..5 {
            assert_eq!(c.record(t), None);
        }
        assert_eq!(c.record(6), Some(GIVE_UP_S));
        assert_eq!(c.record(7), None); // reset after tripping
        let mut slow = CrashLoop::default();
        for i in 0..20 {
            assert_eq!(slow.record(i * 30), None); // spread out: never trips
        }
    }

    #[test]
    fn restart_policy() {
        assert!(wants_restart(Restart::OnFailure, Some(1), false));
        assert!(!wants_restart(Restart::OnFailure, Some(0), false));
        assert!(wants_restart(Restart::OnFailure, None, false)); // killed by signal
        assert!(wants_restart(Restart::Always, Some(0), false));
        assert!(!wants_restart(Restart::Always, Some(1), true));
        assert!(!wants_restart(Restart::No, Some(1), false));
    }
}
