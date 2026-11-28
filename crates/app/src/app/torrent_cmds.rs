//! Torrents and magnet links: Add torrent, the session's settings and accounting, torrents through
//! a debrid account, a paired device's torrents, the Files tab's choices and torrent actions.

use super::*;

/// A torrent's folder, looked at beside the window (#743).
pub(super) enum TorrentChecked {
    /// A torrent a paired device sent (#141), to add: `there`, files of its names are in its
    /// folder already (DDM never writes into them unasked: it isn't added); else whether the
    /// folder holds others' files (`torrent_preexisting`).
    Sent {
        add: Box<crate::torrentdlg::TorrentAdd>,
        there: bool,
        preexisting: bool,
    },
    /// Torrent `id`, added already: whether its folder held files before.
    Added { id: u64, preexisting: bool },
    /// A torrent an RSS rule found (#39), to add: `there`, files of its names are in its folder
    /// already (not added, as for a paired device's). The rule's name and feed for the notes.
    Rss {
        add: Box<crate::torrentdlg::TorrentAdd>,
        there: bool,
        rule: String,
        feed: u64,
        title: String,
    },
}

impl App {
    /// Every 2 s (#40): Torrent settings for the next torrent session, and the running one stopped
    /// once no torrent has needed it for a minute (none running or sharing, no Add torrent open),
    /// when Torrent settings say so or their network settings changed.
    /// What each torrent sent and how long it shared, added to its `TorrentSpec` (librqbit
    /// counts only the current run); and the own upload limit of those sharing without a `Run`
    /// (a `Run` sets its own). Saved with the list at most every 10 s.
    pub(super) fn torrent_accounts(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.account_at).as_secs();
        // Whole seconds only: the rest carries to the next tick.
        if dt > 0 {
            self.account_at += Duration::from_secs(dt);
        }
        let Some(t) = self.torrent_host.get() else {
            // No session: no check is running, whatever the last numbers said.
            self.torrent_resync.clear();
            for i in self.items.iter_mut() {
                i.torrent_check = None;
            }
            crate::torrents::clear_live(self.items.iter_mut().filter_map(|i| i.torrent.as_mut()));
            return;
        };
        let limits = self.settings.sharing_limits();
        let mut changed = false;
        let mut hashes = std::collections::HashSet::new();
        for i in self.items.iter_mut() {
            let Some(spec) = i.torrent.as_mut() else {
                continue;
            };
            hashes.insert(spec.hash.clone());
            // As last read: the window's thread never waits on librqbit (#693).
            let Some(st) = t.seen(&spec.hash) else {
                // Out of the session (released when it finished): no check is running.
                i.torrent_check = None;
                spec.peers = 0;
                spec.up_speed = 0.0;
                continue;
            };
            // Keep the session's in-memory checking progress visible for stopped/complete
            // torrents too; normal download progress is restored when initialization ends.
            i.torrent_check = (st.phase == mdm_torrent::Phase::Checking)
                .then_some((st.checked.unwrap_or(st.done), st.total));
            let sent = self.accountant.sent(&spec.hash, st.uploaded);
            if sent > 0 {
                spec.uploaded += sent;
                changed = true;
            }
            // The peers it has now, for the list's "Peers" column (#357). Not a reason to save:
            // it changes as peers come and go, and the next tick brings the list up to date.
            spec.peers = st.peers_live;
            spec.up_speed = st.up_speed;
            if st.phase == mdm_torrent::Phase::Finished && dt > 0 {
                spec.seeded_secs += dt;
                changed = true;
            }
            // Torrent settings' seeding limits: a finished torrent over one stops being shared
            // (paused; it stays so after a restart, `complete_torrents`), with or without a Run.
            let mut stopped = None;
            if st.phase == mdm_torrent::Phase::Finished
                && let Some(why) = crate::torrents::sharing_limit_reached(spec, st.done, limits)
            {
                let (t2, hash) = (t.clone(), spec.hash.clone());
                self.rt.spawn(async move {
                    let _ = t2.remove(&hash, false).await;
                });
                stopped = Some(why);
                changed = true;
            }
            let (hash, up) = (spec.hash.clone(), spec.upload_limit);
            let stop_after_recheck = !self.settings.torrent_keep_sharing
                || crate::torrents::sharing_limit_reached(spec, st.done, limits).is_some();
            if i.live.is_none() {
                let _ = t.set_torrent_limits(&hash, None, up);
            }
            if let Some(why) = stopped {
                i.note(sharing_stopped_note(why));
            }
            // After a Recheck or a change of files (`torrent_outcomes`), once the session has
            // checked it: the list's numbers are the session's. One not running in the list
            // takes its standing from them; one no longer complete is held paused (shared, it
            // would fetch what is missing out of sight).
            if st.phase != mdm_torrent::Phase::Checking
                && !self.torrent_host.busy(&hash)
                && let Some(report) = self.torrent_resync.remove(&hash)
            {
                if report {
                    i.note(crate::i18n::fmt(
                        // i18n: event log entry after Recheck: {have} and {total} are sizes ("1.2 GB of 4 GB there")
                        crate::i18n::t("Checked: {have} of {total} there"),
                        &[
                            ("have", &model::size(st.done)),
                            ("total", &model::size(st.total)),
                        ],
                    ));
                }
                if i.live.is_none() {
                    i.downloaded = st.done;
                    i.size = Some(st.total);
                    let whole = st.total > 0 && st.done >= st.total;
                    if whole {
                        i.status = Status::Complete;
                        if report && i.live.is_none() && stop_after_recheck {
                            let t2 = t.clone();
                            self.rt.spawn(async move {
                                let _ = t2.remove(&hash, false).await;
                            });
                        }
                    } else if i.status == Status::Complete {
                        i.status = Status::Stopped;
                        let t2 = t.clone();
                        self.rt.spawn(async move {
                            let _ = t2.pause(&hash).await;
                        });
                    }
                }
                self.dirty = true;
            }
        }
        self.accountant.keep(&hashes);
        if changed && self.account_saved.elapsed() >= Duration::from_secs(10) {
            self.account_saved = now;
            self.dirty = true;
        }
    }

    pub(super) fn torrent_tick(&mut self) {
        let cfg = crate::torrents::state_dir().and_then(|d| self.settings.torrent_config(d));
        self.torrent_host.set_config(cfg);
        self.torrent_host
            .set_upload(self.settings.torrent_upload_limit());
        self.torrent_outcomes();
        self.torrent_accounts();
        self.rss_tick();
        if !self.settings.torrent_blocklist.trim().is_empty() {
            crate::torrents::refresh_blocklist(
                &self.rt,
                &self.settings.torrent_blocklist,
                &self.blocklist_busy,
            );
        }
        let sharing = self.settings.torrent_keep_sharing
            && self
                .items
                .iter()
                .any(|i| i.torrent.is_some() && i.status == Status::Complete);
        // A torrent being checked (a Recheck of one that isn't running, its action still going,
        // or its result not read yet) needs the session too: stopped as idle after a minute,
        // the check died where it was and the list said "Checking 17%" for good.
        let checking = !self.torrent_resync.is_empty()
            || self.items.iter().any(|i| {
                i.torrent_check.is_some()
                    || i.torrent
                        .as_ref()
                        .is_some_and(|t| self.torrent_host.busy(&t.hash))
            });
        let busy = sharing
            || checking
            || self.torrent_dlg.is_some()
            || self
                .items
                .iter()
                .any(|i| i.torrent.is_some() && i.live.is_some());
        if busy {
            self.torrent_idle_since = None;
            return;
        }
        let since = *self.torrent_idle_since.get_or_insert_with(Instant::now);
        if since.elapsed() >= Duration::from_secs(60) {
            self.torrent_host
                .stop_if_idle(&self.rt, true, self.settings.torrent_stop_idle);
        }
    }

    /// The info hashes of every torrent in the list (also used to prevent duplicate adds).
    pub(super) fn torrent_hashes(&self) -> std::collections::HashSet<String> {
        self.items
            .iter()
            .filter_map(|i| i.torrent.as_ref().map(|t| t.hash.clone()))
            .collect()
    }

    /// The hashes librqbit should keep or restore in its session.
    pub(super) fn session_hashes(&self) -> std::collections::HashSet<String> {
        let limits = self.settings.sharing_limits();
        self.items
            .iter()
            .filter(|i| {
                i.status != Status::Complete
                    || (self.settings.torrent_keep_sharing
                        && i.torrent.as_ref().is_some_and(|spec| {
                            crate::torrents::sharing_limit_reached(spec, i.downloaded, limits)
                                .is_none()
                        }))
            })
            .filter_map(|i| i.torrent.as_ref().map(|t| t.hash.clone()))
            .collect()
    }

    /// The list's finished torrents (what "keep sharing" shares).
    /// The finished torrents to share (Torrent settings' "Keep sharing"): those that haven't
    /// reached a seeding limit.
    pub(super) fn complete_torrents(&self) -> Vec<String> {
        let limits = self.settings.sharing_limits();
        self.items
            .iter()
            .filter(|i| i.status == Status::Complete)
            .filter_map(|i| i.torrent.as_ref().map(|t| (t, i.downloaded)))
            .filter(|(t, done)| crate::torrents::sharing_limit_reached(t, *done, limits).is_none())
            .map(|(t, _)| t.hash.clone())
            .collect()
    }

    pub(super) fn open_torrent_dialog(&mut self, source: Option<String>, ctx: &egui::Context) {
        self.open_torrent_dialog_with(source, None, ctx);
    }

    /// Add torrent for a .torrent on the web (#39): fetched with `remote`'s request. A magnet
    /// link goes straight to the debrid account when torrents go through it (#44): Add torrent
    /// would ask peers for what it holds.
    pub(super) fn open_torrent_dialog_with(
        &mut self,
        source: Option<String>,
        remote: Option<crate::torrents::Remote>,
        ctx: &egui::Context,
    ) {
        if remote.is_none()
            && self.torrent_account().is_some()
            && let Some(magnet) = source
                .as_deref()
                .and_then(mdm_torrent::Source::magnet)
                .and_then(|s| match s {
                    mdm_torrent::Source::Magnet(m) => url::Url::parse(&m).ok(),
                    mdm_torrent::Source::File(_) => None,
                })
        {
            let dir = self
                .settings
                .category_dir(Category::Torrents, &self.downloads);
            self.add_remote_torrent(magnet, dir, Category::Torrents, true, None);
            self.show_window(ctx);
            return;
        }
        if let Some(open) = &self.torrent_dlg {
            // One Add torrent at a time: a torrent handed over meanwhile waits its turn (#507).
            // Add torrent… from the menu with it open (no source) has nothing to wait for.
            if let Some(source) = source
                && open.source() != source
                && !self.torrent_waiting.iter().any(|(s, _)| *s == source)
            {
                tracing::info!(
                    "Add torrent is open: a torrent waits ({} waiting)",
                    self.torrent_waiting.len() + 1
                );
                self.torrent_waiting.push_back((source, remote));
            }
            return;
        }
        self.torrent_dlg = Some(crate::torrentdlg::TorrentDialog::new(
            source,
            remote,
            self.torrent_host.clone(),
            self.session_hashes(),
            self.downloads.clone(),
            self.settings.category_dirs.clone(),
            &self.rt,
            ctx,
        ));
    }

    /// The debrid account torrents go to (#44): Options → Accounts' "Torrents and magnet links
    /// through the first account", with an account there.
    pub(super) fn torrent_account(&self) -> Option<(u64, mdm_debrid::ServiceKind)> {
        (self.settings.debrid_on && self.settings.debrid_torrents)
            .then(|| self.debrid.torrent_account())
            .flatten()
    }

    /// A torrent through the debrid account (#44): a row whose files come over HTTP once the
    /// service has them, into `dir` (a torrent of several files in a folder of its own there).
    /// `preexisting`: whether its folder already held files, when that was asked beside the
    /// window before; else it is asked there now (`TorrentChecked::Added`, #743).
    pub(super) fn add_remote_torrent(
        &mut self,
        magnet: url::Url,
        dir: PathBuf,
        category: Category,
        start_now: bool,
        preexisting: Option<bool>,
    ) {
        let Some((account, service)) = self.torrent_account() else {
            return;
        };
        let target = dir.join(remote_torrent_name(&magnet));
        let id = self.push_item(
            magnet,
            target.clone(),
            category,
            String::new(),
            None,
            None,
            Default::default(),
            crate::i18n::mark("from a magnet link"),
        );
        if preexisting.is_none() {
            self.folder_taken_beside(id, target);
        }
        if let Some(item) = self.item(id) {
            item.preexisting_folder = preexisting.unwrap_or(false);
            item.note(crate::i18n::fmt(
                // i18n: event log: a torrent goes through the user's debrid account, not to peers; {service} its name ("Real-Debrid")
                crate::i18n::t(
                    "Through {service}: the service downloads the torrent, then DDM its files",
                ),
                &[("service", service.name())],
            ));
            item.remote = Some(model::RemoteSpec {
                account,
                transfer: None,
                folder: dir,
            });
        }
        self.selection.set(id);
        if start_now {
            self.start(id, true);
        } else {
            self.queue_later(vec![id], self.settings.queue_panel_later);
        }
        self.dirty = true;
    }

    /// Start (or continue) a torrent through its debrid account (#44).
    pub(super) fn start_remote(&mut self, id: u64, show: bool) {
        let limit = self.settings.speed_limit();
        let Some((url, spec, own)) = self
            .item(id)
            .and_then(|i| Some((i.url.clone(), i.remote.clone()?, i.own_limit)))
        else {
            return;
        };
        // The files' requests: Options' connections, proxy, DNS and adapter; the service's own
        // unlocking is the torrent job's (not a host's link).
        let mut net = self
            .request_parts(&url, None, &Default::default(), true)
            .opts;
        net.speed_limit = own.or(limit);
        let rt = self.rt.clone();
        let _guard = rt.enter();
        let job = mdm_debrid::torrents::RemoteTorrent::start(
            &self.debrid,
            mdm_debrid::torrents::Order {
                account: spec.account,
                magnet: url,
                transfer: spec.transfer.clone(),
                folder: spec.folder.clone(),
                net,
                poll: std::time::Duration::from_secs(5),
            },
        );
        let Some(item) = self.item(id) else { return };
        item.note(if spec.transfer.is_some() {
            crate::i18n::t("Resumed")
        } else {
            crate::i18n::t("Started")
        });
        item.live = Some(Live::new(model::Job::Remote(job), item.downloaded));
        item.status = Status::Downloading;
        item.error = None;
        item.last_try = Some(model::now_stamp());
        self.dirty = true;
        if show {
            self.open_started_progress(id);
        }
    }

    /// Add torrent → Download Now / Later. Through the debrid account when torrents go there
    /// (#44): the .torrent's magnet link (its hash, name and trackers), all its files.
    pub(super) fn add_torrent(&mut self, add: crate::torrentdlg::TorrentAdd) {
        self.add_torrent_with(add, None);
    }

    /// [`Self::add_torrent`]; `preexisting`: whether its folder already holds files of others
    /// (`torrent_preexisting`), when that was asked beside the window (#743), else asked now.
    pub(super) fn add_torrent_with(
        &mut self,
        add: crate::torrentdlg::TorrentAdd,
        preexisting: Option<bool>,
    ) {
        if self.torrent_account().is_some()
            && let Some(magnet) = remote_torrent_magnet(&add.meta)
        {
            self.add_remote_torrent(
                magnet,
                add.dir.clone(),
                add.category,
                add.start_now,
                preexisting,
            );
            return;
        }
        let meta = &add.meta;
        if let Err(e) = crate::torrents::save_torrent(&meta.info_hash, &meta.torrent) {
            self.notice = Some((
                crate::i18n::mark("Add torrent"),
                crate::i18n::fmt(
                    crate::i18n::t("The torrent could not be saved: {error}"),
                    &[("error", &e.to_string())],
                ),
            ));
            return;
        }
        // Its first start asks the peers its file list came from first.
        self.torrent_host.met_peers(&meta.info_hash, &meta.peers);
        let folder = meta.folder_in(&add.dir);
        let dest = crate::torrents::dest_for(meta, &add.dir, add.files.as_deref());
        let preexisting = preexisting.unwrap_or_else(|| torrent_preexisting(&add, false));
        let size = meta
            .files
            .iter()
            .enumerate()
            .filter(|(i, f)| !f.padding && add.files.as_ref().is_none_or(|c| c.contains(i)))
            .map(|(_, f)| f.size)
            .sum();
        let id = self.push_item(
            crate::torrents::magnet_url(&meta.info_hash, &meta.name),
            dest,
            add.category,
            String::new(),
            Some(size),
            None,
            Default::default(),
            if add.from_magnet {
                crate::i18n::mark("from a magnet link")
            } else {
                crate::i18n::mark("from a torrent file")
            },
        );
        if let Some(item) = self.item(id) {
            item.preexisting_folder = preexisting;
            item.torrent = Some(crate::torrents::TorrentSpec {
                hash: meta.info_hash.clone(),
                files: add.files.clone(),
                folder,
                uploaded: 0,
                seeded_secs: 0,
                peers: 0,
                up_speed: 0.0,
                upload_limit: None,
                in_order: add.in_order,
                ends_first: true,
            });
        }
        self.selection.set(id);
        if add.start_now {
            self.start(id, !add.quiet);
        } else if let Some(q) = add.queue.filter(|q| *q < self.queues.len()) {
            self.queues[q].items.push(id);
        } else {
            self.queue_later(vec![id], self.settings.queue_panel_later);
        }
        self.dirty = true;
    }

    /// Right-click → Torrent → Download in order (`in_order`), or its first and last pieces
    /// first (#39): kept with the torrent, and applied now if it runs (the next piece each peer
    /// is asked for follows it).
    pub(super) fn toggle_torrent_order(&mut self, id: u64, in_order: bool) {
        let Some(item) = self.item(id) else { return };
        let Some(spec) = item.torrent.as_mut() else {
            return;
        };
        if in_order {
            spec.in_order = !spec.in_order;
        } else {
            spec.ends_first = !spec.ends_first;
        }
        let order = spec.order();
        if let Some(model::Job::Torrent(run)) = item.live.as_ref().map(|l| &l.dl) {
            run.set_order(order);
        }
        self.dirty = true;
    }

    /// Start a torrent: the session adds or resumes it.
    pub(super) fn start_torrent(&mut self, id: u64, show: bool) {
        let auto_share = self.settings.torrent_keep_sharing;
        let known = self.session_hashes();
        let limit = self.settings.speed_limit();
        let (host, rt) = (self.torrent_host.clone(), self.rt.clone());
        let Some(item) = self.item(id) else { return };
        // An explicit Start on a completed download means seed it, even when automatic
        // keep sharing is off. It remains in the session until Stop or a sharing limit.
        let share = auto_share
            || item
                .size
                .is_some_and(|size| size > 0 && item.downloaded >= size);
        // Its own remembered limit first, as a file download's (#39).
        let limit = item.own_limit.or(limit);
        let Some(spec) = item.torrent.clone() else {
            return;
        };
        let run =
            crate::torrents::Run::start(&host, &rt, &spec, item.url.clone(), known, limit, share);
        item.note(if item.downloaded > 0 {
            // i18n: event log entry, past tense: the download resumed (just now)
            crate::i18n::t("Resumed")
        } else {
            // i18n: event log entry, past tense: the download started (just now)
            crate::i18n::t("Started")
        });
        item.live = Some(Live::new(model::Job::Torrent(run), item.downloaded));
        item.status = Status::Downloading;
        item.error = None;
        item.last_try = Some(model::now_stamp());
        self.dirty = true;
        if show {
            self.open_started_progress(id);
        }
    }

    pub(super) fn torrent_act(&mut self, id: u64, act: crate::torrents::Act) {
        let known = self.session_hashes();
        let Some(spec) = self
            .items
            .iter()
            .find(|i| i.id == id)
            .and_then(|i| i.torrent.clone())
        else {
            return;
        };
        if self.torrent_host.busy(&spec.hash) {
            return;
        }
        self.torrent_host.act(&self.rt, known, &spec, act);
    }

    /// Files ticked or unticked in a torrent's Files tab (#39): the torrent downloads `files`
    /// from now on (`all`: every one), `size` together. Kept with it (`TorrentSpec::files`); the
    /// session changes now. One that isn't running in the list and gains files is held paused
    /// (Resume fetches them): nothing downloads out of sight.
    pub(super) fn choose_files(&mut self, id: u64, files: Vec<usize>, all: bool, size: u64) {
        let Some(item) = self.item(id) else { return };
        let live = item.live.is_some();
        let Some(spec) = item.torrent.as_mut() else {
            return;
        };
        let before: Option<Vec<usize>> = spec.files.clone();
        let gains = match &before {
            None => false,
            Some(had) => files.iter().any(|f| !had.contains(f)),
        };
        spec.files = if all { None } else { Some(files.clone()) };
        if spec.files == before {
            return;
        }
        item.size = Some(size);
        let n = files.len() as u64;
        // i18n: event log entry: the files ticked in a torrent's Files tab changed; {n} of them are ticked now
        let note = crate::i18n::tn(
            n,
            "{n} file chosen to download",
            "{n} files chosen to download",
        );
        item.note(note);
        self.dirty = true;
        self.torrent_act(
            id,
            crate::torrents::Act::Choose {
                files,
                pause: !live && gains,
            },
        );
    }

    /// The torrent actions done since the last tick, in their downloads' event logs; after a
    /// Recheck or a change of files, the list's numbers follow the session (`torrent_resync`).
    pub(super) fn torrent_outcomes(&mut self) {
        use crate::torrents::{Act, Done};
        for o in self.torrent_host.take_outcomes() {
            let Some(item) = self
                .items
                .iter_mut()
                .find(|i| i.torrent.as_ref().is_some_and(|t| t.hash == o.hash))
            else {
                continue;
            };
            let note = match (&o.act, o.res) {
                (_, Ok(Done::Rechecking)) => {
                    self.torrent_resync.insert(o.hash.clone(), true);
                    // i18n: event log entry: a torrent's files are being read and checked again (Recheck)
                    crate::i18n::t("Checking every piece again").to_owned()
                }
                (_, Ok(Done::Reannounced)) => {
                    // i18n: event log entry: the torrent's trackers were asked for peers again (Reannounce)
                    crate::i18n::t("Asked the trackers for peers again").to_owned()
                }
                (_, Ok(Done::TrackersAdded(0))) => {
                    // i18n: event log entry: Add trackers changed nothing, the torrent had them all
                    crate::i18n::t("No tracker added: it has them already").to_owned()
                }
                (_, Ok(Done::TrackersAdded(n))) => {
                    // i18n: event log entry: trackers were added to the torrent
                    crate::i18n::tn(n as u64, "{n} tracker added", "{n} trackers added")
                }
                (_, Ok(Done::Chosen)) => {
                    self.torrent_resync.insert(o.hash.clone(), false);
                    continue;
                }
                (act, Err(e)) => crate::i18n::fmt(
                    match act {
                        // i18n: event log entry: Recheck failed, with the reason
                        Act::Recheck => crate::i18n::t("Recheck failed: {error}"),
                        // i18n: event log entry: Reannounce failed, with the reason
                        Act::Reannounce => crate::i18n::t("Reannounce failed: {error}"),
                        // i18n: event log entry: Add trackers failed, with the reason
                        Act::AddTrackers(_) => {
                            crate::i18n::t("The trackers could not be added: {error}")
                        }
                        // i18n: event log entry: the files chosen in the Files tab could not be applied, with the reason
                        Act::Choose { .. } => {
                            crate::i18n::t("The files could not be chosen: {error}")
                        }
                    },
                    &[("error", &e)],
                ),
            };
            item.note(note);
            self.dirty = true;
        }
    }

    /// The torrents a paired device sent whose files are known now: added with every file
    /// (into the category's folder), as an RSS rule adds one; never over files already there.
    pub(super) fn poll_remote_torrents(&mut self) {
        let mut ready = Vec::new();
        self.remote_torrents
            .retain(|l| match l.slot.lock().unwrap().take() {
                Some(res) => {
                    ready.push((res, l.category, l.queue, l.from_magnet));
                    false
                }
                None => true,
            });
        for (res, category, queue, from_magnet) in ready {
            let meta = match res {
                Ok(meta) => meta,
                Err(e) => {
                    tracing::warn!("remote control: a torrent could not be read: {e}");
                    continue;
                }
            };
            if self.torrent_hashes().contains(&meta.info_hash) {
                continue;
            }
            let category = category.unwrap_or_else(|| crate::torrentdlg::category_of(&meta));
            let dir = crate::model::category_folder(
                category,
                &self.downloads,
                &self.settings.category_dirs,
            );
            // Its folder is looked at beside the window (#743), then it is added
            // (`poll_torrent_checks`).
            let add = crate::torrentdlg::TorrentAdd {
                meta,
                dir,
                files: None,
                category,
                start_now: queue.is_none(),
                from_magnet,
                in_order: false,
                queue,
                quiet: true,
            };
            let through_account = self.torrent_account().is_some();
            self.check_torrent_beside(move || {
                let there = !add.meta.existing_in(&add.dir).is_empty();
                let preexisting = !there && torrent_preexisting(&add, through_account);
                TorrentChecked::Sent {
                    add: Box::new(add),
                    there,
                    preexisting,
                }
            });
        }
    }

    /// Ask `check` about a torrent's folder on a thread other than the window's (#743): a
    /// sleeping hard disk, a USB drive or a network folder can take seconds to answer. The
    /// answer comes to `poll_torrent_checks`.
    pub(super) fn check_torrent_beside(
        &mut self,
        check: impl FnOnce() -> TorrentChecked + Send + 'static,
    ) {
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = self.ui_ctx.clone();
        self.rt.spawn_blocking(move || {
            let _ = tx.send(check());
            ctx.request_repaint();
        });
        self.torrent_checks.push(rx);
    }

    /// Whether torrent `id`'s place `folder` already held files as it was added
    /// (`Item::preexisting_folder`), asked beside the window (#743).
    pub(super) fn folder_taken_beside(&mut self, id: u64, folder: PathBuf) {
        self.check_torrent_beside(move || TorrentChecked::Added {
            id,
            preexisting: crate::deferred_rename::folder_has_any_entries(&folder),
        });
    }

    /// The torrents' folders looked at: a paired device's torrent is added now (never over
    /// files of its names already there), and a torrent added already learns whether its
    /// folder held files before, while nothing of its own has come into it.
    pub(super) fn poll_torrent_checks(&mut self) {
        let mut done = Vec::new();
        self.torrent_checks.retain(|rx| match rx.try_recv() {
            Ok(c) => {
                done.push(c);
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(_) => false,
        });
        for c in done {
            match c {
                TorrentChecked::Sent {
                    add,
                    there,
                    preexisting,
                } => {
                    if there {
                        tracing::warn!(
                            "remote control: {}: its files are already in {}",
                            add.meta.name,
                            add.dir.display()
                        );
                        continue;
                    }
                    // Sent twice, or added another way meanwhile.
                    if self.torrent_hashes().contains(&add.meta.info_hash) {
                        continue;
                    }
                    self.add_torrent_with(*add, Some(preexisting));
                }
                TorrentChecked::Added { id, preexisting } => {
                    if preexisting
                        && let Some(item) = self.item(id)
                        && item.downloaded == 0
                        && item.written_files.is_none()
                    {
                        item.preexisting_folder = true;
                        self.dirty = true;
                    }
                }
                TorrentChecked::Rss {
                    add,
                    there,
                    rule,
                    feed,
                    title,
                } => self.rss_checked(*add, there, &rule, feed, &title),
            }
        }
    }

    /// Play while downloading (the list's right-click menu, a torrent's progress window): its
    /// first playable file in the browser's player, the torrent switched to its pieces in order,
    /// each file's first and last first (what a player needs to start and to know the length).
    pub(super) fn play_torrent(&mut self, id: u64) {
        let Some((spec, name)) = self
            .items
            .iter()
            .find(|i| i.id == id)
            .and_then(|i| i.torrent.clone().map(|s| (s, i.file_name.clone())))
        else {
            return;
        };
        let Some(t) = self.torrent_host.get() else {
            return;
        };
        let Some((file, file_name)) = player::playable_file(&t, &spec.hash) else {
            return;
        };
        if let Err(e) = t.set_order(
            &spec.hash,
            mdm_torrent::PieceOrder::InOrder {
                first_and_last_first: true,
            },
        ) {
            tracing::warn!(torrent = %name, "could not prioritize playback pieces: {e}");
        }
        if let Err(e) = player::open(&self.rt, t, spec.hash.clone(), file, file_name) {
            tracing::warn!(torrent = %name, "could not open torrent player: {e}");
            self.notice = Some((
                // i18n: title of the message when a torrent's player couldn't open (Right-click → Play while downloading)
                crate::i18n::mark("Play while downloading"),
                crate::i18n::fmt(
                    // i18n: message when a torrent's player couldn't open, with why
                    crate::i18n::t("The player couldn't open: {error}"),
                    &[("error", &e)],
                ),
            ));
        }
    }
}

