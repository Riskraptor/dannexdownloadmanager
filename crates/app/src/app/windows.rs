//! Opening the windows: `/show <window>` by name, Download File Info, File Properties, the progress
//! windows and when a finished download's closes (Download complete then opens).

use super::*;

impl App {
    /// `/show <window>` (mdm_ipc::Command::show): open one of the windows by name, as its menu
    /// command does. Unknown names do nothing.
    pub(super) fn show_named(&mut self, name: &str, ctx: &egui::Context) {
        self.show_window(ctx);
        let first = self.items.first().map(|i| i.id);
        let last = self.items.last().map(|i| i.id);
        let (window, tab) = name.split_once(':').unwrap_or((name, ""));
        match window {
            "addurl" => self.run(Cmd::AddUrl, None, ctx),
            // MDM_CRASH_TEST=1 only (crash.rs): the crash reports' own check.
            "crash-test" => crate::crash::test(tab),
            "batch" => self.run(Cmd::Batch(false), None, ctx),
            "find" => self.run(Cmd::Find, None, ctx),
            // View → Choose columns.
            "columns" => self.run(Cmd::CustomizeColumns, None, ctx),
            // View → Toolbar → Customize... (screenshots).
            "toolbar" => self.run(Cmd::CustomizeToolbar, None, ctx),
            // scheduler:files, scheduler:limits, scheduler:new open it on that page, or with New
            // queue's name asked (screenshots).
            "scheduler" => match tab {
                "files" => self.run(Cmd::EditQueue(0), None, ctx),
                "new" => self.run(Cmd::NewQueue, None, ctx),
                _ => {
                    self.run(Cmd::Scheduler, None, ctx);
                    if let (Some(s), "limits") = (&mut self.scheduler, tab) {
                        s.show_limits();
                    }
                }
            },
            // Off-screen test copies: Tasks → Export / Import (`export:ef2`, `import:txt`), their
            // file dialog answered by MDM_TEST_EXPORT_TO / MDM_TEST_IMPORT_FROM.
            "export" if crate::offscreen() => {
                self.run(Cmd::Export { ef2: tab == "ef2" }, None, ctx)
            }
            "import" if crate::offscreen() => {
                self.run(Cmd::Import { ef2: tab == "ef2" }, None, ctx)
            }
            // Add Metalink with a made-up Metalink of three files (screenshots, checks); and,
            // off-screen, its Later pressed: the files become downloads in the main queue.
            "metalink" => {
                self.metalink_dlg = Some(crate::metalinkdlg::MetalinkDialog::sample(
                    self.downloads.clone(),
                    self.settings.category_dirs.clone(),
                ));
            }
            "metalink-add" if crate::offscreen() => {
                if let Some(mut m) = self.metalink_dlg.take() {
                    let add = m.press_later();
                    self.add_metalink(add);
                }
            }
            // Off-screen test copies: Link grabber's Add to queue, as its button does.
            "grabber-add" if crate::offscreen() => {
                if let Some(g) = self.grabber.take() {
                    let picked = g.picked();
                    self.add_grabbed(false, picked);
                }
            }
            // Link grabber with thousands of a page's links (`grabber-many:5000`), for its speed
            // and its screenshots: nothing asked of the network.
            "grabber-many" => {
                let n = tab.parse().unwrap_or(2000);
                self.show_import(Vec::new(), String::new());
                if let Some(g) = &mut self.grabber {
                    g.look_up = false;
                    g.many(n);
                }
            }
            "about" => self.run(Cmd::About, None, ctx),
            // Update now's end-to-end tests (feature `test-update`, never in a release): Update
            // now pressed in the window `/show updates` opened (against MDM_UPDATE_FEED).
            #[cfg(feature = "test-update")]
            "update-now" => {
                if let Some(u) = &mut self.updates {
                    u.press_update_now();
                }
            }
            // The main window into the tray, as closing it does (tests and #72's measurements).
            "tray" => self.hide_to_tray(ctx),
            // Off-screen test copies: the drop target's menu, as a right-click on it opens it
            // (#208's checks; a posted click has no pointer position by the time it is seen).
            "drop-target-menu" if crate::offscreen() => {
                if let Some(at) = self.drop_target.as_ref().map(|d| d.center()) {
                    self.drop_target_menu(at, ctx);
                }
            }
            // The main window minimized, not to the tray (tests of #162).
            "minimized" => ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
            // Off-screen test copies: the interface zoomed (`zoom:1.5`) while running, as a
            // display's scale changing does to every window (#89's checks).
            "zoom" if crate::offscreen() => {
                if let Ok(z) = tab.parse::<f32>()
                    && (0.5..=4.0).contains(&z)
                {
                    ctx.set_zoom_factor(z);
                }
            }
            // The system's graphics API's device lost on purpose (Direct3D, Vulkan, Metal); DDM
            // starts again (renderer.rs).
            #[cfg(gpu)]
            "renderer-lost" if crate::offscreen() => crate::renderer::test_device_lost(),
            "torrent-settings" => self.run(Cmd::TorrentSettings, None, ctx),
            // The first download's: Add trackers (a torrent), its right-click menu.
            "add-trackers" if first.is_some() => self.run(Cmd::TorrentAddTrackers, first, ctx),
            // rss:rules opens it on its Download rules tab.
            "rss" => self.run(Cmd::RssFeeds(tab == "rules"), None, ctx),
            // menu:torrent opens it with its Torrent submenu open. A number (menu:3) is the
            // menu bar's, below, with downloads in the list or not.
            "menu" if first.is_some() && !tab.starts_with(|c: char| c.is_ascii_digit()) => {
                if let Some(id) = first {
                    self.selection.set(id);
                }
                self.open_popup(Popup::Item(first), Pos2::new(240.0, 150.0));
                let sub = self
                    .item_entries(first)
                    .iter()
                    .position(|e| matches!(e, MenuEntry::Sub("Torrent", ..)));
                if let (Some(i), "torrent") = (sub, tab) {
                    let id = format!("popup-{}", self.popup_serial);
                    chrome::open_sub_at(ctx, &id, i);
                }
            }
            // category:Music / category-delete:Music — the Categories right-click menu's
            // Properties and Delete for a category by name (#93; the docs' shots and checks);
            // category-remove:Music opens Delete with "Remove its downloads" chosen.
            "category" | "category-delete" | "category-remove" => {
                if let Some(c) = Category::all()
                    .into_iter()
                    // `/show` arrives in lower case (mdm_ipc::parse_args).
                    .find(|c| c.key().eq_ignore_ascii_case(tab) || c.shown().to_lowercase() == tab)
                {
                    let cmd = if window == "category" {
                        Cmd::CategoryProps(c)
                    } else {
                        Cmd::DeleteCategory(c)
                    };
                    self.run(cmd, None, ctx);
                    if window == "category-remove" {
                        self.confirm_category_delete =
                            self.confirm_category_delete.take().map(|d| d.removing());
                    }
                }
            }
            "video-page" => self.run(Cmd::VideoPage, None, ctx),
            // Download video from a page → Helpers... (#51).
            "video-helpers" => {
                self.run(Cmd::VideoPage, None, ctx);
                if let Some(v) = &mut self.video_dlg {
                    v.open_helpers();
                }
            }
            // Download video from a page with a sample video's subtitles (#68).
            // `video-subs:flac`: with that choice picked (screenshots of the lower choices).
            "video-subs" => {
                self.run(Cmd::VideoPage, None, ctx);
                if let Some(v) = &mut self.video_dlg {
                    v.show_sample();
                    if !tab.is_empty() {
                        v.pick(tab);
                    }
                }
            }
            // A page whose videos need ffmpeg (#351): without it the list has only the sound.
            "video-adaptive" => {
                self.run(Cmd::VideoPage, None, ctx);
                if let Some(v) = &mut self.video_dlg {
                    v.show_sample_adaptive();
                }
            }
            // A sample playlist (#69), its quality by its place in the list: `video-playlist:7`.
            "video-playlist" => {
                self.run(Cmd::VideoPage, None, ctx);
                if let Some(v) = &mut self.video_dlg {
                    v.show_sample_list(tab);
                }
            }
            "existing-files" => {
                // In General's folder (a test's own, set in its data folder), not the system's
                // Downloads, whose path names the user.
                let dir = self
                    .settings
                    .category_dir(Category::General, &self.downloads)
                    .join("Big Buck Bunny");
                self.shown_existing = Some(crate::dialogs::ExistingFiles::new(&[
                    dir.join("big_buck_bunny_1080p.mkv"),
                    dir.join("subtitles.srt"),
                    dir.join("poster.jpg"),
                ]));
            }
            "extension" => self.extension = Some(crate::extension::Extension::new()),
            // The Safari row's "How to Add" steps, open (screenshots).
            "safari-steps" => {
                let mut offer = crate::extension::Extension::new();
                offer.open_safari_steps();
                self.extension = Some(offer);
            }
            "language" => self.run(Cmd::ChooseLanguage, None, ctx),
            "move-rename" => self.run(Cmd::MoveRename, None, ctx),
            "shortcuts" => self.run(Cmd::Shortcuts, None, ctx),
            // The Help menu's windows and Download a gallery, for screenshots (#192). updates
            // asks GitHub, as Help → Check for updates does.
            "tell-friend" => self.run(Cmd::TellFriend, None, ctx),
            "updates" => self.run(Cmd::CheckUpdates, None, ctx),
            // The note a new version shows once after an update (notes.rs), with this version's
            // notes or the newest the CHANGELOG has.
            "whats-new" => self.whats_new = crate::notes::WhatsNew::latest(),
            "checksum" if first.is_some() => self.run(Cmd::VerifyChecksum, first, ctx),
            "gallery" => {
                let dir = self
                    .settings
                    .category_dir(Category::General, &self.downloads);
                self.gallery_dlg = Some(crate::gallerydlg::GalleryDialog::new(
                    dir,
                    "https://commons.wikimedia.org/wiki/Category:Sunsets",
                ));
            }
            "speedlimit" => self.run(Cmd::LimiterSettings, None, ctx),
            "statistics" => self.run(Cmd::Statistics, None, ctx),
            // The first download's, or `properties:<id>` that download's (screenshots of each
            // kind: a video, a file from a page, one not started yet).
            "properties" if first.is_some() => {
                let id = tab
                    .parse()
                    .ok()
                    .filter(|id| self.items.iter().any(|i| i.id == *id));
                self.run(Cmd::Properties, id.or(first), ctx);
            }
            "delete" if first.is_some() => self.run(Cmd::Remove, first, ctx),
            // The first download's Open, as its right-click menu's (#5: a finished one whose
            // file is gone asks File missing; one whose file is there opens it with its program).
            "open" if first.is_some() => self.run(Cmd::Open, first, ctx),
            // The last download's progress window, as its Show progress opens it (opened, or back
            // from the notification area).
            "progress" => {
                if let Some(id) = last {
                    self.show_progress(id);
                }
                // progress:peers and so on: a torrent's details tab (the docs' screenshots).
                if let Some(id) = last
                    && let Some(p) = self.progress.iter_mut().find(|p| p.id == id)
                {
                    p.show_tab(tab);
                }
            }
            // View → Theme → Add a theme file, as its dialog answers (#45):
            // `theme-add:<path>` (tests of a scheme's import without the system's dialog).
            "theme-add" => {
                self.add_theme_file(std::path::Path::new(tab));
            }
            // View → Theme → a theme, as the menu picks it (#45): `theme:native` (tests of the
            // live switch without a click).
            "theme" => {
                let id = crate::chrome::label(tab);
                self.run(Cmd::Theme(id), None, ctx);
            }
            // View → Theme → Choose a theme... (its screenshots); `accent:#107c10` picks an
            // accent for the theme in use as the picker does.
            "theme-picker" => self.run(Cmd::ThemePicker, None, ctx),
            "accent" => {
                let accent = crate::palette::parse_hex(tab);
                self.run(Cmd::Accent(accent), None, ctx);
            }
            "density" => {
                let d = if tab == "comfortable" {
                    crate::look::Density::Comfortable
                } else {
                    crate::look::Density::Compact
                };
                self.run(Cmd::Density(d), None, ctx);
            }
            // View → Toolbar → Neon / 3D style, as the menu picks it: `toolbar-icons:3d` (both
            // looks in one run of screenshots).
            "toolbar-icons" => {
                let style = match tab {
                    "3d" => crate::icons::ToolbarIcons::ThreeD,
                    _ => crate::icons::ToolbarIcons::Neon,
                };
                self.run(Cmd::ToolbarIcons(style), None, ctx);
            }
            // Downloads → Clean Up Temporary Files, as if chosen: its answer, for screenshots.
            "clean-up" => self.run(Cmd::CleanUpTemp, None, ctx),
            // A menu of the menu bar open (menu:0 is Tasks), for screenshots; menu:3:draw opens
            // View with its Draw with submenu (#89).
            "menu" => {
                self.popup = None;
                let (index, sub) = tab.split_once(':').unwrap_or((tab, ""));
                let index = index.parse().unwrap_or(0);
                self.open_menu = Some(index);
                // menu:3:theme opens View → Theme (#45), menu:3:size View → Interface size (#262),
                // menu:3:toolbar View → Toolbar, menu:3:tray View → Tray icon.
                let wanted = match sub {
                    "draw" => "Draw with",
                    "theme" => "Theme",
                    "size" => "&Interface size",
                    "toolbar" => "Toolbar",
                    "tray" => "DDM &tray icon",
                    "export" => "E&xport",
                    "import" => "&Import",
                    _ => "",
                };
                let at = self
                    .menu_entries(index)
                    .iter()
                    .position(|e| matches!(e, MenuEntry::Sub(name, ..) if *name == wanted));
                if let Some(at) = at {
                    chrome::open_sub_at(ctx, &format!("menu-{index}"), at);
                }
            }
            // Every download selected and the right-click menu open on them (Move/Rename, #333).
            "context" if tab == "all" => {
                self.run(Cmd::SelectAll, None, ctx);
                self.open_menu = None;
                if let Some(id) = self.shown_ids().first().copied() {
                    self.open_popup(Popup::Item(Some(id)), egui::pos2(360.0, 220.0));
                }
            }
            // The list header's right-click menu on column `header-menu:<slot>` (3: Status), and
            // `filter:<slot>` that column's filter box open, for screenshots.
            "header-menu" => {
                self.open_menu = None;
                let col = tab.parse().ok().filter(|c| *c < list::COLS);
                self.open_popup(Popup::Columns(col), egui::pos2(480.0, 140.0));
            }
            // `filter:<slot>:<text>` filters by that text instead (the filtered look).
            "filter" => {
                let (col, text) = tab.split_once(':').unwrap_or((tab, ""));
                if let Ok(c) = col.parse::<usize>()
                    && c < list::COLS
                {
                    if text.is_empty() {
                        self.list.open_filter(c);
                    } else {
                        self.list.filters[c] = Some(list::Filter::Text(text.to_owned()));
                    }
                }
            }
            // `context:<id>`: that download's right-click menu (Show progress on an unfinished
            // one), for screenshots.
            "context"
                if tab
                    .parse::<u64>()
                    .is_ok_and(|id| self.items.iter().any(|i| i.id == id)) =>
            {
                let id = tab.parse::<u64>().unwrap_or_default();
                self.open_menu = None;
                self.selection.set(id);
                self.open_popup(Popup::Item(Some(id)), egui::pos2(360.0, 220.0));
            }
            // A finished archive's right-click menu (Extract, #37), for screenshots.
            "context" => {
                let archive = self
                    .items
                    .iter()
                    .rev()
                    .find(|i| {
                        i.status == Status::Complete
                            && crate::extractq::archive_name(&i.file_name)
                            && mdm_extract::volume_key(&i.file_name).is_none_or(|v| v.is_first())
                    })
                    .or(self.items.last())
                    .map(|i| i.id);
                self.open_menu = None;
                if let Some(id) = archive {
                    self.selection.set(id);
                    self.open_popup(Popup::Item(Some(id)), egui::pos2(360.0, 220.0));
                }
            }
            // Site grabber's list of files found (its screenshots); site-grabber:start (save,
            // explore, run, files-early) on that page.
            "site-grabber" => self.grab_sample(tab),
            // Off-screen test copies: Site grabber on an address with the offline template, as
            // its Start does, then its Download all (tests/site_grab.rs).
            "site-grabber-run" if crate::offscreen() => self.grab_test_run(tab),
            "site-grabber-download" if crate::offscreen() => self.grab_test_download(),
            // Link grabber with a page's links, looked up (the docs' screenshots, #37).
            "grabber" => {
                self.show_import(Vec::new(), String::new());
                if let Some(g) = &mut self.grabber {
                    g.sample();
                }
            }
            "options" => {
                let page = match tab {
                    "types" | "addresses" => 1,
                    "saveto" => 2,
                    "downloads" | "progress" | "virus" => 3,
                    "connection" | "dns" | "exception" => 4,
                    "proxy" | "socks" => 5,
                    "logins" | "login" => 6,
                    "accounts" => 7,
                    "archives" => 10,
                    "sounds" => 8,
                    "regional" => 9,
                    "remote" | "pair" => 11,
                    _ => 0,
                };
                self.run(Cmd::Options, None, ctx);
                self.options = self.options.take().map(|o| o.on_page(page));
                self.raise_options = true;
                // General → Download panels → Edit... (the video button's settings).
                if tab == "panels" {
                    self.options = self.options.take().map(|o| o.with_panels());
                }
                // A window over a page (screenshots, #192): options:test, options:account.
                self.options = self.options.take().map(|o| o.with_window(tab));
                // Remote → Pair a device... (the docs' screenshots, #141).
                if tab == "pair" {
                    self.options = self.options.take().map(|o| o.with_pairing());
                }
            }
            _ => {}
        }
    }

