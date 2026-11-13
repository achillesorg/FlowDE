//! MPRIS2 aggregation logic (pure): which player is "active", state
//! summaries and media-key routing.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MPRIS_PREFIX: &str = "org.mpris.MediaPlayer2.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Playing,
    Paused,
    #[default]
    Stopped,
}

impl Status {
    pub fn parse(s: &str) -> Status {
        match s {
            "Playing" => Status::Playing,
            "Paused" => Status::Paused,
            _ => Status::Stopped,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Player {
    /// Bus name suffix, e.g. `firefox.instance123`.
    pub id: String,
    pub identity: String,
    pub status: Status,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art_url: String,
    pub length_us: i64,
    pub position_us: i64,
    pub can_next: bool,
    pub can_prev: bool,
    pub can_control: bool,
    /// Logical clock of the last time it was seen playing (for selection).
    pub last_playing: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Key {
    PlayPause,
    Play,
    Pause,
    Stop,
    Next,
    Previous,
}

impl Key {
    /// Method name on `org.mpris.MediaPlayer2.Player`.
    pub fn method(self) -> &'static str {
        match self {
            Key::PlayPause => "PlayPause",
            Key::Play => "Play",
            Key::Pause => "Pause",
            Key::Stop => "Stop",
            Key::Next => "Next",
            Key::Previous => "Previous",
        }
    }
}

#[derive(Debug, Default)]
pub struct Aggregator {
    pub players: BTreeMap<String, Player>,
    clock: u64,
    /// Explicit user choice overrides auto selection while that player exists.
    pub pinned: Option<String>,
}

impl Aggregator {
    pub fn update(&mut self, mut p: Player) {
        self.clock += 1;
        let prev = self.players.get(&p.id).map(|x| x.last_playing).unwrap_or(0);
        p.last_playing = if p.status == Status::Playing {
            self.clock
        } else {
            prev
        };
        self.players.insert(p.id.clone(), p);
    }

    pub fn remove(&mut self, id: &str) {
        self.players.remove(id);
        if self.pinned.as_deref() == Some(id) {
            self.pinned = None;
        }
    }

    /// Active player: pinned > currently playing (most recent) > most recently
    /// played > paused > any.
    pub fn active(&self) -> Option<&Player> {
        if let Some(p) = self.pinned.as_ref().and_then(|id| self.players.get(id)) {
            return Some(p);
        }
        self.players.values().max_by_key(|p| {
            (
                p.status == Status::Playing,
                p.last_playing,
                p.status == Status::Paused,
                std::cmp::Reverse(p.id.clone()),
            )
        })
    }

    /// Resolve a media key to (player id, method). PlayPause on nothing returns None.
    pub fn route(&self, k: Key) -> Option<(String, &'static str)> {
        let p = self.active()?;
        if !p.can_control {
            return None;
        }
        let ok = match k {
            Key::Next => p.can_next,
            Key::Previous => p.can_prev,
            _ => true,
        };
        ok.then(|| (p.id.clone(), k.method()))
    }
}

/// Extract a string from an MPRIS metadata value that may be `s` or `as`.
pub fn first_string(vals: &[String]) -> String {
    vals.first().cloned().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pl(id: &str, st: Status) -> Player {
        Player {
            id: id.into(),
            status: st,
            can_control: true,
            can_next: true,
            ..Default::default()
        }
    }

    #[test]
    fn playing_beats_paused_and_recency() {
        let mut a = Aggregator::default();
        a.update(pl("a", Status::Playing));
        a.update(pl("b", Status::Paused));
        assert_eq!(a.active().unwrap().id, "a");
        a.update(pl("b", Status::Playing));
        assert_eq!(a.active().unwrap().id, "b"); // most recent playing
        a.update(pl("b", Status::Paused));
        a.update(pl("a", Status::Paused));
        assert_eq!(a.active().unwrap().id, "b"); // last one that played
    }

    #[test]
    fn pin_and_removal() {
        let mut a = Aggregator::default();
        a.update(pl("a", Status::Playing));
        a.update(pl("b", Status::Paused));
        a.pinned = Some("b".into());
        assert_eq!(a.active().unwrap().id, "b");
        a.remove("b");
        assert_eq!(a.active().unwrap().id, "a");
        assert!(a.pinned.is_none());
        a.remove("a");
        assert!(a.active().is_none());
        assert!(a.route(Key::PlayPause).is_none());
    }

    #[test]
    fn key_routing_respects_capabilities() {
        let mut a = Aggregator::default();
        let mut p = pl("a", Status::Playing);
        p.can_next = false;
        a.update(p);
        assert_eq!(a.route(Key::PlayPause), Some(("a".into(), "PlayPause")));
        assert_eq!(a.route(Key::Next), None);
        let mut q = pl("b", Status::Playing);
        q.can_control = false;
        a.update(q);
        assert_eq!(a.route(Key::Pause), None);
    }

    #[test]
    fn status_parse() {
        assert_eq!(Status::parse("Playing"), Status::Playing);
        assert_eq!(Status::parse("x"), Status::Stopped);
        assert_eq!(first_string(&[]), "");
    }
}
