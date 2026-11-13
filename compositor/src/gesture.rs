//! Edge-swipe recogniser (portable, no Smithay). Coordinates are in *logical* (post-rotation) pixels.
//!
//! Policy (iOS/Android-gesture-nav-like, original):
//! * bottom edge swipe up: fast -> Home, slow/held -> Switcher
//! * top edge swipe down: Shade (right third: QuickSettings)
//! * left or right edge swipe inward: Back
//! A touch that starts inside the edge band is *consumed* (never reaches the client);
//! every other touch passes through untouched.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gesture {
    Home,
    Switcher,
    Back,
    Shade,
    QuickSettings,
}

impl Gesture {
    pub fn name(self) -> &'static str {
        match self {
            Gesture::Home => "home",
            Gesture::Switcher => "switcher",
            Gesture::Back => "back",
            Gesture::Shade => "shade",
            Gesture::QuickSettings => "qs",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Bottom,
    Top,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Not an edge touch: deliver to the client.
    Pass,
    /// Edge touch in progress: swallow it.
    Consume,
    /// Edge touch finished as a gesture.
    Fire(Gesture),
}

#[derive(Debug, Clone, Copy)]
struct Track {
    edge: Edge,
    x0: f64,
    y0: f64,
    t0: u32,
    x: f64,
    y: f64,
}

#[derive(Debug, Clone)]
pub struct Recognizer {
    pub width: f64,
    pub height: f64,
    /// Edge band thickness in px.
    pub band: f64,
    /// Minimum travel in px.
    pub min_travel: f64,
    /// px/ms at or above which a bottom swipe is "fast" (Home).
    pub fast: f64,
    track: Option<Track>,
}

impl Recognizer {
    pub fn new(width: f64, height: f64) -> Self {
        Self { width, height, band: 28.0, min_travel: 80.0, fast: 0.6, track: None }
    }
    pub fn resize(&mut self, width: f64, height: f64) {
        self.width = width;
        self.height = height;
        self.track = None;
    }
    pub fn tracking(&self) -> Option<Edge> {
        self.track.map(|t| t.edge)
    }
    /// 0..1 progress of the active edge swipe (for live feedback).
    pub fn progress(&self) -> Option<(Edge, f64)> {
        let t = self.track?;
        let d = match t.edge {
            Edge::Bottom => t.y0 - t.y,
            Edge::Top => t.y - t.y0,
            Edge::Left => t.x - t.x0,
            Edge::Right => t.x0 - t.x,
        };
        Some((t.edge, (d / (self.height * 0.5)).clamp(0.0, 1.0)))
    }

    pub fn down(&mut self, x: f64, y: f64, t: u32) -> Verdict {
        let edge = if y >= self.height - self.band {
            Some(Edge::Bottom)
        } else if y <= self.band {
            Some(Edge::Top)
        } else if x <= self.band {
            Some(Edge::Left)
        } else if x >= self.width - self.band {
            Some(Edge::Right)
        } else {
            None
        };
        match edge {
            Some(edge) => {
                self.track = Some(Track { edge, x0: x, y0: y, t0: t, x, y });
                Verdict::Consume
            }
            None => Verdict::Pass,
        }
    }

    pub fn motion(&mut self, x: f64, y: f64) -> Verdict {
        match self.track.as_mut() {
            Some(tr) => {
                tr.x = x;
                tr.y = y;
                Verdict::Consume
            }
            None => Verdict::Pass,
        }
    }

    pub fn up(&mut self, x: f64, y: f64, t: u32) -> Verdict {
        let Some(tr) = self.track.take() else { return Verdict::Pass };
        let (dx, dy) = (x - tr.x0, y - tr.y0);
        let dt = (t.saturating_sub(tr.t0)).max(1) as f64;
        let g = match tr.edge {
            Edge::Bottom if -dy >= self.min_travel && dx.abs() < -dy * 1.5 => {
                Some(if -dy / dt >= self.fast { Gesture::Home } else { Gesture::Switcher })
            }
            Edge::Top if dy >= self.min_travel && dx.abs() < dy * 1.5 => {
                Some(if tr.x0 > self.width * 2.0 / 3.0 { Gesture::QuickSettings } else { Gesture::Shade })
            }
            Edge::Left if dx >= self.min_travel && dy.abs() < dx * 1.5 => Some(Gesture::Back),
            Edge::Right if -dx >= self.min_travel && dy.abs() < -dx * 1.5 => Some(Gesture::Back),
            _ => None,
        };
        match g {
            Some(g) => Verdict::Fire(g),
            None => Verdict::Consume, // too short: swallowed, no action
        }
    }

    pub fn cancel(&mut self) {
        self.track = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn swipe(r: &mut Recognizer, a: (f64, f64), b: (f64, f64), ms: u32) -> Verdict {
        assert_eq!(r.down(a.0, a.1, 0), Verdict::Consume);
        r.motion((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
        r.up(b.0, b.1, ms)
    }
    #[test]
    fn middle_touch_passes() {
        let mut r = Recognizer::new(540.0, 1170.0);
        assert_eq!(r.down(270.0, 600.0, 0), Verdict::Pass);
        assert_eq!(r.motion(270.0, 500.0), Verdict::Pass);
        assert_eq!(r.up(270.0, 400.0, 100), Verdict::Pass);
    }
    #[test]
    fn bottom_fast_home_slow_switcher() {
        let mut r = Recognizer::new(540.0, 1170.0);
        assert_eq!(swipe(&mut r, (270.0, 1165.0), (270.0, 800.0), 150), Verdict::Fire(Gesture::Home));
        assert_eq!(swipe(&mut r, (270.0, 1165.0), (270.0, 800.0), 900), Verdict::Fire(Gesture::Switcher));
    }
    #[test]
    fn top_and_sides() {
        let mut r = Recognizer::new(540.0, 1170.0);
        assert_eq!(swipe(&mut r, (100.0, 4.0), (100.0, 400.0), 300), Verdict::Fire(Gesture::Shade));
        assert_eq!(swipe(&mut r, (500.0, 4.0), (500.0, 400.0), 300), Verdict::Fire(Gesture::QuickSettings));
        assert_eq!(swipe(&mut r, (3.0, 600.0), (260.0, 610.0), 200), Verdict::Fire(Gesture::Back));
        assert_eq!(swipe(&mut r, (537.0, 600.0), (280.0, 590.0), 200), Verdict::Fire(Gesture::Back));
    }
    #[test]
    fn short_or_diagonal_swallowed() {
        let mut r = Recognizer::new(540.0, 1170.0);
        assert_eq!(swipe(&mut r, (270.0, 1165.0), (270.0, 1130.0), 100), Verdict::Consume);
        assert_eq!(swipe(&mut r, (270.0, 1165.0), (700.0, 1100.0), 100), Verdict::Consume);
    }
    #[test]
    fn rotation_resize_cancels_and_uses_new_edges() {
        let mut r = Recognizer::new(540.0, 1170.0);
        r.down(270.0, 1165.0, 0);
        r.resize(1170.0, 540.0);
        assert_eq!(r.tracking(), None);
        assert_eq!(swipe(&mut r, (600.0, 535.0), (600.0, 200.0), 120), Verdict::Fire(Gesture::Home));
    }
}