    /// Open (or bring back) the File Properties of a download.
    pub(super) fn properties(&mut self, id: u64) {
        if self.properties.iter().any(|p| p.id == id) {
            dialogs::raise(
                &self.ui_ctx,
                dialogs::dialog_viewport(&format!("properties-{id}")),
            );
            return;
        }
        let route = self.debrid_route();
        if let Some(item) = self.items.iter().find(|i| i.id == id) {
            self.properties
                .push(crate::properties::Properties::new(item, route));
        }
    }

    /// File Properties → OK.
    pub(super) fn apply_properties(&mut self, c: crate::properties::Changes) {
        let Some(item) = self.item(c.id) else { return };
        item.description = c.description;
        item.login = c.login;
        item.context.referer = c.referer;
        // "Through …" changed (#44): the next start goes the new way. The service's link is
        // dropped from the resume state: directly, it continues from the host's own address;
        // through the account, from a new link the service gives. The size and validators
        // still have to match.
        let state = mdm_engine::state::state_path(&item.dest);
        if let Some(direct) = c.direct {
            item.direct = direct;
            item.note(if direct {
                // i18n: event log: File Properties: the download no longer goes through the debrid account
                crate::i18n::t("Set to download directly, not through the debrid account")
                    .to_owned()
            } else {
                // i18n: event log: File Properties: the download goes through the debrid account again
                crate::i18n::t("Set to download through the debrid account").to_owned()
            });
        }
        // The resume state follows "Through…" and Refresh download address (`follow_state`),
        // rewritten beside the window (#743: its folder may be on a drive slow to answer); the
        // row's next start waits for it, as for Redownload's removal (`StartLook::clearing`).
        if c.direct.is_some() || c.address.is_some() {
            let (direct, address) = (c.direct.is_some(), c.address.clone());
            let before = self.clearing.remove(&c.id);
            let (done, clearing) = std::sync::mpsc::channel();
            self.rt.spawn_blocking(move || {
                if let Some(before) = before {
                    let _ = before.recv();
                }
                follow_state(&state, direct, address.as_ref());
                let _ = done.send(());
            });
            self.clearing.insert(c.id, clearing);
        }
        let Some(item) = self.item(c.id) else { return };
        if let Some(url) = c.address {
            // Refresh download address: the resume state follows (above), so the download
            // continues from the new address (its size and validators still have to match).
            // Where the browser's download of the old address ended isn't this one's (#630).
            item.context.final_url = None;
            item.note(crate::i18n::fmt(
                crate::i18n::t("Address changed to {host}"),
                &[("host", &crate::i18n::ltr(url.host_str().unwrap_or("?")))],
            ));
            item.url = url;
            item.error = None;
            if item.status == Status::Error {
                item.status = Status::Stopped;
            }
        }
        // Moved into the folder it is already in: nothing to do (it became "a (1).zip"). Its
        // free name there is looked for beside the window (#743); it moves once found.
        let move_to = c
            .move_to
            .filter(|dir| item.dest.parent() != Some(dir.as_path()));
        let name = item.file_name.clone();
        self.dirty = true;
        if let Some(dir) = move_to {
            let id = c.id;
            self.ask_beside(move || {
                Beside::Moves(vec![(id, mdm_engine::naming::destination(&dir, &name))])
            });
        }
    }

