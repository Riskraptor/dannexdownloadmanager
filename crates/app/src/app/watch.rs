//! What DDM keeps an eye on beside the downloads: the network adapter and the route connections
//! take, debrid accounts, downloads to resume after a network failure, an assistive technology, the
//! clipboard and new versions.

use super::*;

impl App {
    /// Each frame (#35): the route connections take, from the adapter watch. When it changes,
    /// everything running stops (its connections were held to the route before, or to none;
    /// sockets that are bound can't leak, this is so they don't wait for a timeout) and the
    /// torrent session with it; what was stopped so starts again once the route lets it.
    /// Each frame (#44): the debrid accounts set up again when they, or DDM's network settings
    /// (the API calls take the same proxy, DNS and adapter), changed; then the services' host
    /// lists read, off the window's thread. No account: no request at all.
    pub(super) fn debrid_tick(&mut self) {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for a in self.settings.debrid_accounts.iter().filter(|a| a.enabled) {
            (a.id, a.service, &a.secret).hash(&mut h);
        }
        format!("{:?}", crate::settings::own_net().proxy).hash(&mut h);
        self.settings.dns().hash(&mut h);
        self.route.hash(&mut h);
        let from = h.finish();
        if from == self.debrid_from {
            return;
        }
        self.debrid_from = from;
        let accounts: Vec<mdm_debrid::Account> = self
            .settings
            .debrid_accounts
            .iter()
            .filter(|a| a.enabled)
            .filter_map(crate::settings::DebridAccount::account)
            .collect();
        let Ok(http) = crate::debrid::http() else {
            return;
        };
        let any = !accounts.is_empty();
        self.debrid.set_accounts(accounts, http);
        if any {
            let debrid = self.debrid.clone();
            self.rt.spawn(async move {
                if let Err(e) = debrid.refresh_hosts().await {
                    tracing::warn!("debrid host lists: {e:?}");
                }
            });
        }
    }

    /// Which account downloads go through (#44), for the windows that show and change it.
    pub fn debrid_route(&self) -> crate::debrid::Route {
        crate::debrid::Route::new(&self.debrid, &self.settings)
    }

    pub(super) fn network_adapter_tick(&mut self, ctx: &egui::Context) {
        let route = self.netwatch.route(&self.settings.net_adapter, ctx);
        crate::settings::share_route(route.clone());
        if route != self.route {
            let before = std::mem::replace(&mut self.route, route.clone());
            self.route_changed(&before);
        }
        if matches!(self.route, mdm_engine::Route::Down { .. }) || self.adapter_paused.is_empty() {
            return;
        }
        // Started again once their jobs have ended.
        let ready: Vec<u64> = self
            .adapter_paused
            .iter()
            .copied()
            .filter(|id| self.items.iter().any(|i| i.id == *id && i.live.is_none()))
            .collect();
        self.adapter_paused
            .retain(|id| self.items.iter().any(|i| i.id == *id) && !ready.contains(id));
        for id in ready {
            self.start(id, false);
        }
    }

    /// A download that failed because the network did starts again by itself (beyond IDM, which
    /// leaves it failed): one a running queue holds is the queue's to retry.
    pub(super) fn schedule_auto_resume(&mut self, id: u64, now: Instant) {
        let in_queue = self
            .queues
            .iter()
            .any(|q| q.running && q.items.contains(&id));
        let Some(item) = self.items.iter_mut().find(|i| i.id == id) else {
            return;
        };
        let network = item
            .snap
            .as_ref()
            .and_then(|s| s.failure.as_ref())
            .is_some_and(mdm_engine::Error::network_lost);
        if !network || in_queue || self.stopped_by_hand.contains(&id) {
            self.auto_resume.forget(id);
            return;
        }
        match self.auto_resume.failed(id, item.downloaded, now) {
            Some(wait) => item.note(crate::i18n::fmt(
                // i18n: event log entry: the download failed because the network did (no connection, one gone quiet); it starts again by itself after {seconds} seconds
                crate::i18n::t("The network failed; trying again in {seconds} s"),
                &[("seconds", &wait.as_secs().to_string())],
            )),
            None => item.note(
                // i18n: event log entry: the download kept failing on the network for about an hour and is no longer started again by itself
                crate::i18n::t("The network failed too long; no longer trying by itself"),
            ),
        }
        self.dirty = true;
    }

