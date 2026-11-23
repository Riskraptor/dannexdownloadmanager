//! Downloads that failed because the network did (no connection, one gone quiet or broken off)
//! start again by themselves: soon at first, then less often, while the network stays away.
//! A download that got bytes since its last failure starts counting again. Stopped by hand,
//! finished or removed: forgotten. Downloads in a running queue are the queue's to retry.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// The first wait; doubled after each failure up to [`MOST`].
const FIRST: Duration = Duration::from_secs(5);
const MOST: Duration = Duration::from_secs(60);
/// Failures in a row (with no bytes between them) before it gives up: about an hour.
pub const TRIES: u32 = 64;

#[derive(Default)]
pub struct AutoResume {
    /// id → (when to start it again, failures in a row, bytes it had at the last one).
    waiting: HashMap<u64, (Instant, u32, u64)>,
    /// Those started again: their count, kept for their next failure.
    running: HashMap<u64, (u32, u64)>,
}

impl AutoResume {
    /// Download `id` failed on the network with `downloaded` bytes: when it starts again, or
    /// None when it has tried long enough.
    pub fn failed(&mut self, id: u64, downloaded: u64, now: Instant) -> Option<Duration> {
        let (tries, before) = self
            .running
            .remove(&id)
            .or_else(|| self.waiting.remove(&id).map(|(_, t, b)| (t, b)))
            .unwrap_or((0, downloaded));
        let tries = if downloaded > before { 1 } else { tries + 1 };
        if tries > TRIES {
            return None;
        }
        let wait = (FIRST * 2u32.pow((tries - 1).min(8))).min(MOST);
        self.waiting.insert(id, (now + wait, tries, downloaded));
        Some(wait)
    }

    /// The downloads whose wait is over, now counted as started again.
    pub fn due(&mut self, now: Instant) -> Vec<u64> {
        let due: Vec<u64> = self
            .waiting
            .iter()
            .filter(|(_, (at, ..))| *at <= now)
            .map(|(id, _)| *id)
            .collect();
        for id in &due {
            if let Some((_, tries, bytes)) = self.waiting.remove(id) {
                self.running.insert(*id, (tries, bytes));
            }
        }
        due
    }

    /// Finished, stopped by hand, removed, or failed some other way: not started again.
    pub fn forget(&mut self, id: u64) {
        self.waiting.remove(&id);
        self.running.remove(&id);
    }

    pub fn is_waiting(&self) -> bool {
        !self.waiting.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waits_grow_then_hold_and_give_up() {
        let mut a = AutoResume::default();
        let t = Instant::now();
        let mut waits = Vec::new();
        for _ in 0..6 {
            waits.push(a.failed(1, 100, t).unwrap().as_secs());
            assert!(a.due(t).is_empty(), "not before its wait");
            assert_eq!(a.due(t + MOST), vec![1]);
        }
        assert_eq!(waits, [5, 10, 20, 40, 60, 60]);
        for _ in 6..TRIES {
            a.failed(1, 100, t).unwrap();
            a.due(t + MOST);
        }
        assert_eq!(a.failed(1, 100, t), None, "gives up after {TRIES}");
        assert!(!a.is_waiting());
    }

    #[test]
    fn bytes_since_the_last_failure_start_the_count_again() {
        let mut a = AutoResume::default();
        let t = Instant::now();
        for _ in 0..4 {
            a.failed(7, 10, t);
            a.due(t + MOST);
        }
        assert_eq!(a.failed(7, 500, t), Some(FIRST));
    }

    #[test]
    fn forgotten_ones_are_not_started() {
        let mut a = AutoResume::default();
        let t = Instant::now();
        a.failed(3, 0, t);
        a.forget(3);
        assert!(a.due(t + MOST).is_empty());
        assert_eq!(a.failed(3, 0, t), Some(FIRST), "counted from the start");
    }
}