    /// Download `id`'s progress window on the screen, as the person asked (the row's Show
    /// progress, the tray's Downloads in progress, a double-click set to Open): opened when it
    /// has none, back from the notification area when hidden there (#63), in front when behind.
    /// Whatever the progress window's start view (Options → Downloads → Customize).
    pub(super) fn show_progress(&mut self, id: u64) {
        if !self.progress.iter().any(|p| p.id == id) {
            let p = self.new_progress(id);
            self.progress.push(p);
        } else if let Some(p) = self.progress.iter_mut().find(|p| p.id == id && p.hidden) {
            // Hidden in the tray (#63): back, as it was.
            p.hidden = false;
        } else {
            // Open already, behind or minimized: to the front.
            dialogs::raise(
                &self.ui_ctx,
                egui::ViewportId::from_hash_of(("progress", id)),
            );
        }
    }

    /// The progress window of download `id`, just started or resumed by the person (Start
    /// Download, Resume, the browser's download with no dialog): opened as Options → Downloads
    /// → Customize "Download progress" dialog says (shown, minimized, or hidden in the
    /// notification area), and one hidden there shows again unless that start view is hidden.
    /// A queue's downloads open none: they start hidden, and Show progress or the tray brings
    /// one up.
    pub(super) fn open_started_progress(&mut self, id: u64) {
        let tray = self.tray_available();
        let opens = crate::progress::opens(self.settings.progress_view, tray);
        if let Some(p) = self.progress.iter_mut().find(|p| p.id == id) {
            if p.hidden && opens != crate::progress::Opens::Hidden {
                p.hidden = false;
            }
            return;
        }
        let mut p = self.new_progress(id);
        p.open_as(opens);
        self.progress.push(p);
    }