    /// Starts the downloads [`Self::schedule_auto_resume`] waits for, once their time comes and
    /// the network is there.
    pub(super) fn auto_resume_tick(&mut self, now: Instant, ctx: &egui::Context) {
        if !self.auto_resume.is_waiting() {
            return;
        }
        ctx.request_repaint_after(Duration::from_secs(1));
        if matches!(self.route, mdm_engine::Route::Down { .. }) {
            return;
        }
        for id in self.auto_resume.due(now) {
            let waiting = self
                .items
                .iter()
                .any(|i| i.id == id && i.status == Status::Error && i.live.is_none());
            if !waiting || self.stopped_by_hand.contains(&id) {
                self.auto_resume.forget(id);
                continue;
            }
            self.start(id, false);
        }
    }

    pub(super) fn route_changed(&mut self, before: &mdm_engine::Route) {
        let note = match &self.route {
            mdm_engine::Route::Down { label } => crate::i18n::fmt(
                // i18n: event log entry: the download stopped because the chosen network adapter (a VPN's, usually) went away; {adapter} its name
                crate::i18n::t("Stopped: {adapter} is not connected; it goes on when it is back"),
                &[("adapter", label)],
            ),
            // i18n: event log entry: the download stopped to start again through the network adapter as it is now (a VPN reconnected, another adapter chosen)
            _ => crate::i18n::t("Stopped: the network adapter changed; starting again").to_owned(),
        };
        let mut stopped = Vec::new();
        for i in &mut self.items {
            if let Some(l) = &i.live {
                l.dl.cancel();
                i.note(note.clone());
                stopped.push(i.id);
            }
        }
        for id in stopped {
            if !self.adapter_paused.contains(&id) {
                self.adapter_paused.push(id);
            }
        }
        // The torrent session's sockets were bound as things were.
        if !before.is_any() || !self.route.is_any() {
            self.torrent_host.stop_now(&self.rt);
        }
        self.dirty = true;
    }

    /// While an assistive technology is listening (a screen reader running, #131), every window
    /// builds its accessibility tree with each frame, so the first time one is read it answers
    /// with its contents rather than an empty window (egui keeps the last tree for that). Asked
    /// once a second; never turned off here (the platform says when nobody listens any more).
    pub(super) fn follow_assistive_technology(&mut self, ctx: &egui::Context) {
        if self
            .assistive_checked
            .is_some_and(|t| t.elapsed() < Duration::from_secs(1))
        {
            return;
        }
        self.assistive_checked = Some(Instant::now());
        #[cfg(windows)]
        let listening = crate::win32::assistive_technology_listening();
        #[cfg(not(windows))]
        let listening = crate::unix::assistive_technology_listening();
        if listening {
            ctx.enable_accesskit();
        }
    }

