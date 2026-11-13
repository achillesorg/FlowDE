//! Queue and job state machine. Pure: no threads, no clocks (time is a parameter).

use crate::args::{validate_url, Options};
use crate::engine::Progress;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Queued,
    Downloading,
    Paused,
    Done,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: u32,
    pub url: String,
    pub title: String,
    pub options: Options,
    pub state: State,
    #[serde(skip)]
    pub progress: Progress,
    pub error: Option<String>,
    pub attempts: u32,
    /// Earliest start (unix s) after a retryable failure.
    pub not_before: u64,
}

pub const MAX_ATTEMPTS: u32 = 3;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Queue {
    next: u32,
    pub jobs: Vec<Job>,
    pub max_parallel: usize,
}

/// Retry backoff: 30 s, 2 min, 8 min.
pub fn backoff_s(attempt: u32) -> u64 {
    30 * 4u64.pow(attempt.saturating_sub(1).min(3))
}

impl Queue {
    pub fn new(max_parallel: usize) -> Self {
        Self {
            next: 0,
            jobs: vec![],
            max_parallel: max_parallel.max(1),
        }
    }

    pub fn add(&mut self, url: &str, options: Options) -> Result<u32, String> {
        validate_url(url)?;
        if self.jobs.iter().any(|j| {
            j.url == url && matches!(j.state, State::Queued | State::Downloading | State::Paused)
        }) {
            return Err("this url is already in the queue".into());
        }
        self.next += 1;
        self.jobs.push(Job {
            id: self.next,
            url: url.into(),
            title: String::new(),
            options,
            state: State::Queued,
            progress: Progress::default(),
            error: None,
            attempts: 0,
            not_before: 0,
        });
        Ok(self.next)
    }

    fn job(&mut self, id: u32) -> Option<&mut Job> {
        self.jobs.iter_mut().find(|j| j.id == id)
    }

    /// Jobs to start now (respecting parallelism and retry delays).
    pub fn schedulable(&mut self, now: u64) -> Vec<u32> {
        let running = self
            .jobs
            .iter()
            .filter(|j| j.state == State::Downloading)
            .count();
        let room = self.max_parallel.saturating_sub(running);
        let ids: Vec<u32> = self
            .jobs
            .iter()
            .filter(|j| j.state == State::Queued && j.not_before <= now)
            .map(|j| j.id)
            .take(room)
            .collect();
        for id in &ids {
            if let Some(j) = self.job(*id) {
                j.state = State::Downloading;
                j.attempts += 1;
            }
        }
        ids
    }

    /// Seconds until the earliest delayed retry becomes schedulable (None = nothing waiting).
    pub fn next_retry_in(&self, now: u64) -> Option<u64> {
        self.jobs
            .iter()
            .filter(|j| j.state == State::Queued && j.not_before > now)
            .map(|j| j.not_before - now)
            .min()
    }

    pub fn progress(&mut self, id: u32, p: Progress) {
        if let Some(j) = self.job(id) {
            if j.state == State::Downloading {
                j.progress = p;
            }
        }
    }

    pub fn set_title(&mut self, id: u32, t: &str) {
        if let Some(j) = self.job(id) {
            j.title = t.chars().take(300).collect();
        }
    }

    pub fn finished_ok(&mut self, id: u32) {
        if let Some(j) = self.job(id) {
            if j.state == State::Downloading {
                j.state = State::Done;
                j.error = None;
            }
        }
    }

    /// Record a failure; retryable errors requeue with backoff until MAX_ATTEMPTS.
    pub fn failed(&mut self, id: u32, msg: &str, retryable: bool, now: u64) {
        if let Some(j) = self.job(id) {
            if j.state != State::Downloading {
                return;
            }
            j.error = Some(msg.chars().take(500).collect());
            if retryable && j.attempts < MAX_ATTEMPTS {
                j.state = State::Queued;
                j.not_before = now + backoff_s(j.attempts);
            } else {
                j.state = State::Failed;
            }
        }
    }

    /// Cancel from the worker side or user. Part files are kept for resume
    /// unless the job is removed.
    pub fn cancelled(&mut self, id: u32) {
        if let Some(j) = self.job(id) {
            if j.state == State::Downloading && j.error.as_deref() == Some("paused") {
                j.state = State::Paused;
            } else if matches!(j.state, State::Queued | State::Downloading | State::Paused) {
                j.state = State::Cancelled;
            }
        }
    }

    /// User pause: a running job is marked so the worker's cancel becomes Paused.
    pub fn pause(&mut self, id: u32) -> bool {
        match self.job(id) {
            Some(j) if j.state == State::Downloading => {
                j.error = Some("paused".into());
                true
            }
            Some(j) if j.state == State::Queued => {
                j.state = State::Paused;
                true
            }
            _ => false,
        }
    }

