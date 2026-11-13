//! Lock state machine and passcode store.
//!
//! IMPORTANT: this daemon holds *policy and state*. Actual enforcement (hiding
//! the session, input grabs) is the compositor/shell's job; they must treat
//! `locked == true` as authoritative and only unlock through this daemon.

pub mod passcode;

use flow_common::{FlowError, Result};
use passcode::PasscodeStore;
use serde::{Deserialize, Serialize};

/// Lockout schedule after repeated failures: (failures, delay seconds).
/// Failures below the first threshold incur no delay.
pub const LOCKOUT: &[(u32, u64)] = &[(5, 30), (6, 60), (7, 300), (8, 900), (9, 3600)];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LockState {
    Unlocked,
    Locked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    Manual,
    IdleTimeout,
    ScreenOff,
    Boot,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Status {
    pub state: LockState,
    pub reason: Option<Reason>,
    pub has_passcode: bool,
    pub failed_attempts: u32,
    /// Seconds until another attempt is allowed.
    pub retry_after_s: u64,
    pub auto_lock_s: u64,
    pub biometric: bool,
}

/// Biometric hook. Implementations (fprintd, face unlock) live elsewhere;
/// `verify` must only return true after a fresh successful match.
pub trait Biometric: Send {
    fn available(&self) -> bool;
    fn verify(&mut self) -> bool;
}

/// Default: no biometrics.
pub struct NoBiometric;
impl Biometric for NoBiometric {
    fn available(&self) -> bool {
        false
    }
    fn verify(&mut self) -> bool {
        false
    }
}

pub struct LockMachine {
    state: LockState,
    reason: Option<Reason>,
    store: PasscodeStore,
    failed: u32,
    blocked_until: u64,
    /// Seconds the screen may be off/idle before locking; 0 = lock at once.
    pub auto_lock_s: u64,
    screen_off_since: Option<u64>,
    bio: Box<dyn Biometric>,
}

impl LockMachine {
    pub fn new(store: PasscodeStore, auto_lock_s: u64, bio: Box<dyn Biometric>) -> Self {
        // A device with a passcode boots locked.
        let (state, reason) = if store.is_set() {
            (LockState::Locked, Some(Reason::Boot))
        } else {
            (LockState::Unlocked, None)
        };
        Self {
            state,
            reason,
            store,
            failed: 0,
            blocked_until: 0,
            auto_lock_s,
            screen_off_since: None,
            bio,
        }
    }

    pub fn status(&self, now: u64) -> Status {
        Status {
            state: self.state,
            reason: self.reason,
            has_passcode: self.store.is_set(),
            failed_attempts: self.failed,
            retry_after_s: self.blocked_until.saturating_sub(now),
            auto_lock_s: self.auto_lock_s,
            biometric: self.bio.available(),
        }
    }

    pub fn is_locked(&self) -> bool {
        self.state == LockState::Locked
    }

    /// Lock. Without a passcode there is nothing to unlock with, so a lock
    /// request is refused (the shell can still show its own screen-off state).
    pub fn lock(&mut self, reason: Reason) -> Result<bool> {
        if !self.store.is_set() {
            return Err(FlowError::Unavailable("no passcode set".into()));
        }
        let changed = self.state != LockState::Locked;
        self.state = LockState::Locked;
        self.reason = Some(reason);
        Ok(changed)
    }

    /// Try to unlock with a passcode. Wrong code increments the failure
    /// counter and triggers the lockout schedule.
    pub fn unlock_with_pin(&mut self, pin: &str, now: u64) -> Result<bool> {
        if self.state == LockState::Unlocked {
            return Ok(false);
        }
        if now < self.blocked_until {
            return Err(FlowError::Unavailable(format!(
                "locked out for {}s",
                self.blocked_until - now
            )));
        }
        if self.store.verify(pin)? {
            self.on_success();
            Ok(true)
        } else {
            self.failed += 1;
            if let Some((_, delay)) = LOCKOUT.iter().rev().find(|(n, _)| self.failed >= *n) {
                self.blocked_until = now + delay;
            }
            Err(FlowError::Invalid("wrong passcode".into()))
        }
    }

    /// Ask the biometric provider; bypasses the PIN lockout only for a match.
    pub fn unlock_with_biometric(&mut self) -> bool {
        if self.state == LockState::Locked && self.bio.available() && self.bio.verify() {
            self.on_success();
            return true;
        }
        false
    }

    fn on_success(&mut self) {
        self.state = LockState::Unlocked;
        self.reason = None;
        self.failed = 0;
        self.blocked_until = 0;
        self.screen_off_since = None;
    }

    /// Set a passcode. Changing an existing one requires the current one.
    pub fn set_passcode(&mut self, new: &str, current: Option<&str>) -> Result<()> {
        if self.store.is_set()
            && !current
                .map(|c| self.store.verify(c))
                .transpose()?
                .unwrap_or(false)
        {
            return Err(FlowError::Invalid("current passcode required".into()));
        }
        self.store.set(new)
    }

    pub fn clear_passcode(&mut self, current: &str) -> Result<()> {
        if !self.store.verify(current)? {
            return Err(FlowError::Invalid("wrong passcode".into()));
        }
        self.store.clear()?;
        self.state = LockState::Unlocked;
        self.reason = None;
        Ok(())
    }

    /// Screen turned off/on (from flow-power or the compositor).
    /// Returns true if this caused a lock (auto_lock_s == 0 locks immediately).
    pub fn screen_off(&mut self, now: u64) -> bool {
        self.screen_off_since.get_or_insert(now);
        self.tick(now)
    }

    pub fn screen_on(&mut self) {
        self.screen_off_since = None;
    }

    /// Timer tick: lock when the screen has been off long enough.
    pub fn tick(&mut self, now: u64) -> bool {
        match self.screen_off_since {
            Some(t) if self.state == LockState::Unlocked && now >= t + self.auto_lock_s => {
                self.lock(Reason::ScreenOff).unwrap_or(false)
            }
            _ => false,
        }
    }

    /// Seconds until the next auto-lock deadline, for timer scheduling.
    pub fn next_deadline(&self, now: u64) -> Option<u64> {
        if self.state == LockState::Locked {
            return None;
        }
        self.screen_off_since
            .map(|t| (t + self.auto_lock_s).saturating_sub(now))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use passcode::Params;

    fn machine(auto: u64) -> LockMachine {
        let mut m = LockMachine::new(
            PasscodeStore::in_memory(Params::cheap()),
            auto,
            Box::new(NoBiometric),
        );
        m.set_passcode("1234", None).unwrap();
        m
    }

    #[test]
    fn lock_unlock() {
        let mut m = machine(30);
        assert!(!m.is_locked());
        assert!(m.lock(Reason::Manual).unwrap());
        assert!(m.is_locked());
        assert!(m.unlock_with_pin("1234", 0).unwrap());
        assert!(!m.is_locked());
    }

    #[test]
    fn cannot_lock_without_passcode() {
        let mut m = LockMachine::new(
            PasscodeStore::in_memory(Params::cheap()),
            0,
            Box::new(NoBiometric),
        );
        assert!(m.lock(Reason::Manual).is_err());
    }

    #[test]
    fn lockout_schedule() {
        let mut m = machine(30);
        m.lock(Reason::Manual).unwrap();
        for _ in 0..4 {
            assert!(m.unlock_with_pin("0000", 100).is_err());
        }
        assert_eq!(m.status(100).retry_after_s, 0);
        assert!(m.unlock_with_pin("0000", 100).is_err()); // 5th
        assert_eq!(m.status(100).retry_after_s, 30);
        // even the right PIN is refused while blocked
        let e = m.unlock_with_pin("1234", 110).unwrap_err().to_string();
        assert!(e.contains("locked out"));
        assert!(m.unlock_with_pin("1234", 131).unwrap());
        assert_eq!(m.status(131).failed_attempts, 0);
    }

    #[test]
    fn boots_locked_when_passcode_exists() {
        let mut s = PasscodeStore::in_memory(Params::cheap());
        s.set("9999").unwrap();
        let m = LockMachine::new(s, 30, Box::new(NoBiometric));
        assert_eq!(m.status(0).reason, Some(Reason::Boot));
    }

    #[test]
    fn auto_lock_timer() {
        let mut m = machine(30);
        assert!(!m.screen_off(100));
        assert_eq!(m.next_deadline(110), Some(20));
        assert!(!m.tick(129));
        assert!(m.tick(130));
        assert_eq!(m.status(130).reason, Some(Reason::ScreenOff));
        // waking before the deadline cancels
        let mut m = machine(30);
        m.screen_off(0);
        m.screen_on();
        assert!(!m.tick(1000));
    }

    #[test]
    fn immediate_lock_when_zero() {
        let mut m = machine(0);
        assert!(m.screen_off(5));
    }

    #[test]
    fn change_requires_current() {
        let mut m = machine(30);
        assert!(m.set_passcode("5678", None).is_err());
        assert!(m.set_passcode("5678", Some("0000")).is_err());
        m.set_passcode("5678", Some("1234")).unwrap();
        m.lock(Reason::Manual).unwrap();
        assert!(m.unlock_with_pin("5678", 0).unwrap());
        assert!(m.clear_passcode("bad").is_err());
        m.clear_passcode("5678").unwrap();
        assert!(!m.status(0).has_passcode);
    }

    struct Yes;
    impl Biometric for Yes {
        fn available(&self) -> bool {
            true
        }
        fn verify(&mut self) -> bool {
            true
        }
    }

    #[test]
    fn biometric_hook() {
        let mut m = LockMachine::new(PasscodeStore::in_memory(Params::cheap()), 30, Box::new(Yes));
        m.set_passcode("1234", None).unwrap();
        m.lock(Reason::Manual).unwrap();
        assert!(m.status(0).biometric);
        assert!(m.unlock_with_biometric());
        assert!(!m.is_locked());
        let mut n = machine(30);
        n.lock(Reason::Manual).unwrap();
        assert!(!n.unlock_with_biometric());
    }
}
