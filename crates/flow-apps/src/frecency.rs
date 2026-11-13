//! Frecency: launches weighted by age buckets (Firefox-style).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const MAX_SAMPLES: usize = 10;
const DAY: u64 = 86_400;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Frecency {
    /// app id -> most recent launch times (unix s), newest last.
    pub launches: BTreeMap<String, Vec<u64>>,
}

fn weight(age_s: u64) -> f64 {
    match age_s {
        a if a <= 4 * DAY => 100.0,
        a if a <= 14 * DAY => 70.0,
        a if a <= 31 * DAY => 50.0,
        a if a <= 90 * DAY => 30.0,
        _ => 10.0,
    }
}

impl Frecency {
    pub fn record(&mut self, id: &str, now: u64) {
        let v = self.launches.entry(id.to_string()).or_default();
        v.push(now);
        if v.len() > MAX_SAMPLES {
            v.remove(0);
        }
    }

    /// Score = total launches * mean bucket weight of the sampled launches.
    pub fn score(&self, id: &str, now: u64) -> f64 {
        let Some(v) = self.launches.get(id) else {
            return 0.0;
        };
        let sum: f64 = v.iter().map(|t| weight(now.saturating_sub(*t))).sum();
        sum
    }

    /// Top `n` ids by score, ties broken by most recent launch then id.
    pub fn top(&self, n: usize, now: u64) -> Vec<(String, f64)> {
        let mut all: Vec<(String, f64, u64)> = self
            .launches
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    self.score(k, now),
                    v.last().copied().unwrap_or(0),
                )
            })
            .collect();
        all.sort_by(|a, b| b.1.total_cmp(&a.1).then(b.2.cmp(&a.2)).then(a.0.cmp(&b.0)));
        all.into_iter().take(n).map(|(k, s, _)| (k, s)).collect()
    }

    /// Most recent launches, unique ids, newest first.
    pub fn recents(&self, n: usize) -> Vec<String> {
        let mut v: Vec<(&String, u64)> = self
            .launches
            .iter()
            .map(|(k, t)| (k, t.last().copied().unwrap_or(0)))
            .collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        v.into_iter().take(n).map(|(k, _)| k.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frequent_beats_single() {
        let mut f = Frecency::default();
        let now = 1_000_000;
        for i in 0..5 {
            f.record("a", now - i * 100);
        }
        f.record("b", now);
        assert_eq!(f.top(2, now)[0].0, "a");
    }

    #[test]
    fn old_decays() {
        let mut f = Frecency::default();
        let now = 100 * DAY;
        f.record("old", now - 95 * DAY);
        f.record("new", now - DAY);
        assert!(f.score("new", now) > f.score("old", now));
        assert_eq!(f.recents(5), vec!["new", "old"]);
    }

    #[test]
    fn bounded_samples() {
        let mut f = Frecency::default();
        for i in 0..50 {
            f.record("a", i);
        }
        assert_eq!(f.launches["a"].len(), MAX_SAMPLES);
        assert_eq!(f.score("zzz", 5), 0.0);
    }
}
