//! Queues and the Scheduler: starting and stopping queues, what running queues allow at each tick,
//! the download limits, and a queue's "when done".

use super::*;

impl App {
    /// Apply what the Scheduler asks for.
    pub(super) fn scheduler_action(&mut self, a: crate::scheduler::Action) {
        use crate::scheduler::Action;
        let index = |queues: &[Queue], name: &str| queues.iter().position(|q| q.name == name);
        match a {
            Action::Apply { queues, limits } => {
                // A running queue the Scheduler deleted stops first (its downloads stay).
                for q in 0..self.queues.len() {
                    if self.queues[q].running
                        && !queues.iter().any(|n| n.name == self.queues[q].name)
                    {
                        self.stop_queue(q);
                    }
                }
                // Running state belongs to the app, not the draft.
                let running: Vec<String> = self
                    .queues
                    .iter()
                    .filter(|q| q.running)
                    .map(|q| q.name.clone())
                    .collect();
                // The list showing a queue follows it to its new place (All Downloads when
                // it is gone).
                if let View::Queue(i) = self.view {
                    let name = self.queues.get(i).map(|q| q.name.clone());
                    self.view = name
                        .and_then(|n| queues.iter().position(|q| q.name == n))
                        .map_or(View::All, View::Queue);
                }
                self.queues = queues;
                for q in &mut self.queues {
                    q.running = running.contains(&q.name);
                }
                self.settings.limits_on = limits.on;
                self.settings.limits_mb = limits.mb;
                self.settings.limits_hours = limits.hours;
                self.settings.limits_warn = limits.warn;
                self.dirty = true;
            }
            Action::Start(name) => {
                if let Some(q) = index(&self.queues, &name) {
                    self.start_queue(q);
                }
            }
            Action::Stop(name) => {
                if let Some(q) = index(&self.queues, &name) {
                    self.stop_queue(q);
                }
            }
        }
    }

    pub(super) fn start_queue(&mut self, q: usize) {
        if !self.queues[q].running {
            self.sound(2);
        }
        self.queues[q].running = true;
        for id in &self.queues[q].items {
            self.stopped_by_hand.remove(id);
        }
        self.tick_queues();
    }

    pub(super) fn stop_queue(&mut self, q: usize) {
        // Options → Sounds → "Queue processing stopped/finished": stopped by hand, by the
        // Scheduler's stop time or by Stop All, as when it finishes (`queue_done`).
        if self.queues[q].running {
            self.sound(3);
        }
        self.queues[q].running = false;
        for id in self.queues[q].items.clone() {
            self.stop(id);
        }
    }

    /// How a queue sees one of its downloads.
    /// Every download's standing for the queues, by id.
    pub(super) fn members(&self, now: Instant) -> std::collections::HashMap<u64, Member> {
        self.items
            .iter()
            .map(|i| (i.id, self.member_of(i, now)))
            .collect()
    }

    pub(super) fn member_of(&self, i: &Item, now: Instant) -> Member {
        let id = i.id;
        match i.status {
            // Stopped by hand, or a failed one handed to the browser (#376): not retried.
            Status::Stopped | Status::Error if self.stopped_by_hand.contains(&id) => Member::Held,
            Status::Queued | Status::Stopped => Member::Waiting,
            Status::Downloading => Member::Running,
            Status::Complete => Member::Complete,
            Status::Error => {
                let long_ago = now.checked_sub(queue::RETRY_DELAY).unwrap_or(now);
                let (at, tries) = self.failures.get(&id).copied().unwrap_or((long_ago, 0));
                Member::Failed { at, tries }
            }
        }
    }