    /// Resume a paused/failed/cancelled job (yt-dlp `--continue` picks up part files).
    pub fn resume(&mut self, id: u32) -> bool {
        match self.job(id) {
            Some(j) if matches!(j.state, State::Paused | State::Failed | State::Cancelled) => {
                j.state = State::Queued;
                j.error = None;
                j.attempts = 0;
                j.not_before = 0;
                true
            }
            _ => false,
        }
    }

    pub fn remove(&mut self, id: u32) -> bool {
        let n = self.jobs.len();
        self.jobs
            .retain(|j| !(j.id == id && j.state != State::Downloading));
        self.jobs.len() != n
    }

    pub fn clear_finished(&mut self) -> usize {
        let n = self.jobs.len();
        self.jobs
            .retain(|j| !matches!(j.state, State::Done | State::Cancelled));
        n - self.jobs.len()
    }

    /// After a restart nothing is running: interrupted downloads are queued again (resumable).
    pub fn recover(&mut self) {
        for j in &mut self.jobs {
            if j.state == State::Downloading {
                j.state = State::Queued;
                j.attempts = j.attempts.saturating_sub(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> Options {
        Options {
            out_dir: "/tmp/x".into(),
            ..Default::default()
        }
    }

    #[test]
    fn schedule_respects_parallelism_and_dedupe() {
        let mut q = Queue::new(1);
        let a = q.add("https://e.com/a", opts()).unwrap();
        let b = q.add("https://e.com/b", opts()).unwrap();
        assert!(q.add("https://e.com/a", opts()).is_err());
        assert!(q.add("file:///x", opts()).is_err());
        assert_eq!(q.schedulable(0), vec![a]);
        assert!(q.schedulable(0).is_empty());
        q.finished_ok(a);
        assert_eq!(q.schedulable(0), vec![b]);
    }

    #[test]
    fn retry_backoff_then_fail() {
        let mut q = Queue::new(2);
        let a = q.add("https://e.com/a", opts()).unwrap();
        q.schedulable(0);
        q.failed(a, "timed out", true, 100);
        assert_eq!(q.jobs[0].state, State::Queued);
        assert_eq!(q.jobs[0].not_before, 130);
        assert!(q.schedulable(129).is_empty());
        assert_eq!(q.schedulable(130), vec![a]);
        q.failed(a, "timed out", true, 200);
        assert_eq!(q.jobs[0].not_before, 320);
        q.schedulable(320);
        q.failed(a, "timed out", true, 400); // third attempt exhausted
        assert_eq!(q.jobs[0].state, State::Failed);
        assert!(q.resume(a));
        assert_eq!(q.jobs[0].attempts, 0);
    }

    #[test]
    fn non_retryable_fails_at_once() {
        let mut q = Queue::new(1);
        let a = q.add("https://e.com/a", opts()).unwrap();
        q.schedulable(0);
        q.failed(a, "Video unavailable", false, 0);
        assert_eq!(q.jobs[0].state, State::Failed);
    }

    #[test]
    fn pause_resume_cancel_remove() {
        let mut q = Queue::new(1);
        let a = q.add("https://e.com/a", opts()).unwrap();
        let b = q.add("https://e.com/b", opts()).unwrap();
        q.schedulable(0);
        assert!(q.pause(a));
        q.cancelled(a); // worker acknowledged
        assert_eq!(q.jobs[0].state, State::Paused);
        assert!(q.pause(b)); // queued -> paused
        assert!(q.resume(a) && q.resume(b));
        q.schedulable(0);
        q.cancelled(a);
        assert_eq!(q.jobs[0].state, State::Cancelled);
        assert!(q.remove(a));
        assert_eq!(q.clear_finished(), 0);
    }

    #[test]
    fn progress_only_while_downloading_and_recover() {
        let mut q = Queue::new(1);
        let a = q.add("https://e.com/a", opts()).unwrap();
        q.progress(
            a,
            Progress {
                downloaded: 5,
                ..Default::default()
            },
        );
        assert_eq!(q.jobs[0].progress.downloaded, 0);
        q.schedulable(0);
        q.progress(
            a,
            Progress {
                downloaded: 5,
                ..Default::default()
            },
        );
        assert_eq!(q.jobs[0].progress.downloaded, 5);
        q.recover();
        assert_eq!(q.jobs[0].state, State::Queued);
        assert_eq!(q.jobs[0].attempts, 0);
    }

    #[test]
    fn next_retry_timer() {
        let mut q = Queue::new(1);
        let a = q.add("https://e.com/a", opts()).unwrap();
        assert_eq!(q.next_retry_in(0), None);
        q.schedulable(0);
        q.failed(a, "timed out", true, 100);
        assert_eq!(q.next_retry_in(100), Some(30));
        assert_eq!(q.next_retry_in(130), None);
    }

    #[test]
    fn backoff_values() {
        assert_eq!((backoff_s(1), backoff_s(2), backoff_s(3)), (30, 120, 480));
    }
}
