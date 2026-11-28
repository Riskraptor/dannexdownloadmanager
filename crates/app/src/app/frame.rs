//! What runs at every frame, window or not (`tick`, from eframe's `logic` and a headless run's
//! loop): commands taken in, the downloads' progress pulled into the rows, the list saved, and the
//! window's place and size remembered.

use super::*;

impl eframe::App for App {
    /// What comes in from outside (a second launch, the browser's native host, the automation
    /// API) is handled here, which runs at every frame: `ui` doesn't run while the main window
    /// is minimized, and a magnet or a download handed over then waited until it was restored
    /// (#162). A command that opens a window gets the UI pass that creates it, minimized or not
    /// (no window is raised or focused for it beyond what the command itself does).
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // DDM's own work: not counted as the graphics API's time to its first frame (#280).
        crate::renderer::own_work_begin();
        let stats = crate::framestats::begin();
        self.tick(ctx);
        crate::framestats::logic_done(stats);
        crate::renderer::own_work_end();
    }

    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        self.paint_frame(ui, frame);
    }
}

impl App {
    /// Pull engine progress into the rows; open "Download complete" when one finishes.
    pub(super) fn poll(&mut self, ctx: &egui::Context) {
        self.take_made();
        self.poll_missing();
        self.poll_leftovers();
        self.poll_virus_checks();
        self.poll_moves();
        self.poll_numbering();
        self.poll_torrent_checks();
        self.poll_duplicates(ctx);
        self.poll_video_adds();
        self.poll_playlist_adds();
        self.poll_beside(ctx);
        self.poll_tools_wait();
        self.poll_gallery_adds();
        self.poll_deferred_renames();
        self.poll_deferred_torrent_renames();
        if !self.moving.is_empty()
            || !self.renaming.is_empty()
            || !self.renaming_torrents.is_empty()
            || !self.deferred_torrent_queue.is_empty()
            || !self.numbering.is_empty()
            || !self.torrent_checks.is_empty()
            || !self.dup_waits.is_empty()
            || !self.video_adds.is_empty()
            || !self.playlist_adds.is_empty()
            || !self.beside.is_empty()
            || !self.tools_wait.is_empty()
            || !self.gallery_adds.is_empty()
        {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
        let mut finished = Vec::new();
        let mut failed = Vec::new();
        let mut any_live = false;
        for i in &mut self.items {
            if i.poll() {
                self.dirty = true;
                match i.status {
                    Status::Complete => finished.push(i.id),
                    Status::Error => failed.push(i.id),
                    _ => {}
                }
            }
            any_live |= i.live.is_some();
        }
        // One started early behind Download File Info isn't in the list yet: its end waits for
        // the dialog's answer (`held_finished`); a failure is the dialog's to see (Start starts
        // it again). Those the answer took in end now.
        let unlisted: Vec<u64> = self
            .items
            .iter()
            .filter(|i| !i.listed())
            .map(|i| i.id)
            .collect();
        if !unlisted.is_empty() {
            let (held, listed): (Vec<u64>, Vec<u64>) =
                finished.into_iter().partition(|id| unlisted.contains(id));
            self.held_finished.extend(held);
            finished = listed;
            failed.retain(|id| !unlisted.contains(id));
        }
        finished.append(&mut self.released_finished);
        // Beyond IDM: the bytes these reads found, for the speed graphs and the statistics.
        let reads = crate::stats::take_reads(&mut self.items);
        self.speeds.tick(&reads);
        self.stats.record(&self.items, &reads, &finished);
        let now = Instant::now();
        self.check_limits(now);
        if !finished.is_empty() {
            self.sound(0);
        }
        if !failed.is_empty() {
            self.sound(1);
        }
        for id in failed {
            let e = self.failures.entry(id).or_insert((now, 0));
            *e = (now, e.1 + 1);
            for q in &mut self.queues {
                queue::requeue_failed(q, id);
            }
            self.schedule_auto_resume(id, now);
        }
        self.auto_resume_tick(now, ctx);
        for id in &finished {
            self.release_link(*id);
            self.failures.remove(id);
            self.auto_resume.forget(*id);
            // A deferred rename runs beside the window (#743): its answer does these then.
            let renaming = self.apply_deferred_rename(*id);
            if !renaming {
                self.flush_pending_mark(*id);
                // Its archive, when it is to be extracted and its last volume is in (#37).
                self.extracts.finished(*id, &self.items, &self.settings);
                self.virus_check(*id);
            }
            // An offline copy's page or style sheet: its links made to lead to the copy.
            self.grab_finished(*id);
            // Finished files leave download queues (synchronization queues keep them).
            for q in self.queues.iter_mut().filter(|q| !q.sync) {
                q.items.retain(|x| x != id);
            }
        }
        if self.extracts.poll(&mut self.items, ctx) {
            self.dirty = true;
        }
        self.tick_queues();
        // A download started early behind Download File Info stays quiet until the dialog's
        // answer (Start shows its box then; Cancel drops it).
        let early = self.file_info.as_ref().and_then(|f| f.early);
        let close = finished_windows(
            finished,
            |id| self.progress.iter().any(|p| p.id == id && !p.hidden),
            |id| {
                self.items
                    .iter()
                    .any(|i| i.id == id && i.status == Status::Complete)
            },
            &mut self.finishing,
            now,
        );
        for id in close {
            self.close_finished(id, early);
        }
        // The next one's moment ends without a frame of its own otherwise (nothing runs).
        if let Some(&(_, at)) = self.finishing.iter().min_by_key(|(_, at)| *at) {
            ctx.request_repaint_after(progress::SHOWN_WHOLE.saturating_sub(now.duration_since(at)));
        }
        if any_live {
            ctx.request_repaint_after(Duration::from_millis(250));
            // The list's progress figures, every 30 s while something runs: after a crash the
            // rows show about where each download was (torrents keep their own resume data,
            // files their state file; this is only what the list shows).
            if self.last_save.elapsed() > Duration::from_secs(30) {
                self.dirty = true;
            }
        }
        // The keychain took a secret (#44): saved again, with its reference in its place.
        let vault = crate::secrets::vault();
        if vault.pending() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        let confirmed = vault.confirmed();
        if confirmed != self.secrets_confirmed {
            self.secrets_confirmed = confirmed;
            self.dirty = true;
        }
        if self.dirty && self.last_save.elapsed() > Duration::from_secs(1) {
            // Written on the saver's thread (store::Saver); refused at once when the list
            // couldn't be read at start.
            if let Err(e) = self.saver.save(
                self.next_id,
                &self.items,
                &self.list.layout(),
                &self.settings,
                &self.queues,
            ) {
                self.report_save(Err(e.to_string()));
            }
            self.dirty = false;
            self.last_save = Instant::now();
        }
        if let Some(outcome) = self.saver.outcome() {
            self.report_save(outcome);
        }
    }

    /// A list that can't be saved says so, once for each reason (not every second); it was
    /// silent and a restart lost the changes.
    pub(super) fn report_save(&mut self, outcome: Result<(), String>) {
        {
            match outcome {
                Ok(()) => self.save_failed = None,
                Err(e) => {
                    if self.save_failed.as_ref() != Some(&e) {
                        self.notice = Some((
                            crate::i18n::mark("Dannex Download Manager"),
                            crate::i18n::fmt(
                                crate::i18n::t(
                                    "The download list could not be saved: {error}. Changes are kept while DDM runs.",
                                ),
                                &[("error", &e)],
                            ),
                        ));
                    }
                    self.save_failed = Some(e);
                }
            }
        }
    }

    /// The window's place and size, the Categories pane and the node shown, kept in the
    /// settings as they change (so they come back at the next start).
    pub(super) fn remember_layout(&mut self, ctx: &egui::Context) {
        // Screen readers speak in the interface's language (its voice for Arabic, Japanese...).
        let lang = crate::i18n::code();
        if self.a11y_lang != lang {
            ctx.set_language(Some(lang.to_owned()));
            self.a11y_lang = lang;
        }
        let s = &mut self.settings;
        let pane = (
            self.show_categories,
            [
                self.all_open,
                self.unfinished_open,
                self.finished_open,
                self.queues_open,
            ],
            // A grabber project's node isn't kept: a DDM from before Site grabber couldn't read
            // the settings with it.
            Some(self.view).filter(|v| !matches!(v, View::Grabber(_))),
        );
        if (s.show_categories, s.branches, s.view) != pane {
            (s.show_categories, s.branches, s.view) = pane;
            self.dirty = true;
        }
        // Not a test instance's off-screen window, nor one zoomed for screenshots (MDM_ZOOM).
        if crate::offscreen() || crate::scale::test_zoom().is_some() {
            return;
        }
        // Kept at 100 % interface size (the system's logical pixels), as the window is made
        // before the zoom applies (main.rs).
        let zoom = ctx.zoom_factor();
        let (outer, inner, maximized, minimized) = ctx.input(|i| {
            let v = i.viewport();
            (v.outer_rect, v.inner_rect, v.maximized, v.minimized)
        });
        if minimized == Some(true) {
            return;
        }
        if let Some(m) = maximized {
            self.maximized = Some(m);
            if s.window_maximized != m {
                s.window_maximized = m;
                self.dirty = true;
            }
        }
        if self.maximized == Some(false)
            && let (Some(outer), Some(inner)) = (outer, inner)
        {
            let rect = [
                (outer.min.x * zoom).round() as i32,
                (outer.min.y * zoom).round() as i32,
                (inner.width() * zoom).round() as i32,
                (inner.height() * zoom).round() as i32,
            ];
            if s.window != Some(rect) {
                s.window = Some(rect);
                self.dirty = true;
            }
        }
    }

    /// What runs at every frame, window or not (`logic`, and a headless run's loop).
    pub fn tick(&mut self, ctx: &egui::Context) {
        // The freeze watchdog times this as it times `ui` (crash.rs): a command or the clock
        // that hangs is reported too, minimized or not (#202).
        crate::crash::logic_begin();
        // The torrent columns follow the view (list::DEFAULT_AUTO): shown in the Torrents
        // category's, until the person shows or hides one.
        self.list.torrent_view = matches!(
            self.view,
            View::Cat(Category::Torrents)
                | View::Unfinished(Some(Category::Torrents))
                | View::Finished(Some(Category::Torrents))
        );
        let mut handled = false;
        // A command, or the quality menu's choice: it may open a window.
        let mut opens = false;
        while let Ok(inc) = self.commands.try_recv() {
            handled = true;
            match inc.reply {
                Some(reply) => {
                    opens |= inc.cmd.video_pick.is_some();
                    let _ = reply.send(self.answer(&inc.cmd));
                }
                None => {
                    opens = true;
                    crate::starttime::taken(&inc.cmd, inc.at);
                    self.handle_command(inc.cmd, ctx);
                }
            }
        }
        if handled {
            ctx.request_ui_pass(egui::ViewportId::ROOT);
            // The pass that opens its window now, the main window in the notification area too
            // (#3); a question's (the quality menu's polls) needs none.
            if opens {
                crate::starttime::nudge();
            }
        }
        self.lookups.prune(Instant::now());
        // The clock: what must go on whatever the main window's state (#174). Minimized, egui
        // runs no `ui`, and the queues, the Scheduler's starts and stops, the download limits,
        // the kill switch, saving the list, the clipboard monitor, the update check, the tray and
        // the taskbar's progress all stood still until the window was restored.
        crate::settings::share_proxy(self.settings.proxy());
        crate::settings::share_dns(self.settings.dns());
        crate::settings::share_video_cookies(&self.settings);
        // Options → Save to → Temporary directory: where new downloads keep their parts.
        mdm_engine::naming::set_temp_dir(
            Some(PathBuf::from(self.settings.temp_dir.trim())).filter(|d| d.is_absolute()),
        );
        if !self.headless {
            self.follow_assistive_technology(ctx);
        }
        self.network_adapter_tick(ctx);
        // The video helpers DDM gets by itself when one is missing (autohelpers.rs).
        crate::autohelpers::tick(ctx, &self.rt);
        self.limit_restart_tick();
        self.debrid_tick();
        self.poll(ctx);
        self.poll_remote_torrents();
        // What failed in any window, told here (dialogs::tell).
        if let Some((title, text)) = dialogs::told().pop() {
            self.notice = Some((title, text));
        }
        self.apply_global_limit();
        // A server (#157) has no screen, clipboard or tray, and is updated by its image.
        if !self.headless {
            self.auto_update_check(ctx);
            self.watch_clipboard(ctx);
            self.tray(ctx);
            // The taskbar button's green bar: everything running, as one; none when View → Tray
            // icon → Show progress on the taskbar button is off (a bar shown is taken away).
            #[cfg(windows)]
            crate::win32::taskbar_progress(
                crate::MAIN_TITLE,
                self.overall_progress()
                    .filter(|_| self.settings.taskbar_progress),
            );
        }
        if self
            .ticked
            .is_none_or(|t| t.elapsed() > Duration::from_secs(2))
        {
            self.ticked = Some(Instant::now());
            // For a crash report: what DDM has open and running (sent when it changes).
            crate::crash::state(
                self.items.len(),
                self.items.iter().filter(|i| i.live.is_some()).count(),
                self.queues.len(),
                ctx.pixels_per_point(),
            );
            self.torrent_tick();
            // Minimized, the windows these open (Download complete, a limit's notice, Download
            // File Info for a copied link, an update) are made by a pass of the main window's
            // UI, which egui runs then only when asked: every 2 s, as the clock repaints.
            if ctx.input(|i| i.viewport().minimized == Some(true)) {
                ctx.request_ui_pass(egui::ViewportId::ROOT);
            }
        }
        ctx.request_repaint_after(Duration::from_secs(2));
        crate::crash::logic_end();
    }
}