    /// A new progress window for download id, its tabs as Options → Downloads → Customize
    /// says.
    pub(super) fn new_progress(&self, id: u64) -> Progress {
        let s = &self.settings;
        Progress::new(id, s.show_complete)
            .with_tabs(s.progress_tab_limiter, s.progress_tab_completion)
    }

    /// A finished download's progress window closes, and Download complete opens for it (`early`:
    /// the download Download File Info started early, which stays quiet).
    pub(super) fn close_finished(&mut self, id: u64, early: Option<u64>) {
        // This file's "Show download complete dialog" (its progress window's Options on
        // completion tab) once changed there, read before the window closes; else Options' as it
        // is now (a "Don't show this again" meanwhile included).
        let window = self.progress.iter().find(|p| p.id == id);
        let this_file = match window.and_then(Progress::shows_complete) {
            Some(shown) => shown,
            // One of a batch (a grabber's) without a window of its own opens none as it ends.
            None if window.is_none() && self.batch_quiet(id) => false,
            None => self.settings.show_complete,
        };
        if let Some(w) = window.and_then(Progress::when_done) {
            self.when_done(w);
        }
        self.progress.retain(|p| p.id != id);
        if Some(id) == early {
            return;
        }
        // Notify when downloads complete: those whose Download complete window doesn't open.
        if self.settings.notify_complete
            && !this_file
            && let Some(i) = self.items.iter().find(|i| i.id == id)
        {
            self.tray_kept.notifier.done(&i.file_name, Instant::now());
        }
        if this_file
            && !self.complete.iter().any(|c| c.id == id)
            && let Some(i) = self.items.iter().find(|i| i.id == id)
        {
            self.complete.push(Complete::new(
                id,
                i.url.to_string(),
                i.dest.clone(),
                i.size.unwrap_or(i.downloaded),
                i.notices.clone(),
            ));
        }
    }

