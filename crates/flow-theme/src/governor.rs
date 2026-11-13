//! Quality governor: steps down at once, steps up one level only after the
//! better target has been stable for `UP_DWELL_S` (prevents flapping near
//! battery/thermal thresholds, and avoids shader recompiles).

use crate::model::Quality;

pub const UP_DWELL_S: u64 = 10;

#[derive(Debug, Clone)]
pub struct Governor {
    current: Quality,
    /// Time since which the target has been above `current`.
    above_since: Option<u64>,
}

impl Governor {
    pub fn new(initial: Quality) -> Self {
        Self {
            current: initial,
            above_since: None,
        }
    }

    pub fn current(&self) -> Quality {
        self.current
    }

    pub fn update(&mut self, target: Quality, now: u64) -> Quality {
        if target < self.current {
            self.current = target;
            self.above_since = None;
        } else if target > self.current {
            let since = *self.above_since.get_or_insert(now);
            if now >= since + UP_DWELL_S {
                self.current = match self.current {
                    Quality::Flat => Quality::BlurOnly,
                    _ => Quality::Full,
                };
                self.above_since = if self.current < target {
                    Some(now)
                } else {
                    None
                };
            }
        } else {
            self.above_since = None;
        }
        self.current
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Quality::*;

    #[test]
    fn down_immediately_up_after_dwell() {
        let mut g = Governor::new(Full);
        assert_eq!(g.update(Flat, 0), Flat);
        assert_eq!(g.update(Full, 1), Flat);
        assert_eq!(g.update(Full, 10), Flat);
        assert_eq!(g.update(Full, 11), BlurOnly); // one step
        assert_eq!(g.update(Full, 15), BlurOnly);
        assert_eq!(g.update(Full, 21), Full);
    }

    #[test]
    fn flapping_resets_dwell() {
        let mut g = Governor::new(BlurOnly);
        g.update(Full, 0);
        g.update(BlurOnly, 5); // dipped: reset
        assert_eq!(g.update(Full, 9), BlurOnly);
        assert_eq!(g.update(Full, 19), Full);
    }
}
