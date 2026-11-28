//! Speed limits: the Speed Limiter's for each file and for all downloads, the limits videos and
//! galleries run under (#708), and the speeds the status strip and the global cap measure.

use super::*;

impl App {
    /// Apply the Speed Limiter's "for one file" setting to every running download, file or
    /// torrent; one with a remembered limit of its own keeps it, as when it starts. Videos and
    /// galleries start again at their new limit (`reapply_helper_limits`).
    pub(super) fn apply_speed_limit(&mut self) {
        let limit = self.settings.speed_limit();
        for i in &self.items {
            if let Some(l) = &i.live {
                l.dl.set_speed_limit(i.own_limit.or(limit));
            }
        }
        self.reapply_helper_limits();
    }

    /// The limits a video or a gallery (#708) is under, as a file download's are set: its own
    /// limit (set in its progress window, else remembered), else the Speed Limiter's for one
    /// file; and the limit for all downloads together, when on. A change of these starts it
    /// again (`reapply_helper_limits`).
    pub(super) fn helper_limits(&self, id: u64) -> (Option<u64>, Option<u64>) {
        let own = self
            .helper_own
            .get(&id)
            .copied()
            .unwrap_or_else(|| self.items.iter().find(|i| i.id == id)?.own_limit);
        let s = &self.settings;
        (
            own.or(s.speed_limit()),
            s.global_limit_on.then(|| s.global_limit_kb.max(1) * 1024),
        )
    }

    /// The speed limit a video or a gallery (#708) starts with, bytes/s: the lower of its limit
    /// for one file (`helper_limits`) and an even share, with the other videos and galleries
    /// running, of the limit for all downloads together as file downloads have it now
    /// (`apply_global_limit`: what torrents leave of it).
    pub(super) fn helper_rate(&self, id: u64) -> Option<u64> {
        let (one, _) = self.helper_limits(id);
        let others = self
            .items
            .iter()
            .filter(|i| i.id != id && i.live.as_ref().is_some_and(|l| l.dl.is_helper()))
            .count() as u64;
        helper_rate(one, self.global.get(), others)
    }

    /// The speed limits changed (the Speed Limiter turned on or off, its settings, a download's
    /// own limit): every running video and gallery whose limit is now another is stopped and
    /// started again with it (#708). yt-dlp continues its partial file, gallery-dl skips the
    /// files already saved, as when the network adapter changes.
    pub(super) fn reapply_helper_limits(&mut self) {
        let changed: Vec<u64> = self
            .items
            .iter()
            .filter(|i| i.live.as_ref().is_some_and(|l| l.dl.is_helper()))
            .filter(|i| self.helper_rates.get(&i.id).copied() != Some(self.helper_limits(i.id)))
            .map(|i| i.id)
            .collect();
        self.restart_helpers(changed);
    }

    /// A video or a gallery `id` has started under the limit for all downloads (#708): the
    /// others running that were started with more than their even share now are started again
    /// with it, so together they keep to the limit. Starting several at once (a queue, a
    /// playlist) restarts only the ones started a moment before, still looking their page up;
    /// one that ends leaves the others as they are (under the limit).
    pub(super) fn share_with_new_helper(&mut self, id: u64) {
        if self.global.get().is_none() || !self.item(id).is_some_and(|i| i.live.is_some()) {
            return;
        }
        let over: Vec<u64> = self
            .items
            .iter()
            .filter(|i| i.id != id && i.live.as_ref().is_some_and(|l| l.dl.is_helper()))
            .filter(|i| {
                let now = self.helper_rate(i.id);
                match (self.helper_started.get(&i.id).copied().flatten(), now) {
                    (Some(had), Some(now)) => had > now,
                    (None, Some(_)) => true,
                    _ => false,
                }
            })
            .map(|i| i.id)
            .collect();
        self.restart_helpers(over);
    }