/// The magnet link a torrent goes to a debrid account with (#44): its hash, name and trackers.
pub(super) fn remote_torrent_magnet(meta: &mdm_torrent::Meta) -> Option<url::Url> {
    mdm_torrent::magnet_link(&meta.torrent).and_then(|m| url::Url::parse(&m).ok())
}

/// The name of a torrent's place in its folder through a debrid account (#44): the magnet's.
pub(super) fn remote_torrent_name(magnet: &url::Url) -> String {
    magnet
        .query_pairs()
        .find(|(k, _)| k == "dn")
        .map(|(_, v)| mdm_engine::naming::sanitize(&v))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "torrent".to_owned())
}

/// Whether the folder torrent `add` is about to go into already holds files that aren't its
/// own (`Item::preexisting_folder`: a deferred rename then leaves the folder's name alone).
/// `through_account`: it goes to the debrid account (#44), into the place its magnet names.
/// Looks at the disk: for a thread other than the window's when it may wait (#743).
pub(super) fn torrent_preexisting(
    add: &crate::torrentdlg::TorrentAdd,
    through_account: bool,
) -> bool {
    if through_account && let Some(magnet) = remote_torrent_magnet(&add.meta) {
        let target = add.dir.join(remote_torrent_name(&magnet));
        return crate::deferred_rename::folder_has_any_entries(&target);
    }
    let meta = &add.meta;
    let folder = meta.folder_in(&add.dir);
    let dest = crate::torrents::dest_for(meta, &add.dir, add.files.as_deref());
    if dest != folder || !crate::deferred_rename::folder_has_any_entries(&folder) {
        return false;
    }
    let allowed: Vec<PathBuf> = meta
        .files
        .iter()
        .enumerate()
        .filter(|(i, f)| f.padding || add.files.as_ref().is_none_or(|chosen| chosen.contains(i)))
        .map(|(_, f)| folder.join(&f.path))
        .collect();
    crate::deferred_rename::folder_has_untracked_files(&folder, &allowed)
}

/// The event log's line when a seeding limit stops a torrent's sharing.
pub(super) fn sharing_stopped_note(why: crate::torrents::Reached) -> String {
    match why {
        crate::torrents::Reached::Ratio(r) => crate::i18n::fmt(
            // i18n: event log entry, past tense: a finished torrent stopped being shared to others because its share ratio (sent ÷ received) reached the limit set in Torrent settings; {ratio} = e.g. 2.00
            crate::i18n::t("Sharing stopped: the share ratio reached {ratio}"),
            &[("ratio", &crate::i18n::decimal(r, 2))],
        ),
        crate::torrents::Reached::Time(s) => crate::i18n::fmt(
            // i18n: event log entry, past tense: a finished torrent stopped being shared to others after the time set in Torrent settings; {time} = e.g. 1 hour(s) 15 min
            crate::i18n::t("Sharing stopped after sharing for {time}"),
            &[("time", &model::time(s as f64))],
        ),
    }
}
