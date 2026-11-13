//! Notification store: ids, replacement, grouping by app, do-not-disturb,
//! expiry, bounded history, JSON persistence. Pure logic, no D-Bus.

use flow_common::{FlowError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub const MAX_HISTORY: usize = 200;
pub const MAX_TEXT: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Urgency {
    Low,
    #[default]
    Normal,
    Critical,
}

impl Urgency {
    pub fn from_hint(b: u8) -> Self {
        match b {
            0 => Urgency::Low,
            2 => Urgency::Critical,
            _ => Urgency::Normal,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Notification {
    pub id: u32,
    pub app: String,
    pub icon: String,
    pub summary: String,
    pub body: String,
    pub urgency: Urgency,
    pub actions: Vec<(String, String)>,
    /// Unix seconds.
    pub time: u64,
    /// Unix seconds when it stops being shown as a banner; None = sticky.
    pub expires: Option<u64>,
    /// True once removed from the active tray (kept in history).
    pub dismissed: bool,
    /// Suppressed from banners/sound by DND when posted.
    pub silenced: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum Dnd {
    #[default]
    Off,
    On,
    /// Until unix seconds.
    Until {
        until: u64,
    },
}

impl Dnd {
    pub fn active(&self, now: u64) -> bool {
        match self {
            Dnd::Off => false,
            Dnd::On => true,
            Dnd::Until { until } => now < *until,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct NewNotification {
    pub app: String,
    pub replaces: u32,
    pub icon: String,
    pub summary: String,
    pub body: String,
    pub urgency: Urgency,
    pub actions: Vec<(String, String)>,
    /// Spec: -1 server default, 0 never, >0 milliseconds.
    pub expire_ms: i32,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Persist {
    next_id: u32,
    dnd: Dnd,
    items: Vec<Notification>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Group {
    pub app: String,
    pub count: usize,
    pub latest: Notification,
}

/// Default banner lifetime for normal/low urgency.
pub const DEFAULT_EXPIRE_S: u64 = 6;

pub struct Store {
    data: Persist,
    path: Option<PathBuf>,
}

fn clip(s: &str) -> String {
    s.chars().take(MAX_TEXT).collect()
}

impl Store {
    pub fn new(path: Option<PathBuf>) -> Result<Store> {
        let data = match &path {
            Some(p) => match std::fs::read_to_string(p) {
                Ok(t) => serde_json::from_str(&t).unwrap_or_else(|e| {
                    tracing::warn!("notification store unreadable ({e}), starting empty");
                    Persist::default()
                }),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Persist::default(),
                Err(e) => return Err(e.into()),
            },
            None => Persist::default(),
        };
        Ok(Store { data, path })
    }

    fn save(&self) {
        if let Some(p) = &self.path {
            if let Ok(t) = serde_json::to_vec(&self.data) {
                if let Err(e) = flow_common::paths::write_atomic(p, &t) {
                    tracing::warn!("cannot persist notifications: {e}");
                }
            }
        }
    }

    pub fn dnd(&self) -> Dnd {
        self.data.dnd
    }

    pub fn set_dnd(&mut self, d: Dnd) {
        self.data.dnd = d;
        self.save();
    }

    /// Post or replace. Returns the stored notification.
    pub fn post(&mut self, n: NewNotification, now: u64) -> Notification {
        let dnd = self.data.dnd.active(now);
        // Critical notifications always get through.
        let silenced = dnd && n.urgency != Urgency::Critical;
        let expires = match n.expire_ms {
            0 => None,
            ms if ms > 0 => Some(now + (ms as u64).div_ceil(1000)),
            _ if n.urgency == Urgency::Critical => None,
            _ => Some(now + DEFAULT_EXPIRE_S),
        };
        let reuse = n.replaces != 0 && self.data.items.iter().any(|i| i.id == n.replaces);
        let id = if reuse {
            n.replaces
        } else {
            self.data.next_id = self.data.next_id.wrapping_add(1).max(1);
            self.data.next_id
        };
        let item = Notification {
            id,
            app: clip(&n.app),
            icon: clip(&n.icon),
            summary: clip(&n.summary),
            body: clip(&n.body),
            urgency: n.urgency,
            actions: n.actions,
            time: now,
            expires,
            dismissed: false,
            silenced,
        };
        if reuse {
            if let Some(slot) = self.data.items.iter_mut().find(|i| i.id == id) {
                *slot = item.clone();
            }
        } else {
            self.data.items.push(item.clone());
        }
        let len = self.data.items.len();
        if len > MAX_HISTORY {
            self.data.items.drain(0..len - MAX_HISTORY);
        }
        self.save();
        item
    }

    /// Remove from the active tray; history keeps it.
    pub fn dismiss(&mut self, id: u32) -> Result<()> {
        let i = self
            .data
            .items
            .iter_mut()
            .find(|i| i.id == id)
            .ok_or_else(|| FlowError::NotFound(format!("notification {id}")))?;
        i.dismissed = true;
        self.save();
        Ok(())
    }

    pub fn dismiss_app(&mut self, app: &str) -> usize {
        let mut n = 0;
        for i in self
            .data
            .items
            .iter_mut()
            .filter(|i| i.app == app && !i.dismissed)
        {
            i.dismissed = true;
            n += 1;
        }
        self.save();
        n
    }

    pub fn clear_history(&mut self) {
        self.data.items.clear();
        self.save();
    }

    pub fn active(&self) -> Vec<&Notification> {
        self.data.items.iter().filter(|i| !i.dismissed).collect()
    }

    /// Newest first, bounded.
    pub fn history(&self, limit: usize) -> Vec<&Notification> {
        self.data.items.iter().rev().take(limit).collect()
    }

    /// Active notifications grouped by app, groups ordered by latest time desc.
    pub fn groups(&self) -> Vec<Group> {
        let mut by: BTreeMap<&str, Vec<&Notification>> = BTreeMap::new();
        for n in self.active() {
            by.entry(n.app.as_str()).or_default().push(n);
        }
        let mut out: Vec<Group> = by
            .into_iter()
            .map(|(app, v)| Group {
                app: app.to_string(),
                count: v.len(),
                latest: (*v.iter().max_by_key(|n| (n.time, n.id)).unwrap()).clone(),
            })
            .collect();
        out.sort_by(|a, b| {
            b.latest
                .time
                .cmp(&a.latest.time)
                .then(b.latest.id.cmp(&a.latest.id))
        });
        out
    }

    /// Banner expiry: returns ids whose banner timed out at `now`
    /// (they remain in the tray unless the shell dismisses them).
    pub fn expired_banners(&self, now: u64) -> Vec<u32> {
        self.data
            .items
            .iter()
            .filter(|i| !i.dismissed && i.expires.is_some_and(|e| e <= now))
            .map(|i| i.id)
            .collect()
    }

    pub fn get(&self, id: u32) -> Option<&Notification> {
        self.data.items.iter().find(|i| i.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(app: &str, s: &str) -> NewNotification {
        NewNotification {
            app: app.into(),
            summary: s.into(),
            expire_ms: -1,
            ..Default::default()
        }
    }

    #[test]
    fn ids_increase_and_replace_keeps_id() {
        let mut s = Store::new(None).unwrap();
        let a = s.post(n("a", "1"), 100);
        let b = s.post(n("a", "2"), 101);
        assert_eq!((a.id, b.id), (1, 2));
        let c = s.post(
            NewNotification {
                replaces: 1,
                ..n("a", "1b")
            },
            102,
        );
        assert_eq!(c.id, 1);
        assert_eq!(s.active().len(), 2);
        assert_eq!(s.get(1).unwrap().summary, "1b");
    }

    #[test]
    fn replace_unknown_id_allocates_new() {
        let mut s = Store::new(None).unwrap();
        let a = s.post(
            NewNotification {
                replaces: 99,
                ..n("a", "x")
            },
            1,
        );
        assert_eq!(a.id, 1);
    }

    #[test]
    fn dnd_silences_but_critical_passes() {
        let mut s = Store::new(None).unwrap();
        s.set_dnd(Dnd::Until { until: 200 });
        assert!(s.post(n("a", "x"), 100).silenced);
        let crit = s.post(
            NewNotification {
                urgency: Urgency::Critical,
                ..n("a", "y")
            },
            100,
        );
        assert!(!crit.silenced);
        assert!(!s.post(n("a", "z"), 200).silenced); // window over
                                                     // silenced still stored in tray/history
        assert_eq!(s.history(10).len(), 3);
    }

    #[test]
    fn expiry_rules() {
        let mut s = Store::new(None).unwrap();
        let a = s.post(n("a", "x"), 100);
        assert_eq!(a.expires, Some(100 + DEFAULT_EXPIRE_S));
        let b = s.post(
            NewNotification {
                expire_ms: 0,
                ..n("a", "y")
            },
            100,
        );
        assert_eq!(b.expires, None);
        let c = s.post(
            NewNotification {
                urgency: Urgency::Critical,
                ..n("a", "z")
            },
            100,
        );
        assert_eq!(c.expires, None);
        let d = s.post(
            NewNotification {
                expire_ms: 1500,
                ..n("a", "w")
            },
            100,
        );
        assert_eq!(d.expires, Some(102));
        assert_eq!(s.expired_banners(107), vec![a.id, d.id]);
    }

    #[test]
    fn grouping_and_dismiss() {
        let mut s = Store::new(None).unwrap();
        s.post(n("chat", "1"), 10);
        s.post(n("mail", "m"), 11);
        s.post(n("chat", "2"), 12);
        let g = s.groups();
        assert_eq!(g[0].app, "chat");
        assert_eq!(g[0].count, 2);
        assert_eq!(g[0].latest.summary, "2");
        assert_eq!(s.dismiss_app("chat"), 2);
        assert_eq!(s.groups().len(), 1);
        assert_eq!(s.history(10).len(), 3);
        assert!(s.dismiss(42).is_err());
    }

    #[test]
    fn history_bounded_and_text_clipped() {
        let mut s = Store::new(None).unwrap();
        for i in 0..(MAX_HISTORY + 10) {
            s.post(n("a", &i.to_string()), i as u64);
        }
        assert_eq!(s.history(1000).len(), MAX_HISTORY);
        let big = "x".repeat(MAX_TEXT * 2);
        assert_eq!(s.post(n("a", &big), 1).summary.chars().count(), MAX_TEXT);
    }

    #[test]
    fn persistence_roundtrip_keeps_dnd_and_ids() {
        let d = std::env::temp_dir().join(format!("kiln-notifs-{}", std::process::id()));
        let p = d.join("n.json");
        {
            let mut s = Store::new(Some(p.clone())).unwrap();
            s.post(n("a", "x"), 1);
            s.set_dnd(Dnd::On);
        }
        let mut s = Store::new(Some(p.clone())).unwrap();
        assert_eq!(s.dnd(), Dnd::On);
        assert_eq!(s.post(n("a", "y"), 2).id, 2);
        std::fs::write(&p, "garbage").unwrap();
        assert!(Store::new(Some(p)).unwrap().history(5).is_empty());
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn urgency_hint() {
        assert_eq!(Urgency::from_hint(0), Urgency::Low);
        assert_eq!(Urgency::from_hint(1), Urgency::Normal);
        assert_eq!(Urgency::from_hint(2), Urgency::Critical);
    }
}
