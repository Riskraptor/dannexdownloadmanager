//! The windows' answers, taken at every pass of the main window: each dialog's outcome acted on
//! (`dialogs`), the Help menu's windows (`help_dialogs`).

use super::*;

impl App {
    pub(super) fn help_dialogs(&mut self, ctx: &egui::Context) {
        if let Some(d) = &mut self.lang_picker {
            let first = !self.settings.language_asked;
            match d.show(ctx) {
                Outcome::Open => {}
                Outcome::Done(code) => {
                    // On the first start, keeping the preselected Windows language follows
                    // Windows from then on.
                    let code = code.filter(|c| !(first && *c == crate::i18n::system_code()));
                    self.lang_picker = None;
                    self.run(Cmd::Language(code), None, ctx);
                    crate::theme::interface_fonts(ctx);
                }
                Outcome::Closed => {
                    self.lang_picker = None;
                    crate::theme::interface_fonts(ctx);
                    // Cancelled on the first start: Windows' language, and don't ask again.
                    if first {
                        self.settings.language_asked = true;
                        self.dirty = true;
                    }
                }
            }
        }
        // The video helpers aren't offered: DDM gets them by itself (autohelpers.rs).
        if let Some(q) = &mut self.shown_existing
            && !matches!(q.show(ctx), Outcome::Open)
        {
            self.shown_existing = None;
        }
        // Once the language is settled: the browser extension, once per start until a browser
        // has it (extension.rs).
        if self.lang_picker.is_none() && self.settings.language_asked && !self.extension_weighed {
            self.extension_weighed = true;
            if crate::extension::worth_asking(&self.settings) {
                self.extension = Some(crate::extension::Extension::new());
            }
        }
        if let Some(d) = &mut self.extension {
            match d.show(ctx) {
                Outcome::Open => {}
                Outcome::Done(()) => {
                    self.extension = None;
                    self.settings.extension_never_ask = true;
                    self.dirty = true;
                }
                Outcome::Closed => self.extension = None,
            }
        }
        // Then, once, what's new in this version (notes.rs): its first start after an update,
        // never a first start; not in off-screen test runs (`/show whats-new` there).
        if self.lang_picker.is_none()
            && self.extension.is_none()
            && self.extension_weighed
            && !self.whats_new_weighed
        {
            self.whats_new_weighed = true;
            if self.settings.notes_seen != crate::help::VERSION {
                let due = crate::notes::whats_new_due(
                    &self.settings.notes_seen,
                    crate::help::VERSION,
                    self.ran_before,
                );
                tracing::info!(
                    "what's new in {}: {} (last seen {:?})",
                    crate::help::VERSION,
                    if due { "due" } else { "not due" },
                    self.settings.notes_seen
                );
                self.settings.notes_seen = crate::help::VERSION.to_owned();
                self.dirty = true;
                if due && !crate::offscreen() {
                    self.whats_new = crate::notes::WhatsNew::new();
                }
            }
        }
        if let Some(d) = &mut self.whats_new
            && !matches!(d.show(ctx), Outcome::Open)
        {
            self.whats_new = None;
        }
        if let Some(d) = &mut self.about
            && !matches!(d.show(ctx), Outcome::Open)
        {
            self.about = None;
        }
        let mut weekly = self.settings.update_check;
        let mut close_for_update = false;
        if let Some(d) = &mut self.updates {
            match d.show(ctx, &self.rt, &mut weekly) {
                Outcome::Open => {}
                // Update now: the new copy is going in place; DDM saves and closes for it.
                Outcome::Done(true) => {
                    self.updates = None;
                    close_for_update = true;
                }
                _ => self.updates = None,
            }
        }
        if close_for_update {
            self.run(Cmd::Exit, None, ctx);
        }
        if weekly != self.settings.update_check {
            self.settings.update_check = weekly;
            self.dirty = true;
        }
        if let Some(d) = &mut self.shortcuts
            && !matches!(d.show(ctx), Outcome::Open)
        {
            self.shortcuts = None;
        }
        if let Some(d) = &mut self.tell_friend
            && !matches!(d.show(ctx), Outcome::Open)
        {
            self.tell_friend = None;
        }
    }

