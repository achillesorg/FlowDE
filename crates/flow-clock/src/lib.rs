//! Clock logic: alarms (weekday masks), timers, stopwatch, wake scheduling.
//! All persisted times are absolute unix seconds so they survive suspend and
//! reboots; after resume the daemon simply re-evaluates what is due.

use flow_common::time::{from_civil, to_civil, Civil};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Alarm {
    pub id: u32,
    pub label: String,
    pub hour: u32,
    pub minute: u32,
    /// Bit 0 = Monday .. bit 6 = Sunday; 0 = one-shot (next occurrence).
    pub days: u8,
    pub enabled: bool,
    /// Snoozed until (unix s).
    pub snoozed_until: Option<u64>,
}

impl Alarm {
    pub fn validate(&self) -> Result<(), String> {
        if self.hour > 23 || self.minute > 59 {
            return Err("invalid time".into());
        }
        if self.days & 0x80 != 0 {
            return Err("invalid weekday mask".into());
        }
        if self.label.len() > 200 {
            return Err("label too long".into());
        }
        Ok(())
    }

    /// Next firing strictly after `after` (unix s) at UTC offset `offset_s`.
    pub fn next_fire(&self, after: u64, offset_s: i64) -> Option<u64> {
        if !self.enabled {
            return None;
        }
        let start = to_civil(after as i64, offset_s);
        for add in 0..=8 {
            let day = flow_common::time::days_from_civil(start.y, start.mo, start.d) + add;
            let (y, mo, d) = flow_common::time::civil_from_days(day);
            let wd = flow_common::time::weekday(day);
            if self.days != 0 && self.days & (1 << wd) == 0 {
                continue;
            }
            let t = from_civil(
                Civil {
                    y,
                    mo,
                    d,
                    h: self.hour,
                    mi: self.minute,
                    s: 0,
                },
                offset_s,
            );
            if t > after as i64 {
                return Some(t as u64);
            }
        }
        None
    }