    /// Options → General → "Offer to download links copied to the clipboard": a newly copied
    /// link to a listed file type (Options → File types) is handled as if a browser had sent
    /// it, so Download File Info opens with it (unless that window is turned off). Only read,
    /// never written; on by default.
    pub(super) fn watch_clipboard(&mut self, ctx: &egui::Context) {
        // Read off the window's thread (clipboard.rs): the program that owns the clipboard may
        // hang, and a read that waits for it took the window with it (#418). The reader looks
        // every second by itself while this is on and wakes the window only when the text
        // changed: an idle clipboard costs no frames (it took two a second, MDM_FRAME_STATS).
        crate::clipboard::monitor(self.settings.clipboard_autostart);
        if !self.settings.clipboard_autostart {
            self.clip_seen = None;
            self.clip_asked = None;
            return;
        }
        // The first look only notes what is there: turning it on doesn't grab an old link. It
        // is a read asked for now, not one from before it was turned on. An ask that is never
        // answered just means no look at the clipboard.
        let Some(before) = self.clip_seen.clone() else {
            match self.clip_asked {
                Some(ask) => {
                    if let Some(text) = crate::clipboard::answer(ask) {
                        self.clip_asked = None;
                        self.clip_seen = Some(text.map(|t| t.trim().to_owned()));
                    }
                }
                None => self.clip_asked = Some(crate::clipboard::ask()),
            }
            return;
        };
        let Some(text) = crate::clipboard::last_read() else {
            return;
        };
        let text = text.map(|t| t.trim().to_owned());
        if text == before {
            return;
        }
        self.clip_seen = Some(text.clone());
        let Some(text) = text else {
            return;
        };
        if clipboard_catches(&self.settings, &text) {
            let cmd = ipc::Command {
                url: Some(text),
                ..Default::default()
            };
            self.handle_command(cmd, ctx);
        }
    }

    /// IDM looks for a new version once a week; so do we (off: the switch in Check for updates). Found: the
    /// update window says so; otherwise nothing shows.
    /// The automatic update check: a few seconds after every start, then once a day while DDM
    /// runs (it was at most once a week). A new version's window comes to the front, as apps
    /// announce theirs. Not in off-screen test runs (their windows stay as asked).
    pub(super) fn auto_update_check(&mut self, ctx: &egui::Context) {
        let now = chrono::Utc::now().timestamp();
        let first = self.update_checked_this_run.is_none()
            && self.started.elapsed() >= crate::help::CHECK_AFTER_START;
        let again = self
            .update_checked_this_run
            .is_some_and(|t| now - t >= crate::help::CHECK_EVERY);
        if self.auto_update.is_none()
            && self.settings.update_check
            // Off-screen test runs check only when told where (MDM_UPDATE_FEED: their own feed).
            && (!crate::offscreen() || std::env::var_os("MDM_UPDATE_FEED").is_some())
            && (first || again)
        {
            self.update_checked_this_run = Some(now);
            self.settings.update_checked = now;
            self.dirty = true;
            self.auto_update = Some(crate::help::check(&self.rt, ctx));
        }
        if self.updates.is_some() {
            if self.raise_updates && crate::dialogs::focus_when_shown(ctx, "updates") {
                self.raise_updates = false;
            }
        } else {
            self.raise_updates = false;
        }
        if self.options.is_some() {
            if self.raise_options && crate::dialogs::focus_when_shown(ctx, "options") {
                self.raise_options = false;
            }
        } else {
            self.raise_options = false;
        }
        // Until the check at start is due.
        if self.update_checked_this_run.is_none() && self.settings.update_check {
            ctx.request_repaint_after(crate::help::CHECK_AFTER_START);
        }
        let done = self
            .auto_update
            .as_ref()
            .and_then(|s| s.lock().unwrap().take());
        if let Some(res) = done {
            self.auto_update = None;
            if let (Ok(crate::help::Found::Newer(rel)), None) = (res, &self.updates) {
                tracing::info!("update available: {}", rel.version);
                self.updates = Some(crate::help::Updates::found(rel));
                self.raise_updates = true;
            }
        }
    }
}

/// Whether the clipboard monitor takes a copied text (trimmed): an address DDM downloads of a
/// type it takes, or a magnet link (Add torrent) when torrents are among those types, as a
/// .torrent's address is.
pub(super) fn clipboard_catches(settings: &crate::settings::Settings, text: &str) -> bool {
    if mdm_torrent::Source::magnet(text).is_some() {
        return settings.clipboard_type("magnet.torrent");
    }
    let Ok(url) = url::Url::parse(text) else {
        return false;
    };
    let name = mdm_engine::naming::file_name(None, &url);
    mdm_engine::supported(&url) && settings.clipboard_type(&name)
}