    pub(super) fn dialogs(&mut self, ctx: &egui::Context) {
        self.help_dialogs(ctx);
        self.missing_dialogs(ctx);
        // The next question in turn; one about a download that has left the list meanwhile is
        // checked afresh instead (it may ask again, about another).
        while self.duplicate.is_none()
            && let Some((dup, pending, d)) = self.duplicates_waiting.pop_front()
        {
            if self.items.iter().any(|i| i.id == dup) {
                self.duplicate = Some((dup, pending, d));
            } else {
                self.duplicate_again(pending, ctx);
            }
        }
        if let Some((_, _, d)) = &mut self.duplicate {
            match d.show(ctx) {
                Outcome::Open => {}
                Outcome::Closed => self.duplicate = None,
                Outcome::Done(choice) => {
                    if let Some((dup, pending, _)) = self.duplicate.take() {
                        self.resolve_duplicate(dup, pending, choice, ctx);
                    }
                }
            }
        }
        if let Some((_, d)) = &mut self.queue_pick {
            match d.show(ctx) {
                Outcome::Open => {}
                // Cancel: the downloads stay in the list, in no queue.
                Outcome::Closed => self.queue_pick = None,
                Outcome::Done((q, start)) => {
                    if let Some((ids, _)) = self.queue_pick.take()
                        && q < self.queues.len()
                    {
                        for id in ids {
                            if !self.queues[q].items.contains(&id) {
                                self.queues[q].items.push(id);
                            }
                        }
                        if start {
                            self.start_queue(q);
                        }
                        self.dirty = true;
                    }
                }
            }
        }
        self.grabber_checks(ctx);
        self.grab_tick(ctx);
        self.theme_picker(ctx);
        if let Some(d) = &mut self.grabber {
            match d.show(ctx) {
                Outcome::Open => {}
                Outcome::Closed => self.grabber = None,
                Outcome::Done((start, packages)) => {
                    self.grabber = None;
                    self.add_grabbed(start, packages);
                }
            }
        }
        if let Some(d) = &mut self.extracts.needs_7zip
            && !matches!(d.show(ctx), Outcome::Open)
        {
            self.extracts.needs_7zip = None;
        }
        if let Some(d) = &mut self.extracts.ask {
            match d.show(ctx) {
                Outcome::Open => {}
                Outcome::Closed => self.extracts.answer(None),
                Outcome::Done(pw) => self.extracts.answer(pw),
            }
        }
        if let Some(d) = &mut self.category_dlg {
            match d.show(ctx, &crate::categories::names()) {
                Outcome::Open => {}
                Outcome::Closed => self.category_dlg = None,
                Outcome::Done(e) => {
                    self.category_dlg = None;
                    let made = crate::categories::apply(&mut self.settings, e, &self.downloads);
                    // Made with Download File Info's "+": that download goes in it.
                    if std::mem::take(&mut self.category_for_file_info)
                        && let Some(fi) = &mut self.file_info
                    {
                        fi.set_category(made);
                    }
                    self.dirty = true;
                }
            }
        }
        if let Some(d) = &mut self.confirm_category_delete {
            match d.show(ctx) {
                Outcome::Open => {}
                Outcome::Closed => self.confirm_category_delete = None,
                Outcome::Done(remove) => {
                    let cat = d.category;
                    self.confirm_category_delete = None;
                    // Off the list first (not their files); the running ones can't go, so they
                    // move to General with the rest.
                    if remove {
                        let ids: Vec<u64> = self
                            .items
                            .iter()
                            .filter(|i| i.category == cat && i.live.is_none())
                            .map(|i| i.id)
                            .collect();
                        self.remove_downloads(&ids, false);
                    }
                    crate::categories::delete(&mut self.settings, cat, &mut self.items);
                    let gone = Some(cat);
                    if matches!(self.view, View::Cat(c) if Some(c) == gone)
                        || matches!(self.view, View::Unfinished(c) | View::Finished(c) if c == gone)
                    {
                        self.view = View::All;
                    }
                    self.dirty = true;
                }
            }
        }
        if let Some((id, d)) = &mut self.confirm_redownload {
            match d.show(ctx) {
                Outcome::Open => {}
                Outcome::Closed => self.confirm_redownload = None,
                Outcome::Done(()) => {
                    let id = *id;
                    self.confirm_redownload = None;
                    self.redownload(id);
                }
            }
        }
        if let Some((q, d)) = &mut self.confirm_queue_delete {
            match d.show(ctx) {
                Outcome::Open => {}
                Outcome::Closed => self.confirm_queue_delete = None,
                Outcome::Done(()) => {
                    let q = *q;
                    self.confirm_queue_delete = None;
                    self.delete_queue(q);
                }
            }
        }
        if let Some(d) = &mut self.add_url {
            let out = d.show(ctx, |url| self.settings.site_login(url));
            // Every address that goes on is remembered for the Address drop list.
            if let Outcome::Done(new) = &out {
                crate::settings::remember(&mut self.settings.url_history, new.url.as_str());
                self.dirty = true;
            }
            match out {
                Outcome::Open => {}
                Outcome::Closed => self.add_url = None,
                Outcome::Done(new) if new.url.scheme() == "magnet" => {
                    self.add_url = None;
                    self.open_torrent_dialog(Some(new.url.to_string()), ctx);
                }
                // A .torrent's address (#39): Add torrent fetches it and shows what it holds.
                Outcome::Done(new) if crate::torrents::is_torrent_url(&new.url, None, None) => {
                    self.add_url = None;
                    let parts = self.request_parts(
                        &new.url,
                        new.login.as_ref(),
                        &Default::default(),
                        false,
                    );
                    let remote = crate::torrents::Remote {
                        url: new.url.clone(),
                        parts,
                    };
                    self.open_torrent_dialog_with(Some(new.url.to_string()), Some(remote), ctx);
                }
                // A video's page, not a file (YouTube, Vimeo…): the video window looks it up.
                Outcome::Done(new) if mdm_tools::pages::is_video_page(&new.url) => {
                    self.add_url = None;
                    // After the one open, if one is (never in its place).
                    self.open_video_page(new.url.to_string(), Default::default());
                }
                // An image gallery's page (Wikimedia Commons, Flickr…): gallery-dl (#58).
                Outcome::Done(new) if mdm_tools::pages::is_gallery_page(&new.url) => {
                    self.add_url = None;
                    let dir = self
                        .settings
                        .category_dir(Category::General, &self.downloads);
                    self.gallery_dlg =
                        Some(crate::gallerydlg::GalleryDialog::new(dir, new.url.as_str()));
                }
                Outcome::Done(new) => {
                    self.add_url = None;
                    let url = new.url.clone();
                    if self.duplicate_gate(&url, || {
                        Pending::Url(dialogs::NewUrl {
                            url: new.url.clone(),
                            login: new.login.clone(),
                        })
                    }) {
                        self.open_file_info(new, Default::default(), ctx);
                    }
                }
            }
        }
        if let Some(d) = &mut self.confirm_delete {
            match d.show(ctx) {
                Outcome::Open => {}
                Outcome::Closed => self.confirm_delete = None,
                Outcome::Done((files, dont_show)) => {
                    let ids = std::mem::take(&mut d.ids);
                    self.confirm_delete = None;
                    self.remove_downloads(&ids, files);
                    if dont_show {
                        self.settings.confirm_delete = false;
                    }
                    self.dirty = true;
                }
            }
        }
        if let Some(n) = &mut self.limit_notice
            && !matches!(n.show(ctx), Outcome::Open)
        {
            self.limit_notice = None;
        }
        if std::mem::take(&mut self.exit_requested) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if let Some(t) = &mut self.turn_off {
            match t.show(ctx) {
                Outcome::Open => {}
                Outcome::Closed => self.turn_off = None,
                Outcome::Done(()) => {
                    t.execute();
                    self.turn_off = None;
                }
            }
        }
        let mut changes = Vec::new();
        let mut files_requested = Vec::new();
        let items = &self.items;
        let icons = &mut self.icons;
        self.properties.retain_mut(|pr| {
            let Some(item) = items.iter().find(|i| i.id == pr.id) else {
                return false;
            };
            match pr.show(ctx, icons, item) {
                Outcome::Open => {
                    if pr.take_files_requested() {
                        files_requested.push(pr.id);
                    }
                    true
                }
                Outcome::Closed => false,
                Outcome::Done(c) => {
                    changes.push(c);
                    false
                }
            }
        });
        for c in changes {
            self.apply_properties(c);
        }
        for id in files_requested {
            if let Some(item) = self.items.iter().find(|i| i.id == id) {
                self.torrent_files =
                    crate::torrentfiles::TorrentFiles::new(item, &self.torrent_host);
            }
        }
        let mut hashes = Vec::new();
        self.checksums.retain_mut(|c| match c.show(ctx) {
            Outcome::Open => true,
            Outcome::Closed => false,
            Outcome::Done(h) => {
                hashes.push((c.id, h));
                false
            }
        });
        for (id, h) in hashes {
            if let Some(item) = self.item(id) {
                item.note(crate::i18n::fmt(
                    // i18n: event log entry: the user gave the checksum the file must have
                    crate::i18n::t("Expected checksum set ({algorithm})"),
                    &[("algorithm", h.algo.name())],
                ));
                item.checksum = Some(h);
                self.dirty = true;
            }
        }
        if let Some(f) = &mut self.find {
            match f.show(ctx) {
                Outcome::Open => {}
                Outcome::Closed => self.find = None,
                Outcome::Done(()) => {
                    let q = f.query.clone();
                    self.find = None;
                    self.find_next(&q);
                    self.last_find = Some(q);
                }
            }
        }
        // View → Choose columns: its columns into the list on OK, saved with its layout
        // (the frame sees the layout change, as after a header drag).
        if let Some(d) = &mut self.columns_dlg {
            match d.show(ctx) {
                Outcome::Open => {}
                Outcome::Closed => self.columns_dlg = None,
                Outcome::Done(cols) => {
                    self.columns_dlg = None;
                    cols.apply(&mut self.list);
                    ctx.request_repaint();
                }
            }
        }
        if let Some(d) = &mut self.toolbar_dlg {
            match d.show(ctx) {
                Outcome::Open => {}
                Outcome::Closed => self.toolbar_dlg = None,
                Outcome::Done(buttons) => {
                    self.toolbar_dlg = None;
                    self.settings.toolbar_buttons = buttons;
                    self.dirty = true;
                }
            }
        }
        if let Some((title, text)) = &self.notice
            && !matches!(dialogs::message(ctx, "notice", title, text), Outcome::Open)
        {
            self.notice = None;
        }
        if !self.torrents_started {
            self.torrents_started = true;
            // The handlers follow the exe (a portable copy may have moved).
            if self.settings.torrent_associations {
                #[cfg(windows)]
                crate::win32::set_torrent_handlers(true);
                #[cfg(unix)]
                crate::unix::set_torrent_handlers(true);
            }
            if self.settings.torrent_keep_sharing {
                self.torrent_host.set_sharing(
                    &self.rt,
                    self.session_hashes(),
                    self.complete_torrents(),
                    true,
                );
            }
        }
        if let Some(t) = &mut self.torrent_settings {
            match t.show(ctx) {
                Outcome::Open => {}
                Outcome::Closed => self.torrent_settings = None,
                Outcome::Done(crate::torrentdlg::TorrentChoice {
                    share,
                    assoc,
                    net,
                    upload,
                    seeding,
                }) => {
                    self.torrent_settings = None;
                    // The network: the next torrent session takes it (torrent_tick).
                    net.apply(&mut self.settings);
                    // The upload limit: at once (torrent_tick sets it on the session).
                    (
                        self.settings.torrent_upload_on,
                        self.settings.torrent_upload_kb,
                    ) = upload;
                    // Seeding limits: the tick stops those over them; raised, the finished
                    // torrents now under them are shared again (when sharing stays on).
                    let s = &mut self.settings;
                    let before = s.sharing_limits();
                    s.torrent_stop_ratio_on = seeding.ratio_on;
                    s.torrent_stop_ratio = seeding.ratio;
                    s.torrent_stop_minutes_on = seeding.minutes_on;
                    s.torrent_stop_minutes = seeding.minutes;
                    if share
                        && share == self.settings.torrent_keep_sharing
                        && before != self.settings.sharing_limits()
                    {
                        self.torrent_host.set_sharing(
                            &self.rt,
                            self.session_hashes(),
                            self.complete_torrents(),
                            true,
                        );
                    }
                    if share != self.settings.torrent_keep_sharing {
                        self.settings.torrent_keep_sharing = share;
                        self.torrent_host.set_sharing(
                            &self.rt,
                            self.session_hashes(),
                            self.complete_torrents(),
                            share,
                        );
                    }
                    if assoc != self.settings.torrent_associations {
                        self.settings.torrent_associations = assoc;
                        #[cfg(windows)]
                        crate::win32::set_torrent_handlers(assoc);
                        #[cfg(unix)]
                        crate::unix::set_torrent_handlers(assoc);
                    }
                    self.dirty = true;
                }
            }
        }
        if let Some(w) = &mut self.rss_dlg {
            match w.show(ctx, &mut self.rss, &self.rss_busy) {
                Outcome::Open => {}
                Outcome::Closed => {
                    self.rss_dlg = None;
                    self.save_rss();
                }
                Outcome::Done(crate::rssdlg::Act::Saved) => self.save_rss(),
                Outcome::Done(crate::rssdlg::Act::Refresh(ids)) => {
                    self.save_rss();
                    let ids = if ids.is_empty() {
                        self.rss.feeds.iter().map(|f| f.id).collect()
                    } else {
                        ids
                    };
                    for id in ids {
                        self.rss_fetch(id);
                    }
                }
                // By hand: Add torrent, as for any address (a .torrent's is fetched).
                Outcome::Done(crate::rssdlg::Act::Download(link)) => {
                    let remote = url::Url::parse(&link)
                        .ok()
                        .filter(|u| matches!(u.scheme(), "http" | "https"))
                        .map(|url| crate::torrents::Remote {
                            parts: crate::app::RequestParts {
                                headers: Default::default(),
                                opts: crate::settings::own_net(),
                            },
                            url,
                        });
                    self.open_torrent_dialog_with(Some(link), remote, ctx);
                }
            }
        }
        if let Some(d) = &mut self.add_trackers {
            match d.show(ctx) {
                Outcome::Open => {}
                Outcome::Closed => self.add_trackers = None,
                Outcome::Done(list) => {
                    let id = d.id;
                    self.add_trackers = None;
                    self.torrent_act(id, crate::torrents::Act::AddTrackers(list));
                }
            }
        }
        let mut torrent_files_selection = None;
        if let Some(d) = &mut self.torrent_files {
            match d.show(ctx) {
                Outcome::Open => {
                    torrent_files_selection = d.take_selection().map(|selection| (d.id, selection));
                }
                Outcome::Closed => self.torrent_files = None,
                Outcome::Done(_) => self.torrent_files = None,
            }
        }
        if let Some((id, (files, all, size))) = torrent_files_selection {
            self.choose_files(id, files, all, size);
        }
        // The next torrent handed over while Add torrent was open (#507), a frame after it
        // closed: that frame without it closes its window, and the next one opens as a window
        // of its own (opened in the same frame, the closed window would only change its
        // contents). One that goes to the debrid account opens nothing, and the one after it
        // is next.
        if self.torrent_dlg.is_none() && !self.torrent_waiting.is_empty() {
            if std::mem::take(&mut self.torrent_closed) {
                ctx.request_repaint();
            } else {
                while self.torrent_dlg.is_none()
                    && let Some((source, remote)) = self.torrent_waiting.pop_front()
                {
                    self.open_torrent_dialog_with(Some(source), remote, ctx);
                }
            }
        }
        if let Some(t) = &mut self.torrent_dlg {
            let closed = match t.show(ctx, &self.rt) {
                Outcome::Open => false,
                Outcome::Closed => true,
                Outcome::Done(add) => {
                    if self.settings.change_folder_on_last
                        && self
                            .settings
                            .keep_category_dir(add.category, &add.dir, &self.downloads)
                    {
                        self.dirty = true;
                    }
                    self.torrent_dlg = None;
                    self.add_torrent(add);
                    true
                }
            };
            if closed {
                self.torrent_dlg = None;
                self.torrent_closed = true;
                ctx.request_repaint();
            }
        }
        // The next Metalink handed over while Add Metalink was open, a frame after it closed (as
        // Add torrent's waiting torrents).
        if self.metalink_dlg.is_none() && !self.metalink_waiting.is_empty() {
            if std::mem::take(&mut self.metalink_closed) {
                ctx.request_repaint();
            } else if let Some(opened) = self.metalink_waiting.pop_front() {
                self.open_metalink(opened, ctx);
            }
        }
        if let Some(m) = &mut self.metalink_dlg {
            let out = m.show(ctx);
            if !matches!(out, Outcome::Open) {
                self.metalink_dlg = None;
                self.metalink_closed = true;
                ctx.request_repaint();
            }
            if let Outcome::Done(add) = out {
                self.add_metalink(add);
            }
        }
        if let Some(g) = &mut self.gallery_dlg {
            match g.show(ctx, &self.rt) {
                Outcome::Open => {}
                Outcome::Closed => self.gallery_dlg = None,
                Outcome::Done(add) => {
                    self.gallery_dlg = None;
                    self.add_gallery(add);
                }
            }
        }
        // The next video page handed over while Download video was open, a frame after it closed
        // (opened in the same frame, the closed window would only change its contents), as Add
        // torrent's waiting torrents.
        if self.video_dlg.is_none() && !self.video_waiting.is_empty() {
            if std::mem::take(&mut self.video_closed) {
                ctx.request_repaint();
            } else if let Some((page, cmd, pick)) = self.video_waiting.pop_front() {
                let dlg = self.video_dialog(Some((&page, cmd)));
                self.video_dlg = Some(match pick {
                    Some(p) => dlg.picked(*p),
                    None => dlg,
                });
            }
        }
        if let Some(v) = &mut self.video_dlg {
            match v.show(ctx, &self.rt) {
                Outcome::Open => {}
                Outcome::Closed => {
                    self.video_dlg = None;
                    self.video_closed = true;
                    ctx.request_repaint();
                }
                Outcome::Done(out) => {
                    self.video_closed = true;
                    ctx.request_repaint();
                    // Options' "Change folder for this category on last selected", which
                    // Download File Info follows through its box (#475): Music's for a
                    // sound-only choice (#611).
                    if self.settings.change_folder_on_last
                        && self.settings.keep_category_dir(
                            v.category(),
                            &v.save_folder(),
                            &self.downloads,
                        )
                    {
                        self.dirty = true;
                    }
                    self.video_dlg = None;
                    match out {
                        crate::videodlg::VideoOut::One(add) => {
                            self.add_video_checked(add);
                        }
                        crate::videodlg::VideoOut::Many(add) => self.add_playlist(add),
                    }
                }
            }
        }
        if let Some(b) = &mut self.batch {
            match b.show(ctx) {
                Outcome::Open => {}
                Outcome::Closed => self.batch = None,
                Outcome::Done(added) => {
                    self.batch = None;
                    self.add_batch(added);
                }
            }
        }
        if let Some(d) = &mut self.options {
            let outcome = d.show(ctx, &self.downloads);
            // General → Test browser integration → Add to your browser...: the extension's
            // window, the one each start offers until its "Don't ask again".
            if std::mem::take(&mut d.offer_extension) {
                match &self.extension {
                    Some(_) => dialogs::raise(ctx, dialogs::dialog_viewport("extension")),
                    None => self.extension = Some(crate::extension::Extension::new()),
                }
            }
            match outcome {
                Outcome::Open => {}
                Outcome::Closed => {
                    // Cancel: categories the dialog added, changed or restored are undone, and
                    // the remote control listens as before Pair a device... moved it.
                    self.settings.publish_categories();
                    if let Some(r) = &self.remote {
                        r.apply(crate::remote::config(&self.settings));
                    }
                    self.options = None;
                }
                Outcome::Done(new) => {
                    let new = Settings::merge_changes(&self.settings, &d.from, &new);
                    self.options = None;
                    self.apply_options(new);
                }
            }
        }
        if let Some(mut s) = self.scheduler.take() {
            let mut actions = Vec::new();
            let open = s.show(
                ctx,
                &mut self.icons,
                &self.items,
                &self.queues,
                self.wake.error.as_deref(),
                &mut actions,
            );
            for a in actions {
                self.scheduler_action(a);
            }
            if open {
                self.scheduler = Some(s);
            }
        }
        crate::statsdlg::run(ctx, &mut self.statistics, &mut self.stats);
        if let Some(d) = &mut self.speed_settings {
            match d.show(ctx) {
                Outcome::Open => {}
                Outcome::Closed => self.speed_settings = None,
                Outcome::Done(c) => {
                    self.settings.speed_limit_kb = c.kb;
                    self.settings.limiter_on_startup = c.on_startup;
                    self.settings.global_limit_on = c.global_on;
                    self.settings.global_limit_kb = c.global_kb;
                    self.settings.global_only_active = c.only_active;
                    self.speed_settings = None;
                    self.apply_speed_limit();
                    self.dirty = true;
                }
            }
        }
        if let Some(mut fi) = self.file_info.take() {
            self.start_early(&mut fi);
            // The downloads waiting their turn behind this one (a browser selection's arrive
            // one by one): Cancel asks whether they are cancelled too (#394).
            fi.waiting = self.file_info_waiting.len();
            let out = fi.show(ctx, &mut self.icons);
            if std::mem::take(&mut fi.new_category) {
                self.category_for_file_info = true;
                self.run(Cmd::AddCategory, None, ctx);
            }
            // The folder a download goes to is remembered for the Save As drop list.
            if matches!(out, Outcome::Done(_))
                && let Some(dir) = fi.dest().parent()
            {
                crate::settings::remember(
                    &mut self.settings.folder_history,
                    &dir.display().to_string(),
                );
                self.dirty = true;
            }
            // The address was edited after the early start, or "Download through" changed (#44):
            // that download is the old one's. It goes; the new one starts early in its turn
            // (start_early).
            if let Some(id) = fi.early
                && self
                    .item(id)
                    .is_some_and(|i| i.url != fi.url || i.direct != fi.direct)
            {
                self.stop(id);
                self.after_stop.push((id, None));
                self.held_finished.retain(|h| *h != id);
                fi.early = None;
            }
            match out {
                Outcome::Open => self.file_info = Some(fi),
                // The address was a Metalink of several files: Add Metalink lists them.
                Outcome::Closed if fi.handoff.is_some() => {
                    if let Some(opened) = fi.handoff.take() {
                        self.open_metalink(opened, ctx);
                    }
                }
                Outcome::Closed => {
                    if fi.cancel_all {
                        // #394: "Yes" to "Cancel them too?": the browser's links waiting
                        // behind this one are cancelled with it (the browser gave its own copy
                        // up, and Download File Info was their step).
                        self.file_info_waiting.clear();
                    }
                    // Cancel: the download started early goes with its files, never having
                    // been in the list (no row, no queue, nothing counted).
                    if let Some(id) = fi.early {
                        self.stop(id);
                        self.after_stop.push((id, None));
                        self.held_finished.retain(|h| *h != id);
                    }
                }
                Outcome::Done(start) => match fi.early {
                    Some(id) => self.adopt_early(&fi, id, start),
                    None => self.add(&mut fi, start),
                },
            }
        }
        // The next download that waited for Download File Info.
        if self.file_info.is_none()
            && let Some((new, context, pick)) = self.file_info_waiting.pop_front()
        {
            self.open_file_info_with(new, context, pick, ctx);
        }
        self.finish_stopped();
        let mut actions = Vec::new();
        // A progress window can be hidden in the tray only when there is one to bring it back.
        // Off-screen test copies have no tray icon: their windows are drawn as if they had one
        // (the captures and the accessibility checks see the hide button), unless the tray icon
        // is turned off (View → Tray icon).
        let tray = self.tray_available();
        let _progress = crate::framestats::span("progress windows");
        let rename_stamp = if !self.progress.is_empty() {
            self.progress_rename_guard_stamp()
        } else {
            0
        };
        for p in &mut self.progress {
            p.keep_sharing = self.settings.torrent_keep_sharing;
            if p.host.is_none() {
                p.host = Some(self.torrent_host.clone());
            }
            p.tray = tray;
            p.minimize_to_tray = self.settings.minimize_to_tray;
            p.main_shown = !self.hidden;
            // Hidden: not shown (its window is gone) until the tray or the list brings it back.
            if p.hidden && tray {
                continue;
            }
            p.hidden = false;
            p.speed = self.speeds.of(p.id).cloned();
            if let Some(item) = self.items.iter().find(|i| i.id == p.id) {
                if item.status == Status::Complete {
                    p.rename_disabled_reason = None;
                    p.rename_guard_stamp = Some(rename_stamp);
                } else if p.rename_guard_stamp != Some(rename_stamp) {
                    // Looked at off the window's thread (#743), with copies of the rows' places.
                    if p.rename_guard
                        .as_ref()
                        .is_none_or(|(s, _)| *s != rename_stamp)
                    {
                        let asked = crate::launch::Asked::default();
                        let (item, items) = (
                            item.place_copy(),
                            self.items.iter().map(Item::place_copy).collect::<Vec<_>>(),
                        );
                        let (downloads, settings, rss) = (
                            self.downloads.clone(),
                            self.settings.clone(),
                            self.rss.clone(),
                        );
                        let _ = asked.get(
                            &rename_stamp,
                            Duration::MAX,
                            Duration::ZERO,
                            Some(ctx),
                            move |_| {
                                deferred_rename_disabled_reason_with(
                                    &item, &downloads, &settings, &rss, &items,
                                )
                            },
                        );
                        p.rename_guard = Some((rename_stamp, asked));
                    }
                    if let Some(reason) = p.rename_guard.as_ref().and_then(|(s, a)| a.answer(s)) {
                        p.rename_disabled_reason = reason;
                        p.rename_guard_stamp = Some(rename_stamp);
                        p.rename_guard = None;
                    }
                }
                actions.push((p.id, p.show(ctx, item, item.snap.as_ref())));
            }
        }
        drop(_progress);
        for (id, a) in actions {
            match a {
                progress::Action::None => {}
                progress::Action::Pause => self.stop_by_hand(id),
                progress::Action::Resume => self.start(id, true),
                // Cancel, or the window's × on a running download (as IDM's): stopped, resumable;
                // a name confirmed in its Rename box is kept.
                progress::Action::Cancel => {
                    let confirmed = self
                        .progress
                        .iter_mut()
                        .find(|p| p.id == id)
                        .and_then(Progress::take_confirmed_rename);
                    if let Some(name) = confirmed
                        && let Some(item) = self.item(id)
                    {
                        item.rename_on_complete = name;
                        self.dirty = true;
                    }
                    self.stop_by_hand(id);
                    self.progress.retain(|p| p.id != id);
                }
                // The browser has it now (#376): its window closes, the row stays.
                progress::Action::WithBrowser => {
                    self.download_with_browser(id);
                    self.progress.retain(|p| p.id != id);
                }
                // The torrent's player (its window stays: the download goes on).
                progress::Action::Play => self.play_torrent(id),
                progress::Action::Rename(name) => {
                    if let Some(item) = self.item(id) {
                        item.rename_on_complete = name;
                        self.dirty = true;
                    }
                }
                progress::Action::Close => {
                    let confirmed = self
                        .progress
                        .iter_mut()
                        .find(|p| p.id == id)
                        .and_then(Progress::take_confirmed_rename);
                    if let Some(name) = confirmed
                        && let Some(item) = self.item(id)
                    {
                        item.rename_on_complete = name;
                        self.dirty = true;
                    }
                    self.progress.retain(|p| p.id != id);
                }
                progress::Action::Hide => {
                    let mut confirmed = None;
                    if let Some(p) = self.progress.iter_mut().find(|p| p.id == id) {
                        confirmed = p.take_confirmed_rename();
                        p.hidden = true;
                    }
                    if let Some(name) = confirmed
                        && let Some(item) = self.item(id)
                    {
                        item.rename_on_complete = name;
                        self.dirty = true;
                    }
                }
                progress::Action::Limit(limit, remember) => {
                    let mut helper = false;
                    if let Some(item) = self.item(id) {
                        if let Some(l) = item.live.as_ref() {
                            l.dl.set_speed_limit(limit);
                            helper = l.dl.is_helper();
                        }
                        // Remembered, it applies again after a stop or a restart.
                        item.own_limit = if remember { limit } else { None };
                        self.dirty = true;
                    }
                    // A video or a gallery starts again with it (#708).
                    if helper {
                        self.helper_own.insert(id, limit);
                        self.reapply_helper_limits();
                    }
                }
                progress::Action::UploadLimit(limit) => {
                    if let Some(item) = self.item(id)
                        && let Some(spec) = item.torrent.as_mut()
                    {
                        // Kept with the torrent; a Run sets it now, a torrent sharing without
                        // one gets it at the next torrent_tick (torrent_accounts).
                        spec.upload_limit = limit;
                        if let Some(model::Job::Torrent(run)) = item.live.as_ref().map(|l| &l.dl) {
                            run.set_upload_limit(limit);
                        }
                        self.dirty = true;
                    }
                }
                progress::Action::Choose { files, all, size } => {
                    self.choose_files(id, files, all, size)
                }
            }
        }
        // Download complete, one box at a time at one place, as Download File Info is: many
        // downloads finishing together (a batch, a playlist) never cover the screen or fill the
        // taskbar; the next shows as one closes, and the one shown says how many wait behind it,
        // with Close all. Each keeps its own Open and Open folder.
        let waiting = self.complete.len().saturating_sub(1);
        if let Some(c) = self.complete.first_mut() {
            c.more = waiting;
            if !matches!(c.show(ctx), Outcome::Open) {
                let c = self.complete.remove(0);
                // "Don't show this again": none of the others either.
                if c.dont_show {
                    self.settings.show_complete = false;
                    self.dirty = true;
                }
                if c.dont_show || c.close_all {
                    self.complete.clear();
                }
                ctx.request_repaint();
            }
        }
    }
}
