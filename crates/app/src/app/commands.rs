//! The main window's commands (menus, toolbar, keys, the tray, the Categories tree): `Cmd`, and
//! what each one does (`App::run`).

use super::*;

/// Menu, toolbar and context-menu commands.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Cmd {
    AddUrl,
    /// Help → a page of the website (`""` = home).
    HelpPage(&'static str),
    Shortcuts,
    ReportProblem,
    CheckUpdates,
    About,
    TellFriend,
    /// Help → Support DDM: Dannex's donation page (GitHub Sponsors), in the browser.
    Sponsor,
    Exit,
    Stop,
    Remove,
    /// Shift+Delete: Remove with "Also delete the files from disk" ticked.
    RemoveWithFiles,
    Resume,
    StopAll,
    DeleteCompleted,
    /// Downloads → Clean Up Temporary Files: leftover part files no download owns (`leftovers`).
    CleanUpTemp,
    /// Start / stop a queue (index into `App::queues`).
    StartQueue(usize),
    StopQueue(usize),
    /// Queue right-click → Edit queue (its files) / Schedule; Delete; Create new queue.
    /// Tasks → Show drop target (shown / hidden); the drop target's Show main window.
    ToggleDropTarget,
    ShowMain,
    /// Tasks → Export / Import (D-021): IDM's export file (.ef2) or a text file.
    Export {
        ef2: bool,
    },
    Import {
        ef2: bool,
    },
    /// Download again from the start (File menu, right-click), after asking.
    Redownload,
    /// Move the file or rename it (right-click, Ctrl+M).
    MoveRename,
    /// Categories right-click (D-021): Add category, Properties, Delete (the user's only).
    AddCategory,
    CategoryProps(Category),
    DeleteCategory(Category),
    EditQueue(usize),
    ScheduleQueue(usize),
    DeleteQueue(usize),
    NewQueue,
    /// Categories right-click → Browse: open the category's folder.
    Browse(Category),
    /// List right-click → On Double click → Open (true) / Properties.
    DoubleClickOpen(bool),
    /// List right-click → Show progress: the download's progress window, opened or brought back
    /// from the notification area.
    ShowProgress,
    /// Right-click → Add to queue → queue index; Delete from queue.
    AddToQueue(usize),
    RemoveFromQueue,
    ToggleCategories,
    /// View → Follow the system's dark mode.
    ToggleDarkMode,
    /// View → Always dark (#60).
    ToggleAlwaysDark,
    /// View → Theme → a theme (#45): a built-in's id or a theme file's name.
    Theme(&'static str),
    /// View → Theme → Open themes folder.
    OpenThemesFolder,
    /// View → Theme → Choose a theme...: the theme picker.
    ThemePicker,
    /// View → Theme → Export this theme...: the theme in use as a theme file.
    ExportTheme,
    /// The theme picker: the theme's accent colour (None: its own).
    Accent(Option<egui::Color32>),
    /// View → Font → Compact rows / Comfortable rows (and the theme picker).
    Density(crate::look::Density),
    /// The theme picker: light, dark, or as the system is.
    Mode(crate::themepick::Mode),
    /// View → Theme → Add a theme file: a theme or a Base16 or pywal colour scheme, copied
    /// into the themes folder and picked.
    AddThemeFile,
    /// View → Draw with → OpenGL / Direct3D, Vulkan or Metal (#89, #206), from the next start.
    #[cfg(gpu)]
    DrawWith(crate::renderer::Api),
    /// View → Interface size → a size in percent (#262).
    InterfaceSize(u16),
    /// View → Text → Smooth (`true`) or Classic (Windows).
    #[cfg(windows)]
    SmoothText(bool),
    /// View → Font → a size for the download list's rows; Bold file names.
    ListFont(list::FontSize),
    ToggleBoldNames,
    /// View → Choose columns...: the columns' window.
    CustomizeColumns,
    /// View → Language: a catalog's code, or follow Windows (`None`).
    Language(Option<&'static str>),
    /// View → Language...: the language picker.
    ChooseLanguage,
    /// View → Toolbar → Large buttons (`false`) / Small buttons (`true`).
    ToolbarSmall(bool),
    /// View → Toolbar → Show text labels.
    ToggleToolbarLabels,
    /// View → Toolbar → Customize...: which buttons show, and in what order.
    CustomizeToolbar,
    /// View → Toolbar → Neon / 3D style: the buttons' icons.
    ToolbarIcons(crate::icons::ToolbarIcons),
    /// View → DDM tray icon → 3D style / Classic style (`Some`) / Don't show (`None`).
    TrayIcon(Option<crate::tray::TrayStyle>),
    /// View → Tray icon → Show download speed in the tray tooltip.
    ToggleTraySpeed,
    /// View → DDM tray icon (and the tray's menu) → Notify when downloads complete.
    ToggleNotify,
    /// View → Tray icon → Show progress on the taskbar button (Windows only: ITaskbarList3).
    #[cfg(windows)]
    ToggleTaskbarProgress,
    Open,
    OpenWith,
    OpenFolder,
    Properties,
    /// Show or hide a list column (data slot).
    ToggleColumn(usize),
    /// Downloads → Speed Limiter.
    LimiterOn,
    LimiterOff,
    LimiterSettings,
    /// Downloads → Statistics... (beyond IDM).
    Statistics,
    /// Downloads → Scheduler (also the toolbar button).
    Scheduler,
    /// Downloads → Options (also the toolbar button).
    Options,
    /// File → Add batch download (true: from the clipboard).
    Batch(bool),
    /// Tasks → Download video from a page… (yt-dlp, M9).
    VideoPage,
    /// Add torrent → Add magnet link… (M15): the Add torrent dialog, empty or with the
    /// clipboard's magnet.
    AddMagnet,
    /// Add torrent → Open torrent file…: pick a .torrent, then the Add torrent dialog.
    OpenTorrentFile,
    /// Tasks → Open Metalink file…: pick a .metalink or .meta4, then its file in Download File
    /// Info or its files in Add Metalink.
    OpenMetalinkFile,
    /// Add torrent → Torrent settings… (sharing after finishing, Windows associations).
    TorrentSettings,
    /// Right-click → Torrent (#39): read every piece again; ask the trackers again; add
    /// trackers (a window); copy its magnet link.
    TorrentRecheck,
    TorrentReannounce,
    /// Right-click → Torrent → Download in order, and its first and last pieces first (#39).
    TorrentInOrder,
    TorrentEndsFirst,
    TorrentAddTrackers,
    TorrentFiles,
    /// Right-click → Torrent → Play while downloading: opens the first playable torrent file.
    PlayTorrent,
    TorrentCopyMagnet,
    /// Tasks → RSS feeds… (also the Add Torrent button's arrow), #39: true opens it on its
    /// Download rules tab.
    RssFeeds(bool),
    /// Right-click → Refresh download address: open the source page, edit the address.
    RefreshAddress,
    /// Right-click → Download with the browser (#376), on a row the site's bot check refused.
    WithBrowser,
    /// Right-click → Verify checksum… (beyond IDM).
    VerifyChecksum,
    /// Right-click → Extract (beyond IDM, #37): a finished archive, into its folder.
    Extract,
    /// Tasks → Link grabber… (#37): the window, empty (links pasted in) or as it is.
    LinkGrabber,
    /// Tasks → Run site grabber…, and a grabber project's right-click commands.
    SiteGrabber,
    GrabProject(sitegrab::GrabAct),
    /// Downloads → Find (Ctrl-F) / Find Next (F3).
    Find,
    FindNext,
    /// View → Arrange files: sort by a column (data slot), or by order of addition.
    Arrange(Option<usize>),
    /// Header right-click → Clear all filters.
    ClearFilters,
    /// Every download the list shows (Ctrl+A).
    SelectAll,
    /// The All Downloads view (an empty list's link).
    ShowAll,
    /// Header right-click → Reset columns.
    ResetColumns,
    /// List header right-click → Fit columns to content: every shown column as wide as its title
    /// and cells need.
    FitColumns,
    /// List header right-click → Filter this column...: its filter box (the keyboard's way to it).
    FilterColumn(usize),
    None,
}

impl Cmd {
    /// The key that runs this command anywhere in the main window, as the shortcuts list writes
    /// it (Help → Keyboard shortcuts); menus show it beside the item.
    pub(super) fn hint(&self) -> Option<&'static str> {
        Some(match self {
            Cmd::AddUrl => "Ctrl+N",
            Cmd::Find => "Ctrl+F",
            Cmd::FindNext => "F3",
            Cmd::MoveRename => "Ctrl+M",
            Cmd::Remove => "Delete",
            Cmd::RemoveWithFiles => "Shift+Delete",
            Cmd::Open => "Enter",
            Cmd::SelectAll => "Ctrl+A",
            Cmd::Properties => "Alt+Enter",
            Cmd::InterfaceSize(100) => "Ctrl+0",
            _ => return None,
        })
    }
}

impl App {
    /// `target`: the right-clicked download, if any. Stop, Resume, Remove and the queue
    /// commands act on every selected download (or the right-clicked one outside the
    /// selection); the others on one download.
    pub(super) fn run(&mut self, cmd: Cmd, target: Option<u64>, ctx: &egui::Context) {
        // The log a crash report quotes: what was asked for (commands carry no addresses).
        tracing::info!("command {cmd:?}");
        let many = self.targets(target);
        let target = target.or(self.selection.one());
        let dest = target
            .and_then(|id| self.items.iter().find(|i| i.id == id))
            .map(|i| i.dest.clone());
        match cmd {
            Cmd::AddUrl => self.add_url = Some(AddUrl::from_clipboard(&self.settings.url_history)),
            Cmd::Exit => {
                self.exiting = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Cmd::Stop => {
                for id in many {
                    if self.item(id).is_some_and(|i| i.live.is_some()) {
                        self.stop_by_hand(id);
                    }
                }
            }
            Cmd::Remove => self.ask_delete(many, false),
            Cmd::RemoveWithFiles => self.ask_delete(many, true),
            Cmd::Resume => {
                for id in many {
                    if self.item(id).is_some_and(|i| i.can_start()) {
                        self.start(id, true);
                    }
                }
            }
            Cmd::StopAll => self.stop_all(),
            Cmd::DeleteCompleted => self.delete_completed(),
            Cmd::CleanUpTemp => self.sweep_leftovers(true),
            Cmd::StartQueue(q) => self.start_queue(q),
            Cmd::StopQueue(q) => self.stop_queue(q),
            Cmd::Browse(c) => {
                let dir = self.settings.category_dir(c, &self.downloads);
                // Made when it isn't there yet, then opened, beside the window (#743: it may be
                // on a drive slow to answer).
                crate::launch::spawn("browse", move || {
                    let _ = std::fs::create_dir_all(&dir);
                    crate::launch::open(dir);
                });
            }
            Cmd::DoubleClickOpen(open) => {
                self.settings.double_click_open = open;
                self.dirty = true;
            }
            Cmd::ShowProgress => {
                if let Some(id) = target {
                    self.show_progress(id);
                }
            }
            Cmd::AddToQueue(q) => {
                // "Move to queue" for a download already in one: it leaves that queue.
                let done: Vec<u64> = many
                    .iter()
                    .copied()
                    .filter(|id| {
                        self.items
                            .iter()
                            .any(|i| i.id == *id && i.status == Status::Complete)
                    })
                    .collect();
                for id in many {
                    // Finished files go only to synchronization queues.
                    if done.contains(&id) && !self.queues[q].sync {
                        continue;
                    }
                    for (k, other) in self.queues.iter_mut().enumerate() {
                        if k != q {
                            other.items.retain(|x| *x != id);
                        }
                    }
                    if !self.queues[q].items.contains(&id) {
                        self.queues[q].items.push(id);
                    }
                }
                self.dirty = true;
            }
            Cmd::RemoveFromQueue => {
                for id in many {
                    for q in &mut self.queues {
                        q.items.retain(|x| *x != id);
                    }
                }
                self.dirty = true;
            }
            Cmd::ToggleCategories => self.show_categories = !self.show_categories,
            Cmd::ChooseLanguage => {
                if self.lang_picker.is_none() {
                    self.lang_picker = Some(crate::langpick::Picker::new(
                        false,
                        self.settings.language.as_deref(),
                    ));
                }
            }
            Cmd::Language(code) => {
                self.settings.language = code.map(str::to_owned);
                self.settings.language_asked = true;
                crate::i18n::set_lang(code);
                crate::theme::language_fonts(ctx);
                if let Some(tray) = &self.tray {
                    tray.relabel();
                }
                self.dirty = true;
                ctx.request_repaint();
            }
            Cmd::ToggleDarkMode => {
                self.settings.dark_mode_support = !self.settings.dark_mode_support;
                self.theme_checked = None;
                self.dirty = true;
            }
            Cmd::ToggleAlwaysDark => {
                self.settings.always_dark = !self.settings.always_dark;
                self.theme_checked = None;
                self.dirty = true;
            }
            // View → Toolbar and View → Tray icon: applied from the next frame (the toolbar and
            // the panes under it, `tray`, the taskbar's progress).
            Cmd::ToolbarSmall(small) => {
                self.settings.toolbar_small = small;
                self.dirty = true;
            }
            Cmd::ToggleToolbarLabels => {
                self.settings.toolbar_labels = !self.settings.toolbar_labels;
                self.dirty = true;
            }
            Cmd::CustomizeToolbar => {
                if self.toolbar_dlg.is_none() {
                    self.toolbar_dlg = Some(crate::toolbar::Customize::new(
                        &self.settings.toolbar_buttons,
                    ));
                } else {
                    dialogs::raise(ctx, dialogs::dialog_viewport("toolbar-customize"));
                }
            }
            Cmd::ToolbarIcons(style) => {
                // Kept for the theme in use; the line or 3D style also for the themes without
                // icons of their own that have no choice yet.
                crate::look::look_mut(&mut self.settings.theme_looks, &self.settings.theme)
                    .toolbar = Some(style);
                if style != crate::icons::ToolbarIcons::Own {
                    self.settings.toolbar_icons = style;
                }
                self.dirty = true;
            }
            Cmd::ThemePicker => {
                if self.theme_picker.is_none() {
                    self.theme_picker = Some(crate::themepick::Picker::new(ctx, &self.theme_list));
                } else {
                    dialogs::raise(ctx, dialogs::dialog_viewport(crate::themepick::ID));
                }
            }
            Cmd::ExportTheme => {
                // i18n: title of the file dialog of View → Theme → Export this theme
                let title = crate::i18n::t("Export this theme").to_owned();
                // i18n: the kind of file View → Theme → Export this theme writes
                let kind = crate::i18n::t("DDM themes").to_owned();
                let name = format!("{}.toml", crate::theme::current().name);
                winui::start_pick(&self.ui_ctx, "app-theme-export", move || {
                    // Off-screen test runs have no file dialog to answer.
                    if let Some(path) =
                        std::env::var_os("MDM_TEST_EXPORT_TO").filter(|_| crate::offscreen())
                    {
                        return Some(PathBuf::from(path));
                    }
                    rfd::FileDialog::new()
                        .set_title(title)
                        .set_file_name(name)
                        .add_filter(kind, &["toml"])
                        .save_file()
                });
            }
            Cmd::Accent(accent) => {
                let id = self.settings.theme.clone();
                crate::look::look_mut(&mut self.settings.theme_looks, &id).accent =
                    accent.map(crate::palette::to_hex);
                crate::theme::set_accent(accent);
                crate::theme::select(&id, ctx);
                crate::theme::apply_theme(ctx);
                crate::theme::apply_mode(ctx);
                self.caption_set = false;
                self.theme_checked = None;
                self.dirty = true;
                ctx.request_repaint();
            }
            Cmd::Density(d) => {
                self.settings.density = d;
                self.dirty = true;
            }
            Cmd::Mode(mode) => {
                use crate::themepick::Mode;
                let s = &mut self.settings;
                (s.dark_mode_support, s.always_dark) = match mode {
                    Mode::Light => (false, false),
                    Mode::Dark => (s.dark_mode_support, true),
                    Mode::System => (true, false),
                };
                self.theme_checked = None;
                self.dirty = true;
            }
            Cmd::TrayIcon(style) => {
                self.settings.tray_icon = style.is_some();
                if let Some(style) = style {
                    self.settings.tray_style = style;
                }
                self.dirty = true;
            }
            Cmd::ToggleTraySpeed => {
                self.settings.tray_speed = !self.settings.tray_speed;
                self.dirty = true;
            }
            Cmd::ToggleNotify => {
                self.settings.notify_complete = !self.settings.notify_complete;
                self.dirty = true;
            }
            #[cfg(windows)]
            Cmd::ToggleTaskbarProgress => {
                self.settings.taskbar_progress = !self.settings.taskbar_progress;
                self.dirty = true;
            }
            Cmd::Theme(id) => {
                self.settings.theme = id.to_owned();
                // In the accent chosen for it, if one was (`look`).
                crate::theme::set_accent(crate::look::accent_of(&self.settings.theme_looks, id));
                crate::theme::select(id, ctx);
                // Live: egui's own visuals, the glyph rendering and the captions follow at once;
                // every painter reads the new theme from its next frame.
                crate::theme::apply_theme(ctx);
                crate::theme::apply_mode(ctx);
                self.caption_set = false;
                self.theme_checked = None;
                self.dirty = true;
                ctx.request_repaint();
            }
            Cmd::OpenThemesFolder => {
                if let Some(dir) = crate::themefile::dir() {
                    // As Browse: made and opened beside the window (#743).
                    crate::launch::spawn("themes folder", move || {
                        let _ = std::fs::create_dir_all(&dir);
                        crate::launch::open(dir);
                    });
                }
            }
            Cmd::AddThemeFile => {
                // i18n: title of the file dialog of View → Theme → Add a theme file
                let title = crate::i18n::t("Add a theme file").to_owned();
                // i18n: the kind of file View → Theme → Add a theme file lists: DDM's themes (TOML, JSON), Base16 schemes (YAML) and pywal's colors.json
                let kinds = crate::i18n::t("Themes and colour schemes").to_owned();
                let all = crate::i18n::t("All files").to_owned();
                let wal = crate::themefile::pywal_file();
                winui::start_pick(&self.ui_ctx, "app-theme-file", move || {
                    // Where pywal writes its colours, when it has: the first place to look
                    // (asked on the dialog's own thread, #743).
                    let start = wal
                        .filter(|p| p.is_file())
                        .and_then(|p| p.parent().map(std::path::Path::to_path_buf));
                    let dialog = rfd::FileDialog::new().set_title(title);
                    let dialog = match start {
                        Some(dir) => dialog.set_directory(dir),
                        None => dialog,
                    };
                    dialog
                        .add_filter(kinds, &["toml", "json", "yaml", "yml"])
                        .add_filter(all, &["*"])
                        .pick_file()
                });
            }
            // Every window at once (the frames after this one); saved by `follow_zoom`.
            Cmd::InterfaceSize(percent) => crate::scale::set(ctx, percent),
            #[cfg(windows)]
            Cmd::SmoothText(on) => {
                crate::theme::set_smooth_text(ctx, on);
                self.settings.smooth_text = on;
                self.dirty = true;
            }
            // The list takes them from the settings each time it is drawn.
            Cmd::ListFont(size) => {
                self.settings.list_font = size;
                self.dirty = true;
            }
            Cmd::ToggleBoldNames => {
                self.settings.list_bold_names = !self.settings.list_bold_names;
                self.dirty = true;
            }
            Cmd::CustomizeColumns => {
                if self.columns_dlg.is_none() {
                    self.columns_dlg = Some(crate::columnsdlg::CustomizeColumns::new(&self.list));
                } else {
                    dialogs::raise(ctx, dialogs::dialog_viewport(crate::columnsdlg::ID));
                }
            }
            #[cfg(gpu)]
            Cmd::DrawWith(api) => {
                // Chosen again after it failed here: its next start is tried.
                if Some(api) == crate::renderer::GPU {
                    crate::renderer::forget_failure();
                }
                tracing::info!("draw with {api:?} from the next start");
                self.settings.renderer = Some(api);
                self.dirty = true;
            }
            Cmd::Open => {
                if let Some(id) = target {
                    self.open_item(id);
                }
            }
            Cmd::Properties => {
                if let Some(id) = target {
                    self.properties(id);
                }
            }
            Cmd::Extract => {
                // Every chosen archive (a split one once, from whichever volume).
                for id in self.targets(target) {
                    if let Some(i) = self.items.iter().find(|i| i.id == id) {
                        self.extracts.offer(i, &self.items, &self.settings, true);
                    }
                }
            }
            Cmd::VerifyChecksum => {
                if let Some(id) = target
                    && !self.checksums.iter().any(|c| c.id == id)
                    && let Some(i) = self.items.iter().find(|i| i.id == id)
                {
                    let file = (i.status == Status::Complete).then(|| i.dest.clone());
                    self.checksums.push(crate::checksum::ChecksumDialog::new(
                        id,
                        i.checksum.as_ref(),
                        file,
                    ));
                }
            }
            Cmd::RefreshAddress => {
                // As IDM: the page the link came from opens, so a fresh link can be taken
                // there; Properties takes the new address and the download resumes from it.
                if let Some(id) = target {
                    let page = self
                        .items
                        .iter()
                        .find(|i| i.id == id)
                        .and_then(|i| i.context.referer.clone());
                    if let Some(page) = page {
                        crate::launch::open(page);
                    }
                    self.properties(id);
                }
            }
            Cmd::WithBrowser => {
                if let Some(id) = target {
                    self.download_with_browser(id);
                }
            }
            Cmd::OpenWith => {
                if let Some(d) = dest {
                    dialogs::open_with(&d);
                }
            }
            Cmd::OpenFolder => {
                // A finished download known to be gone: asked at once (#5).
                let missing = target
                    .and_then(|id| self.items.iter().find(|i| i.id == id))
                    .filter(|i| i.missing && i.status == Status::Complete)
                    .map(|i| (i.id, i.file_name.clone(), i.dest.clone()));
                if let Some((id, name, path)) = missing {
                    self.ask_missing(id, &name, &path);
                } else if let Some(d) = dest {
                    dialogs::show_in_folder(&d);
                }
            }
            Cmd::ToggleColumn(i) => {
                let on = !self.list.is_shown(i);
                self.list.set_visible(i, on);
            }
            Cmd::LimiterOn | Cmd::LimiterOff => {
                self.settings.speed_limit_on = cmd == Cmd::LimiterOn;
                self.apply_speed_limit();
                self.dirty = true;
            }
            Cmd::LimiterSettings => {
                self.speed_settings = Some(dialogs::SpeedSettings::new(&self.settings));
            }
            Cmd::Statistics => {
                if self.statistics.is_none() {
                    self.statistics = Some(Default::default());
                } else {
                    dialogs::raise(ctx, dialogs::dialog_viewport("statistics"));
                }
            }
            Cmd::Find => {
                if self.find.is_none() {
                    self.find = Some(crate::find::Find::new(self.last_find.as_ref()));
                } else {
                    dialogs::raise(ctx, dialogs::dialog_viewport("find"));
                }
            }
            Cmd::FindNext => match self.last_find.clone() {
                Some(q) => self.find_next(&q),
                None => self.find = Some(crate::find::Find::new(None)),
            },
            Cmd::VideoPage => {
                if self.video_dlg.is_none() {
                    self.video_dlg = Some(self.video_dialog(None));
                }
            }
            Cmd::AddMagnet => self.open_torrent_dialog(None, ctx),
            Cmd::HelpPage(page) => crate::help::open_page(page),
            Cmd::Shortcuts => self.shortcuts = Some(crate::help::Shortcuts),
            Cmd::ReportProblem => crate::help::report_problem(),
            Cmd::CheckUpdates => {
                if self.updates.is_none() {
                    self.updates = Some(crate::help::Updates::check(&self.rt, ctx));
                }
            }
            Cmd::About => self.about = Some(crate::help::About),
            Cmd::Sponsor => crate::help::sponsor(),
            Cmd::TellFriend => {
                if self.tell_friend.is_none() {
                    self.tell_friend = Some(crate::help::TellFriend::new());
                }
            }
            Cmd::TorrentSettings => {
                if self.torrent_settings.is_none() {
                    self.torrent_settings =
                        Some(crate::torrentdlg::TorrentSettings::new(&self.settings));
                }
            }
            Cmd::RssFeeds(rules) => {
                if self.rss_dlg.is_none() {
                    self.rss_dlg = Some(crate::rssdlg::RssWindow::new(&self.rss, rules));
                }
            }
            Cmd::TorrentRecheck => {
                if let Some(id) = target {
                    self.torrent_act(id, crate::torrents::Act::Recheck);
                }
            }
            Cmd::TorrentReannounce => {
                if let Some(id) = target {
                    self.torrent_act(id, crate::torrents::Act::Reannounce);
                }
            }
            Cmd::TorrentInOrder | Cmd::TorrentEndsFirst => {
                if let Some(id) = target {
                    self.toggle_torrent_order(id, matches!(cmd, Cmd::TorrentInOrder));
                }
            }
            Cmd::TorrentAddTrackers => {
                let torrent = |i: &&Item| i.torrent.is_some();
                if let Some(i) = target
                    .and_then(|id| self.items.iter().find(|i| i.id == id))
                    .filter(torrent)
                {
                    self.add_trackers =
                        Some(crate::torrentdlg::AddTrackers::new(i.id, &i.file_name));
                }
            }
            Cmd::TorrentFiles => {
                if let Some(item) = target.and_then(|id| self.items.iter().find(|i| i.id == id)) {
                    self.torrent_files =
                        crate::torrentfiles::TorrentFiles::new(item, &self.torrent_host);
                }
            }
            Cmd::PlayTorrent => {
                if let Some(id) = target {
                    self.play_torrent(id);
                }
            }
            Cmd::TorrentCopyMagnet => {
                // With its trackers, from the .torrent kept for it; else the list's address.
                let found = target.and_then(|id| self.items.iter().find(|i| i.id == id));
                if let Some((spec, url)) = found.and_then(|i| i.torrent.as_ref().zip(Some(&i.url)))
                {
                    let link =
                        crate::torrents::magnet_of(&spec.hash).unwrap_or_else(|| url.to_string());
                    crate::copy_text(ctx, link);
                }
            }
            Cmd::OpenTorrentFile => {
                let kind = crate::i18n::t("Torrent files").to_owned();
                let title = crate::i18n::t("Open torrent file").to_owned();
                winui::start_pick(ctx, "app-torrent-file", move || {
                    rfd::FileDialog::new()
                        .add_filter(kind, &["torrent"])
                        .set_title(title)
                        .pick_file()
                });
            }
            Cmd::OpenMetalinkFile => {
                // i18n: the kind of file the Open dialog lists (.metalink, .meta4)
                let kind = crate::i18n::t("Metalink files").to_owned();
                // i18n: title of the file dialog of Tasks → Open Metalink file
                let title = crate::i18n::t("Open Metalink file").to_owned();
                winui::start_pick(ctx, "app-metalink-file", move || {
                    rfd::FileDialog::new()
                        .add_filter(kind, &["metalink", "meta4"])
                        .set_title(title)
                        .pick_file()
                });
            }
            Cmd::Batch(clipboard) => {
                if self.batch.is_none() {
                    self.batch = Some(crate::batch::Batch::new(clipboard));
                }
            }
            Cmd::Options => {
                if self.options.is_none() {
                    self.options = Some(
                        crate::options::OptionsDialog::new(
                            &self.settings,
                            &self.downloads,
                            self.host_limits.learned(),
                        )
                        .with_remote(self.remote.clone()),
                    );
                } else {
                    dialogs::raise(ctx, dialogs::dialog_viewport("options"));
                }
            }
            Cmd::Scheduler => {
                if self.scheduler.is_none() {
                    self.scheduler = Some(crate::scheduler::Scheduler::new(
                        &self.queues,
                        &self.settings,
                    ));
                } else {
                    dialogs::raise(ctx, dialogs::dialog_viewport("scheduler"));
                }
            }
            Cmd::EditQueue(_) | Cmd::ScheduleQueue(_) | Cmd::NewQueue => {
                let s = self.scheduler.get_or_insert_with(|| {
                    crate::scheduler::Scheduler::new(&self.queues, &self.settings)
                });
                match cmd {
                    // Edit queue: its files; Schedule: when it runs.
                    Cmd::EditQueue(q) => s.show_queue(q, true),
                    Cmd::ScheduleQueue(q) => s.show_queue(q, false),
                    _ => s.new_queue(),
                }
            }
            // A dialog already open (half filled in) stays.
            Cmd::AddCategory => {
                if self.category_dlg.is_none() {
                    self.category_dlg = Some(crate::categories::CategoryDialog::new());
                }
            }
            Cmd::CategoryProps(c) => {
                let folder = self.settings.category_dir(c, &self.downloads);
                self.category_dlg = Some(crate::categories::CategoryDialog::edit(c, &folder));
            }
            // The user's categories and the built-in ones by file type (#93); not General nor
            // Torrents.
            Cmd::DeleteCategory(c) if c.editable() => {
                self.confirm_category_delete = Some(dialogs::DeleteCategory::new(c));
            }
            Cmd::DeleteCategory(_) => {}
            Cmd::ToggleDropTarget => {
                self.settings.drop_target = !self.settings.drop_target;
                self.dirty = true;
            }
            Cmd::ShowMain => self.show_window(ctx),
            Cmd::Export { ef2 } => self.export(ef2),
            Cmd::Import { ef2 } => self.import(ef2),
            Cmd::SiteGrabber => self.grab_cmd(None, ctx),
            Cmd::GrabProject(act) => self.grab_cmd(Some(act), ctx),
            Cmd::LinkGrabber => {
                self.show_import(Vec::new(), String::new());
                dialogs::raise(ctx, dialogs::dialog_viewport("grabber"));
            }
            Cmd::Redownload => {
                if let Some(id) = target
                    && self.can_redownload(id)
                    && let Some(item) = self.items.iter().find(|i| i.id == id)
                {
                    let text = crate::i18n::fmt(
                        crate::i18n::t(
                            "Download \"{name}\" again from the start? A finished file is replaced when the new download completes.",
                        ),
                        &[("name", &item.file_name)],
                    );
                    // i18n: window title, verb: download the file again from the start
                    let title = crate::i18n::mark("Redownload");
                    self.confirm_redownload = Some((id, dialogs::Confirm::new(title, text)));
                }
            }
            Cmd::MoveRename => {
                if many.len() > 1 {
                    self.move_many(many);
                } else if let Some(id) = target {
                    self.move_rename(id);
                }
            }
            Cmd::DeleteQueue(q) => {
                if q >= 2 && q < self.queues.len() {
                    let name = self.queues[q].name.clone();
                    self.confirm_queue_delete = Some((
                        q,
                        dialogs::Confirm::new(
                            crate::i18n::mark("Delete queue"),
                            crate::i18n::fmt(
                                crate::i18n::t(
                                    "Delete the queue \"{queue}\"? Its downloads stay in the list.",
                                ),
                                &[("queue", &name)],
                            ),
                        ),
                    ));
                }
            }
            Cmd::Arrange(col) => self.list.sort = col.map(|c| (c, true)),
            Cmd::ClearFilters => self.list.filters = vec![None; list::COLS],
            Cmd::ResetColumns => self.list.reset_columns(),
            Cmd::FitColumns => self.list.fit_columns(),
            Cmd::FilterColumn(c) => self.list.open_filter(c),
            Cmd::ShowAll => self.view = View::All,
            Cmd::SelectAll => {
                let ids = self.shown_ids();
                self.selection.apply(crate::selection::Pick::All, &ids);
            }
            Cmd::None => {}
        }
    }
}