    /// Effective next fire considering snooze.
    pub fn due_at(&self, after: u64, offset_s: i64) -> Option<u64> {
        match self.snoozed_until {
            Some(s) if s > after => Some(s),
            _ => self.next_fire(after, offset_s),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Timer {
    pub id: u32,
    pub label: String,
    pub ends_at: u64,
    pub duration_s: u64,
    /// When paused: remaining seconds (ends_at is meaningless).
    pub paused_remaining: Option<u64>,
}

impl Timer {
    pub fn remaining(&self, now: u64) -> u64 {
        self.paused_remaining
            .unwrap_or_else(|| self.ends_at.saturating_sub(now))
    }
    pub fn pause(&mut self, now: u64) {
        if self.paused_remaining.is_none() {
            self.paused_remaining = Some(self.remaining(now));
        }
    }
    pub fn resume(&mut self, now: u64) {
        if let Some(r) = self.paused_remaining.take() {
            self.ends_at = now + r;
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Stopwatch {
    /// Accumulated milliseconds while stopped.
    pub acc_ms: u64,
    /// Start (unix ms) when running.
    pub started_ms: Option<u64>,
    pub laps_ms: Vec<u64>,
}

impl Stopwatch {
    pub fn elapsed_ms(&self, now_ms: u64) -> u64 {
        self.acc_ms + self.started_ms.map_or(0, |s| now_ms.saturating_sub(s))
    }
    pub fn start(&mut self, now_ms: u64) {
        self.started_ms.get_or_insert(now_ms);
    }
    pub fn stop(&mut self, now_ms: u64) {
        if let Some(s) = self.started_ms.take() {
            self.acc_ms += now_ms.saturating_sub(s);
        }
    }
    pub fn lap(&mut self, now_ms: u64) {
        if self.started_ms.is_some() {
            let e = self.elapsed_ms(now_ms);
            self.laps_ms.push(e);
        }
    }
    pub fn reset(&mut self) {
        *self = Stopwatch::default();
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClockState {
    pub next_id: u32,
    pub alarms: Vec<Alarm>,
    pub timers: Vec<Timer>,
    pub stopwatch: Stopwatch,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Fired {
    pub kind: &'static str,
    pub id: u32,
    pub label: String,
}

impl ClockState {
    pub fn add_alarm(&mut self, mut a: Alarm) -> Result<u32, String> {
        a.validate()?;
        self.next_id += 1;
        a.id = self.next_id;
        self.alarms.push(a);
        Ok(self.next_id)
    }

    pub fn add_timer(&mut self, label: &str, secs: u64, now: u64) -> Result<u32, String> {
        if secs == 0 || secs > 7 * 86_400 {
            return Err("duration must be 1s..7d".into());
        }
        self.next_id += 1;
        self.timers.push(Timer {
            id: self.next_id,
            label: label.chars().take(200).collect(),
            ends_at: now + secs,
            duration_s: secs,
            paused_remaining: None,
        });
        Ok(self.next_id)
    }

    /// Earliest absolute time (unix s) something needs attention after `now`.
    pub fn next_wake(&self, now: u64, offset_s: i64) -> Option<u64> {
        let a = self.alarms.iter().filter_map(|a| a.due_at(now, offset_s));
        let t = self
            .timers
            .iter()
            .filter(|t| t.paused_remaining.is_none())
            .map(|t| t.ends_at);
        a.chain(t).min()
    }

    /// Collect everything that fired in (`last_check`, `now`]. This is what
    /// runs after a resume: anything missed while suspended fires once.
    pub fn collect_fired(&mut self, last_check: u64, now: u64, offset_s: i64) -> Vec<Fired> {
        let mut out = vec![];
        for a in &mut self.alarms {
            if let Some(t) = a.due_at(last_check, offset_s) {
                if t <= now {
                    out.push(Fired {
                        kind: "alarm",
                        id: a.id,
                        label: a.label.clone(),
                    });
                    a.snoozed_until = None;
                    if a.days == 0 {
                        a.enabled = false;
                    }
                }
            }
        }
        let mut keep = vec![];
        for t in self.timers.drain(..) {
            if t.paused_remaining.is_none() && t.ends_at <= now {
                out.push(Fired {
                    kind: "timer",
                    id: t.id,
                    label: t.label.clone(),
                });
            } else {
                keep.push(t);
            }
        }
        self.timers = keep;
        out
    }

    pub fn snooze(&mut self, id: u32, now: u64, minutes: u64) -> bool {
        match self.alarms.iter_mut().find(|a| a.id == id) {
            Some(a) => {
                a.snoozed_until = Some(now + minutes.clamp(1, 60) * 60);
                true
            }
            None => false,
        }
    }
}

/// argv for a transient user timer that wakes the system (WakeSystem needs the
/// kernel RTC alarm capability; when it is refused the alarm still fires on
/// resume via `collect_fired`). Shell-free; `unit` and `secs` are validated.
pub fn wake_timer_argv(unit: &str, secs_from_now: u64, flow_cli: &str) -> Option<Vec<String>> {
    if unit.is_empty()
        || !unit
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }
    Some(vec![
        "systemd-run".into(),
        "--user".into(),
        format!("--unit={unit}"),
        format!("--on-active={}s", secs_from_now.max(1)),
        "--timer-property=WakeSystem=true".into(),
        "--timer-property=AccuracySec=1s".into(),
        "--collect".into(),
        flow_cli.into(),
        "ipc".into(),
        "flow-clock".into(),
        "clock.tick".into(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2023-11-14 (Tuesday) 22:13:20 UTC
    const T: u64 = 1_700_000_000;

    fn alarm(h: u32, m: u32, days: u8) -> Alarm {
        Alarm {
            id: 0,
            label: "x".into(),
            hour: h,
            minute: m,
            days,
            enabled: true,
            snoozed_until: None,
        }
    }

    #[test]
    fn next_fire_today_or_tomorrow() {
        let a = alarm(23, 0, 0);
        assert_eq!(a.next_fire(T, 0), Some(T + 2800)); // 23:00:00 is 46m40s later
    }

    #[test]
    fn weekday_mask() {
        // Only Saturday (bit 5): 2023-11-18 08:00 UTC
        let a = alarm(8, 0, 1 << 5);
        let t = a.next_fire(T, 0).unwrap();
        let c = flow_common::time::to_civil(t as i64, 0);
        assert_eq!((c.y, c.mo, c.d, c.h, c.mi), (2023, 11, 18, 8, 0));
        // strictly after: asking exactly at fire time gives next week
        let t2 = a.next_fire(t, 0).unwrap();
        assert_eq!(t2, t + 7 * 86_400);
    }

    #[test]
    fn offset_applied() {
        let a = alarm(0, 30, 0);
        // local midnight+30 at +02:00 is 22:30 UTC on the 14th: 17 minutes after T.
        assert_eq!(a.next_fire(T, 7200), Some(T + 1000));
    }

    #[test]
    fn disabled_and_validation() {
        let mut a = alarm(7, 0, 0);
        a.enabled = false;
        assert_eq!(a.next_fire(T, 0), None);
        assert!(alarm(24, 0, 0).validate().is_err());
        assert!(alarm(1, 60, 0).validate().is_err());
        assert!(alarm(1, 1, 0x80).validate().is_err());
    }

    #[test]
    fn snooze_overrides_next() {
        let mut a = alarm(7, 0, 0);
        a.snoozed_until = Some(T + 540);
        assert_eq!(a.due_at(T, 0), Some(T + 540));
        assert_ne!(a.due_at(T + 541, 0), Some(T + 540));
    }

    #[test]
    fn timer_pause_resume() {
        let mut s = ClockState::default();
        let id = s.add_timer("tea", 300, 1000).unwrap();
        let t = s.timers.iter_mut().find(|t| t.id == id).unwrap();
        assert_eq!(t.remaining(1100), 200);
        t.pause(1100);
        assert_eq!(t.remaining(5000), 200);
        t.resume(5000);
        assert_eq!(t.ends_at, 5200);
        assert!(s.add_timer("x", 0, 0).is_err());
    }

    #[test]
    fn stopwatch() {
        let mut w = Stopwatch::default();
        w.start(1000);
        w.lap(1500);
        w.stop(2000);
        w.start(3000);
        assert_eq!(w.elapsed_ms(3500), 1500);
        assert_eq!(w.laps_ms, vec![500]);
        w.reset();
        assert_eq!(w.elapsed_ms(9999), 0);
    }

    #[test]
    fn fires_after_suspend_gap() {
        let mut s = ClockState::default();
        s.add_alarm(alarm(23, 0, 0)).unwrap();
        s.add_timer("t", 60, T).unwrap();
        // device slept from T to T+2h: both missed, each fires exactly once.
        let fired = s.collect_fired(T, T + 7200, 0);
        assert_eq!(fired.len(), 2);
        assert!(!s.alarms[0].enabled); // one-shot disabled
        assert!(s.timers.is_empty());
        assert!(s.collect_fired(T + 7200, T + 8000, 0).is_empty());
    }

    #[test]
    fn repeating_alarm_stays_enabled_and_next_wake() {
        let mut s = ClockState::default();
        s.add_alarm(alarm(23, 0, 0x7f)).unwrap();
        assert!(s.next_wake(T, 0).is_some());
        s.collect_fired(T, T + 3600, 0);
        assert!(s.alarms[0].enabled);
        let nw = s.next_wake(T + 3600, 0).unwrap();
        assert!(nw > T + 3600);
    }

    #[test]
    fn wake_argv_validated() {
        let v = wake_timer_argv("flow-clock-wake", 90, "/usr/bin/flow").unwrap();
        assert!(v.contains(&"--on-active=90s".to_string()));
        assert!(v.contains(&"--timer-property=WakeSystem=true".to_string()));
        assert!(wake_timer_argv("bad unit;rm", 1, "x").is_none());
    }
}