    /// Download File Info for an address from Add URL, the browser (`context`) or the command
    /// line. One at a time: another that arrives meanwhile waits its turn (the browser has
    /// already given its own copy up, so none may be dropped).
    pub(super) fn open_file_info(
        &mut self,
        new: dialogs::NewUrl,
        context: ipc::Command,
        ctx: &egui::Context,
    ) {
        self.open_file_info_with(new, context, None, ctx);
    }

    /// [`Self::open_file_info`]; `pick`: the one file of a Metalink read from a file (its
    /// first source is `new`'s address), which the dialog downloads with its mirrors and hashes
    /// (`FileInfo::apply_metalink`).
    pub(super) fn open_file_info_with(
        &mut self,
        new: dialogs::NewUrl,
        context: ipc::Command,
        pick: Option<crate::metalinkdlg::Pick>,
        ctx: &egui::Context,
    ) {
        if self.file_info.is_some() {
            self.file_info_waiting.push_back((new, context, pick));
            return;
        }
        let direct = crate::debrid::browser_started(&context);
        let request = self.request_parts(&new.url, new.login.as_ref(), &context, direct);
        let mut fi = FileInfo::new(
            new,
            context,
            request,
            self.debrid_route(),
            self.downloads.clone(),
            self.settings.category_dirs.clone(),
            &self.rt,
            ctx,
        );
        fi.remember = self.settings.change_folder_on_last;
        fi.folders = self.settings.folder_history.clone();
        if let Some(pick) = pick {
            fi.apply_metalink(pick, ctx);
        }
        self.file_info = Some(fi);
    }
}