    /// Stop the videos and galleries `ids` to start them again with their speed limit now
    /// (`limit_restart_tick`).
    pub(super) fn restart_helpers(&mut self, ids: Vec<u64>) {
        for id in ids {
            if let Some(i) = self.items.iter_mut().find(|i| i.id == id)
                && let Some(l) = &i.live
            {
                l.dl.cancel();
                i.note(
                    // i18n: event log entry: a video or gallery download stopped to start again at the new speed limit (its helper program takes a limit only as it starts)
                    crate::i18n::t("Stopped: the speed limit changed; starting again with it")
                        .to_owned(),
                );
            }
            if !self.limit_restarts.contains(&id) {
                self.limit_restarts.push(id);
            }
        }
    }

    /// Videos and galleries stopped for a new speed limit, started again once their jobs have
    /// ended (#708).
    pub(super) fn limit_restart_tick(&mut self) {
        if self.limit_restarts.is_empty() {
            return;
        }
        let ready: Vec<u64> = self
            .limit_restarts
            .iter()
            .copied()
            .filter(|id| self.items.iter().any(|i| i.id == *id && i.live.is_none()))
            .collect();
        for &id in &ready {
            self.start(id, false);
        }
        self.limit_restarts
            .retain(|id| self.items.iter().any(|i| i.id == *id) && !ready.contains(id));
    }

    /// The global cap's rate now: on, and (if asked) only while the user is active (input in
    /// the last minute).
    ///
    /// Torrents share it with file downloads (D-012, "one limit for all downloads together"),
    /// evenly: each side may take half of it, and more when the other uses less (`share`).
    /// Both sides' speeds are measured each frame: the torrents' as the torrent session's
    /// download limit, file downloads' as the global limiter they wait on.
    pub(super) fn apply_global_limit(&mut self) {
        let s = &self.settings;
        #[cfg(windows)]
        let active = crate::win32::idle_seconds() < 60;
        #[cfg(not(windows))]
        let active = true;
        let rate = (s.global_limit_on && (!s.global_only_active || active))
            .then(|| s.global_limit_kb.max(1) * 1024);
        let torrents = self.torrent_speed();
        let files = self.file_speed();
        // The torrents' limit moves in eighths of the cap, at most every 3 s: librqbit starts
        // each new limit with a second's burst, so following the other side's speed closely
        // let torrents run far past their share (measured: 773 KB/s for a 400 KB/s half).
        let want = rate.map(|cap| {
            let step = (cap / 8).max(1);
            (share(cap, files) / step * step).max(1024)
        });
        let (had, when) = self.torrent_cap;
        // Down at once (the other side has started: give way now), up only 3 s after the last
        // change.
        let moved = match (had, want) {
            (Some(a), Some(b)) => b < a || (b > a && when.elapsed() >= Duration::from_secs(3)),
            (a, b) => a != b,
        };
        if moved {
            self.torrent_cap = (want, Instant::now());
        }
        self.torrent_host.set_cap(self.torrent_cap.0);
        let for_files = rate.map(|cap| share(cap, torrents));
        if self.global.get() != for_files {
            self.global.set(for_files);
        }
    }

    /// File downloads' speed together, bytes/s, smoothed over about two seconds: the bytes that
    /// went through the global limiter, which every file download waits on. (Their progress
    /// rates read high and low by a quarter, which tipped the split.)
    pub(super) fn file_speed(&mut self) -> f64 {
        let now = Instant::now();
        let taken = self.global.taken();
        let (speed, at, before) = self.file_speed;
        let dt = now.duration_since(at).as_secs_f64();
        if dt <= 0.0 {
            return speed;
        }
        let new = taken.saturating_sub(before) as f64;
        let k = 1.0 - (-dt / 2.0).exp();
        let mut speed = speed + (new / dt - speed) * k;
        // None running: 0 at once, so the torrents take the whole cap back in one step.
        if !self
            .items
            .iter()
            .any(|i| matches!(i.live.as_ref().map(|l| &l.dl), Some(model::Job::File(_))))
        {
            speed = 0.0;
        }
        self.file_speed = (speed, now, taken);
        speed
    }