    /// Scheduler times, then start what running queues allow (notes/ENGINE.md §5).
    pub(super) fn tick_queues(&mut self) {
        use chrono::{Datelike, Timelike};
        // The local time with the offset the clock's thread keeps: asking Windows here held the
        // window for 8 s (#536). Until its first answer, schedules wait.
        let local = crate::clock::now_known();
        let now_s = local.map(|t| t.num_seconds_from_midnight());
        let prev = std::mem::replace(&mut self.clock, now_s).or(now_s);
        if let (Some(local), Some(now_s), Some(prev)) = (local, now_s, prev) {
            let today = local.date_naive();
            let weekday = today.weekday().num_days_from_sunday() as usize;
            let date = (today.year(), today.month(), today.day());
            for q in 0..self.queues.len() {
                let sch = self.queues[q].schedule.clone();
                if let Some(t) = sch.start_at
                    && sch.starts_on(date, weekday)
                    && queue::crossed(t, prev, now_s)
                {
                    // "Queue processing started", as a start by hand plays it (`start_queue`).
                    if !self.queues[q].running {
                        self.sound(2);
                    }
                    self.queues[q].running = true;
                }
                if let Some(t) = sch.stop_at
                    && queue::crossed(t, prev, now_s)
                {
                    self.stop_queue(q);
                }
            }
        }
        let now = Instant::now();
        for q in 0..self.queues.len() {
            if !self.queues[q].running {
                continue;
            }
            // Each download's standing, looked up once (a linear search per queued id before).
            let members = self.members(now);
            let starts = queue::to_start(&self.queues[q], |id| member_in(&members, id), now);
            for id in starts {
                // A queue's download opens no window: its progress window starts hidden (Show
                // progress, the tray) and its end opens no Download complete (atch_quiet).
                self.queue_started.insert(id);
                self.start(id, false);
            }
            let members = self.members(now);
            if queue::finished(&self.queues[q], |id| member_in(&members, id)) {
                self.queues[q].running = false;
                self.queue_done(q);
            }
        }
        // The next wake follows the schedules (set again when they change), and a queue that
        // wakes the computer keeps it awake while it runs (#142).
        self.wake.sync(&self.queues);
        let waking = self
            .queues
            .iter()
            .find(|q| q.running && q.schedule.wake)
            .map(|q| q.name.clone());
        self.wake
            .keep_awake(waking.is_some(), waking.as_deref().unwrap_or_default());
    }

    /// Scheduler → Download limits: stop at the limit, go on when the window has room.
    pub(super) fn check_limits(&mut self, now: Instant) {
        self.quota
            .record(now, self.items.iter().map(|i| (i.id, i.downloaded)));
        let s = &self.settings;
        let limit = u64::from(s.limits_mb.max(1)) << 20;
        let window = Duration::from_secs(u64::from(s.limits_hours.max(1)) * 3600);
        let change = if s.limits_on {
            self.quota.check(now, limit, window)
        } else if self.quota.blocked {
            self.quota.blocked = false;
            Some(false)
        } else {
            None
        };
        match change {
            Some(true) => {
                let running: Vec<u64> = self
                    .items
                    .iter()
                    .filter(|i| i.live.is_some())
                    .map(|i| i.id)
                    .collect();
                for id in running {
                    self.stop(id);
                    self.quota.held.push(id);
                }
                for id in self.quota.held.clone() {
                    if let Some(item) = self.item(id) {
                        item.status = Status::Queued;
                    }
                }
                if self.settings.limits_warn {
                    let wait = self.quota.room_in(now, limit, window);
                    self.limit_notice = Some(dialogs::LimitNotice::new(
                        self.settings.limits_mb,
                        self.settings.limits_hours,
                        wait,
                    ));
                }
                self.dirty = true;
            }
            Some(false) => {
                for id in std::mem::take(&mut self.quota.held) {
                    if let Some(item) = self.item(id) {
                        item.status = Status::Stopped;
                    }
                    self.start(id, false);
                }
                self.dirty = true;
            }
            None => {}
        }
    }

    /// A queue ran out of work: its "when done" actions (Scheduler).
    pub(super) fn queue_done(&mut self, q: usize) {
        self.sound(3);
        let sch = self.queues[q].schedule.clone();
        if let Some(path) = &sch.open_when_done {
            crate::launch::open(path);
        }
        self.when_done(sch.when_done());
    }

    /// A queue's, or a download's (its progress window's When done), "when done":
    /// hang up, turn the computer off (a countdown first), or exit.
    pub(super) fn when_done(&mut self, w: crate::queue::WhenDone) {
        // Ends dial-up (RAS) connections; nothing happens without one. Windows only: macOS and
        // Linux have no dial-up command to call.
        #[cfg(windows)]
        if w.hang_up {
            let _ = std::process::Command::new("rasdial")
                .arg("/disconnect")
                .spawn();
        }
        if let Some(mode) = w.turn_off {
            self.turn_off = Some(dialogs::TurnOffCountdown::new(mode, w.force));
        } else if w.exit {
            self.exit_requested = true;
        }
    }

    /// Delete a queue of the user's (not the two built-in ones); its downloads stay in the list.
    pub(super) fn delete_queue(&mut self, q: usize) {
        if q < 2 || q >= self.queues.len() {
            return;
        }
        if self.queues[q].running {
            self.stop_queue(q);
        }
        self.queues.remove(q);
        self.grab_queue_removed(q);
        // The tree's selection follows the queues after it, and leaves the deleted one.
        self.view = match self.view {
            View::Queue(v) if v == q => View::All,
            View::Queue(v) if v > q => View::Queue(v - 1),
            v => v,
        };
        self.dirty = true;
    }
}

/// A download's standing (a gone one counts as complete: nothing to start).
pub(super) fn member_in(members: &std::collections::HashMap<u64, Member>, id: u64) -> Member {
    members.get(&id).copied().unwrap_or(Member::Complete)
}
