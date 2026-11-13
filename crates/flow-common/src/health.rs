//! Health and cheap metrics shared by every daemon.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

#[derive(Debug)]
pub struct Metrics {
    started: Instant,
    pub requests: AtomicU64,
    pub errors: AtomicU64,
    pub events: AtomicU64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Health {
    pub name: String,
    pub version: String,
    pub uptime_s: u64,
    pub requests: u64,
    pub errors: u64,
    pub events: u64,
    /// Resident set size in KiB (Linux only).
    pub rss_kb: Option<u64>,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
            requests: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            events: AtomicU64::new(0),
        }
    }
    pub fn inc(&self, c: &AtomicU64) {
        c.fetch_add(1, Ordering::Relaxed);
    }
    pub fn health(&self, name: &str) -> Health {
        Health {
            name: name.to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_s: self.started.elapsed().as_secs(),
            requests: self.requests.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
            events: self.events.load(Ordering::Relaxed),
            rss_kb: rss_kb(),
        }
    }
}

/// Parse the second field of `/proc/self/statm` (resident pages) to KiB.
pub fn parse_statm(s: &str, page_kb: u64) -> Option<u64> {
    s.split_whitespace()
        .nth(1)?
        .parse::<u64>()
        .ok()
        .map(|p| p * page_kb)
}

pub fn rss_kb() -> Option<u64> {
    let s = std::fs::read_to_string("/proc/self/statm").ok()?;
    parse_statm(&s, 4) // 4 KiB pages; 16K-page kernels are reported conservatively low
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statm() {
        assert_eq!(parse_statm("100 25 3 0 0 0 0", 4), Some(100));
        assert_eq!(parse_statm("", 4), None);
    }

    #[test]
    fn counters() {
        let m = Metrics::new();
        m.inc(&m.requests);
        m.inc(&m.requests);
        let h = m.health("x");
        assert_eq!(h.requests, 2);
        assert_eq!(h.name, "x");
    }
}