/// File Properties' changes to a download's resume state at `state`, so that it continues as
/// they say: "Through…" changed (`direct`: the service's link dropped, it continues from the
/// host's own address or a new link), a new address (`address`). None there: nothing to change.
/// Reads and writes the disk: for a thread other than the window's (#743).
pub(super) fn follow_state(state: &std::path::Path, direct: bool, address: Option<&url::Url>) {
    use mdm_engine::state::ResumeState;
    if direct && let Ok(mut saved) = ResumeState::load(state) {
        saved.final_url = saved.url.clone();
        let _ = saved.write(state);
    }
    if let Some(url) = address
        && let Ok(mut saved) = ResumeState::load(state)
    {
        saved.url = url.clone();
        saved.final_url = url.clone();
        let _ = saved.write(state);
    }
}

/// The finished downloads whose progress windows close at `now` (then Download complete opens):
/// of the ones just `finished`, those without a window on screen (`shown`), at once. A window on
/// screen first shows its download whole for a moment ([`progress::SHOWN_WHOLE`]; its bar glides
/// a little behind the figures and would close short of the end): kept in `finishing` with when
/// it finished, it closes once that moment is over, unless the download was started again
/// meanwhile (no longer `complete`): its window stays for the new run.
pub(super) fn finished_windows(
    finished: Vec<u64>,
    shown: impl Fn(u64) -> bool,
    complete: impl Fn(u64) -> bool,
    finishing: &mut Vec<(u64, Instant)>,
    now: Instant,
) -> Vec<u64> {
    let mut close = Vec::new();
    for id in finished {
        if shown(id) {
            finishing.push((id, now));
        } else {
            close.push(id);
        }
    }
    finishing.retain(|&(id, at)| {
        let wait = now.duration_since(at) < progress::SHOWN_WHOLE;
        if !wait && complete(id) {
            close.push(id);
        }
        wait
    });
    close
}