    /// The torrents' download speed together, bytes/s, smoothed over about two seconds (from
    /// the bytes librqbit says each fetched; it counts from 0 again each time a torrent
    /// resumes, so a smaller count is all new).
    pub(super) fn torrent_speed(&mut self) -> f64 {
        let now = Instant::now();
        let mut seen = HashMap::new();
        let mut new = 0u64;
        for i in &self.items {
            let Some(model::Job::Torrent(run)) = i.live.as_ref().map(|l| &l.dl) else {
                continue;
            };
            let Some(fetched) = run.fetched() else {
                continue;
            };
            if let Some(&before) = self.torrent_fetched.get(&i.id) {
                new += fetched.checked_sub(before).unwrap_or(fetched);
            }
            seen.insert(i.id, fetched);
        }
        self.torrent_fetched = seen;
        let dt = now.duration_since(self.torrent_speed.1).as_secs_f64();
        self.torrent_speed.1 = now;
        if dt > 0.0 {
            let k = 1.0 - (-dt / 2.0).exp();
            self.torrent_speed.0 += (new as f64 / dt - self.torrent_speed.0) * k;
        }
        if self.torrent_fetched.is_empty() {
            self.torrent_speed.0 = 0.0;
        }
        self.torrent_speed.0
    }

    /// Progress of all running downloads together (bytes done / bytes in all), None when
    /// nothing runs or no size is known. For the taskbar button's progress, which only Windows
    /// has (macOS and Linux docks have no common way to show one), and the tray icon's bar.
    pub(super) fn overall_progress(&self) -> Option<f64> {
        let (done, total) = self
            .items
            .iter()
            .filter(|i| i.live.is_some() && i.listed())
            .filter_map(|i| i.size.map(|s| (i.downloaded, s)))
            .fold((0u64, 0u64), |(d, t), (di, si)| (d + di.min(si), t + si));
        (total > 0).then(|| done as f64 / total as f64)
    }

    /// The speed limit a video or gallery starts with now (`helper_rate`); the limits it is under
    /// are kept to tell a change (`reapply_helper_limits`). A limit set in its progress window lasts until it is stopped:
    /// a start other than for a new limit forgets it.
    pub(super) fn helper_starts(&mut self, id: u64) -> Option<u64> {
        // Started again for a new limit (by `limit_restart_tick`, or its queue first): done.
        if let Some(at) = self.limit_restarts.iter().position(|x| *x == id) {
            self.limit_restarts.remove(at);
        } else {
            self.helper_own.remove(&id);
        }
        self.helper_rates.insert(id, self.helper_limits(id));
        let rate = self.helper_rate(id);
        self.helper_started.insert(id, rate);
        rate
    }
}

/// What one side (torrents, or file downloads) may take of the global cap (bytes/s) while the
/// other uses `other`: half of it at least, and what the other leaves when that is more; never
/// under 1 KB/s (the limiter takes 0 as no limit).
///
/// A video's or a gallery's speed limit as it starts (#708): the lower of `one` (its own limit,
/// else the one for one file) and an even share of `global` (the limit for all downloads as file
/// downloads have it now) with the `others` running; never under 1 KB/s.
pub(super) fn helper_rate(one: Option<u64>, global: Option<u64>, others: u64) -> Option<u64> {
    let share = global.map(|g| (g / (others + 1)).max(1024));
    match (one, share) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// An other side within 15% of its half counts as using its half: measured speeds wobble, and
/// taking every dip as room to grow kept both sides over their halves (measured: 463 + 398
/// KB/s under an 800 KB/s cap).
pub(super) fn share(cap: u64, other: f64) -> u64 {
    let half = cap / 2;
    let other = other.max(0.0);
    let rest = if other >= half as f64 * 0.85 {
        half
    } else {
        cap.saturating_sub(other as u64)
    };
    rest.max(half).max(1024)
}
