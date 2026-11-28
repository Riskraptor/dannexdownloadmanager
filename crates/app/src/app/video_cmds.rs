//! Videos, playlists and galleries: Download video's hand-offs, the browser's quality menu, their
//! folders read beside the window, and their starts through yt-dlp and gallery-dl.

use super::*;

/// Where a video's choice goes after the duplicate check: added now; kept, look-up and all,
/// for the user's answer, the look at the disk beside the window or the removal of the one it
/// replaces (#743); or (the one already there opened or resumed) dropped with the menu's
/// look-up, which only ever goes with the download born from its own choice.
pub(super) enum VideoRoute {
    Add(Box<crate::videodlg::VideoAdd>),
    Wait(Pending),
    Dropped,
}

/// A video being added (#743): its folder read beside the window, its name picked once the
/// names there are known (`App::poll_video_adds`).
pub(super) struct VideoAdding {
    pub(super) add: crate::videodlg::VideoAdd,
    pub(super) folder: PathBuf,
    pub(super) name: String,
    pub(super) names: std::sync::mpsc::Receiver<Vec<String>>,
}

/// A gallery being added (#743): whether its folder holds anything already, asked beside the
/// window (`App::poll_gallery_adds`).
pub(super) struct GalleryAdding {
    pub(super) add: crate::gallerydlg::GalleryAdd,
    pub(super) preexisting: std::sync::mpsc::Receiver<bool>,
}

/// Where a video just added goes ([`video_goes`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum VideoGoes {
    /// Downloading now, its progress window up.
    Start,
    /// Into the queue the selection panel asks for (Options → Downloads).
    AskQueue,
    /// Into the main queue, nothing asked.
    MainQueue,
}

impl App {
    /// Download video from a page, opened for a page (looked up at once) or from the menu: Save
    /// to is the Video category's folder, the Music category's while a sound-only choice is
    /// chosen (#611; not when the user deleted Music).
    /// Download video for `page`, the browser's button over a video (`cmd` its request): one
    /// window at a time, as Download File Info and Add torrent. A page handed over while it is
    /// open waits its turn (`video_waiting`), never taking the open window's place; the same
    /// page clicked again brings the open window up.
    pub(super) fn open_video_page(&mut self, page: String, cmd: ipc::Command) {
        if let Some(open) = &self.video_dlg {
            if open.opened_for(&page) {
                dialogs::raise(&self.ui_ctx, dialogs::dialog_viewport("video-page"));
            } else if !self.video_waiting.iter().any(|(p, _, _)| *p == page) {
                tracing::info!(
                    "Download video is open: a page waits ({} waiting)",
                    self.video_waiting.len() + 1
                );
                self.video_waiting.push_back((page, cmd, None));
            }
            return;
        }
        self.video_dlg = Some(self.video_dialog(Some((&page, cmd))));
    }

    pub(super) fn video_dialog(
        &self,
        page: Option<(&str, mdm_ipc::Command)>,
    ) -> crate::videodlg::VideoDialog {
        crate::videodlg::refresh_ytdlp(&self.rt);
        let dir = self.settings.category_dir(Category::Video, &self.downloads);
        let music = (!Category::Music.deleted())
            .then(|| self.settings.category_dir(Category::Music, &self.downloads));
        match page {
            Some((page, cmd)) => crate::videodlg::VideoDialog::for_page(dir, page, cmd),
            None => crate::videodlg::VideoDialog::new(dir),
        }
        .with_subs(self.settings.video_subs.clone())
        .with_music_folder(music)
    }

    /// Download video from a page → Download Now / Later ([`video_goes`]), through the
    /// duplicate check first ([`Self::add_video_checked`]). Its folder is read beside the
    /// window (#743): it is added once the names there are known (`poll_video_adds`).
    pub(super) fn add_video(&mut self, add: crate::videodlg::VideoAdd) {
        let folder = add
            .dest
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.downloads.clone());
        let name = add
            .dest
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "video".into());
        let (tx, names) = std::sync::mpsc::channel();
        let (dir, ctx) = (folder.clone(), self.ui_ctx.clone());
        // Files being removed there (Delete with its files…) are gone first (`Removing`).
        let waits = self.removals_in(Some(&folder), &Writes::Folder);
        self.rt.spawn_blocking(move || {
            for w in waits {
                let _ = w.recv();
            }
            let _ = tx.send(video_folder(&dir));
            ctx.request_repaint();
        });
        self.video_adds.push(VideoAdding {
            add,
            folder,
            name,
            names,
        });
    }

    /// The videos whose folders have been read (`add_video`), added in the order they came.
    pub(super) fn poll_video_adds(&mut self) {
        if self.video_adds.is_empty() {
            return;
        }
        let mut ready = Vec::new();
        for adding in std::mem::take(&mut self.video_adds) {
            match adding.names.try_recv() {
                Ok(names) => ready.push((adding, names)),
                Err(std::sync::mpsc::TryRecvError::Empty) => self.video_adds.push(adding),
                // The read ended without an answer: nothing known there.
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    ready.push((adding, Vec::new()))
                }
            }
        }
        for (adding, names) in ready {
            self.video_added(adding, &names);
        }
    }

    /// Video `adding` added, its folder holding `names` (lowercase, read beside the window).
    pub(super) fn video_added(&mut self, adding: VideoAdding, names: &[String]) {
        let VideoAdding {
            add, folder, name, ..
        } = adding;
        // yt-dlp takes any `<stem>.*` there as its own: a stem no file and no other download
        // in the folder has, whatever the extension (#32). The rows as they are now: one added
        // while the folder was read counts.
        let dest = video_place(&folder, &name, names, &self.items);
        let id = self.push_item(
            add.page,
            dest,
            add.category,
            String::new(),
            add.size,
            None,
            add.context,
            // i18n: how a download was added (its log: "Added from a video page"): a video from a web page
            crate::i18n::mark("from a video page"),
        );
        let goes = video_goes(add.start_now, self.settings.queue_panel_later);
        if let Some(item) = self.item(id) {
            item.size_approx = add.size_approx;
            item.video = Some(model::VideoSpec {
                selector: add.selector,
                subs: add.subs,
            });
            // Its page is its own address (`Item::web_page`): the browser's Referer is the page
            // that led to it (a search), not where the video is.
            item.page = None;
            // The quality menu's look-up goes with this new download only, started now and while
            // fresh: one queued for later, or added on a duplicate question answered late,
            // starts with a look-up of its own (the links will have expired).
            item.preloaded = kept_lookup(add.preloaded, goes, Instant::now());
        }
        self.selection.set(id);
        match goes {
            VideoGoes::Start => self.start(id, true),
            VideoGoes::AskQueue => self.queue_later(vec![id], true),
            VideoGoes::MainQueue => self.queue_later(vec![id], false),
        }
        self.dirty = true;
    }

    /// The browser's quality menu over a video (crates/nmhost, videolookup.rs): a look-up
    /// started (answered at once, yt-dlp on a thread of its own), a question about one, or the
    /// choice made there, which opens Download video with that quality chosen
    /// ([`Self::open_video_pick`]).
    pub(super) fn quality_menu(&mut self, cmd: &ipc::Command) -> ipc::api::VideoLookupAnswer {
        use ipc::api::{VideoLookupAnswer, lookup};
        let now = Instant::now();
        // A look-up asked ahead for a page since left: forgotten (stopped when nothing else
        // holds it), whatever else.
        if let Some(token) = &cmd.video_cancel {
            self.lookups.cancel(token, now);
            return VideoLookupAnswer::of(lookup::GONE);
        }
        if menu_needs_window(cmd, self.headless) {
            return VideoLookupAnswer::of(lookup::USE_WINDOW);
        }
        if let Some(ask) = &cmd.video_lookup {
            crate::videodlg::refresh_ytdlp(&self.rt);
            // yt-dlp and ffmpeg as last found in DDM's tools folder, read beside the window
            // (#743, `lookup_with`).
            return match lookup_with(crate::videodlg::found_tools(Some(&self.ui_ctx))) {
                Ok((exe, merge)) => self
                    .lookups
                    .start(ask, cmd, exe, merge, self.rt.clone(), now),
                Err(lookup::NO_YTDLP) => VideoLookupAnswer::failed(lookup::NO_YTDLP, None),
                Err(state) => VideoLookupAnswer::of(state),
            };
        }
        if let Some(token) = &cmd.video_poll {
            return self.lookups.poll(token, now);
        }
        let Some(pick) = &cmd.video_pick else {
            return VideoLookupAnswer::of(lookup::USE_WINDOW);
        };
        let Some(p) = self.lookups.pick(pick, cmd.private, now) else {
            return VideoLookupAnswer::of(lookup::USE_WINDOW);
        };
        self.open_video_pick(p);
        VideoLookupAnswer::of(lookup::SENT)
    }

    /// The quality menu's choice `p` in Download video, that quality chosen and its look-up kept
    /// (nothing looked up again): Save to, the name and the subtitles are there to check, and
    /// Download Now / Later add it, as for any video. Open for this page already, the window
    /// takes the quality chosen now and comes up; open for another page, it comes up and keeps
    /// its place, and this choice waits its turn as it is (never looked up again, so never a
    /// playlist's list): the latest choice for a page replaces one waiting for it. A new window
    /// comes in front of the browser by itself (`dialogs::window`, #208).
    pub(super) fn open_video_pick(&mut self, p: crate::videolookup::Picked) {
        match &self.video_dlg {
            Some(open) if !open.opened_for(&p.address) => {
                dialogs::raise(&self.ui_ctx, dialogs::dialog_viewport("video-page"));
                let (page, cmd) = (p.address.clone(), p.context.clone());
                if let Some(w) = self.video_waiting.iter_mut().find(|(w, _, _)| *w == page) {
                    *w = (page, cmd, Some(Box::new(p)));
                } else {
                    tracing::info!(
                        "Download video is open: a choice waits ({} waiting)",
                        self.video_waiting.len() + 1
                    );
                    self.video_waiting.push_back((page, cmd, Some(Box::new(p))));
                }
                return;
            }
            Some(_) => dialogs::raise(&self.ui_ctx, dialogs::dialog_viewport("video-page")),
            None => {}
        }
        let dlg = self
            .video_dialog(Some((&p.address, p.context.clone())))
            .picked(p);
        self.video_dlg = Some(dlg);
    }

    /// A playlist's videos (#69), one download each, into its folder: the folder is read once,
    /// beside the window (#743: a sleeping hard disk, a USB drive or a network folder can take
    /// seconds to answer), and they are added once the names there are known
    /// (`poll_playlist_adds`).
    pub(super) fn add_playlist(&mut self, add: crate::playlistdlg::PlaylistAdd) {
        let (tx, names) = std::sync::mpsc::channel();
        let (dir, ctx) = (add.folder.clone(), self.ui_ctx.clone());
        // Files being removed there (Delete with its files…) are gone first (`Removing`).
        let waits = self.removals_in(Some(&add.folder), &Writes::Folder);
        self.rt.spawn_blocking(move || {
            for w in waits {
                let _ = w.recv();
            }
            let _ = tx.send(mdm_engine::naming::names_in(&dir));
            ctx.request_repaint();
        });
        self.playlist_adds.push((add, names));
    }

    /// The playlists whose folders have been read (`add_playlist`), added in the order they came.
    pub(super) fn poll_playlist_adds(&mut self) {
        if self.playlist_adds.is_empty() {
            return;
        }
        let mut ready = Vec::new();
        for (add, names) in std::mem::take(&mut self.playlist_adds) {
            match names.try_recv() {
                Ok(there) => ready.push((add, there)),
                Err(std::sync::mpsc::TryRecvError::Empty) => self.playlist_adds.push((add, names)),
                // The read ended without an answer: nothing known there.
                Err(std::sync::mpsc::TryRecvError::Disconnected) => ready.push((add, Vec::new())),
            }
        }
        for (add, there) in ready {
            self.playlist_added(add, &there);
        }
    }

    /// Playlist `add`'s videos added, its folder holding `there` (lowercase, read beside the
    /// window): names picked from that and the rows as they are now (naming::Stems::among, never
    /// the disk), added at once and saved once, then queued (Download Now starts the main queue,
    /// so its "N at the same time" applies).
    pub(super) fn playlist_added(
        &mut self,
        add: crate::playlistdlg::PlaylistAdd,
        there: &[String],
    ) {
        let started = Instant::now();
        let folder = add.folder;
        let mut stems = playlist_stems(&folder, there, &self.items);
        self.items.reserve(add.entries.len());
        let mut ids = Vec::with_capacity(add.entries.len());
        for (page, title) in add.entries {
            let stem = mdm_engine::naming::sanitize(&title);
            let dest = stems.pick(&format!("{stem}.{}", add.ext));
            let id = self.push_item(
                page,
                dest,
                add.category,
                String::new(),
                None,
                None,
                add.context.clone(),
                // i18n: where a download came from, in its log: a playlist or channel, by yt-dlp
                // i18n: how a download was added (its log): one video of a playlist or a channel
                crate::i18n::mark("from a playlist"),
            );
            // push_item adds at the end: no search through the list for each one.
            if let Some(item) = self.items.last_mut() {
                item.video = Some(model::VideoSpec {
                    selector: add.selector.clone(),
                    subs: Default::default(),
                });
            }
            ids.push(id);
        }
        let Some(&first) = ids.first() else { return };
        self.selection.set(first);
        if add.start_now {
            self.queues[0].items.extend(ids.iter().copied());
            self.start_queue(0);
        } else {
            self.queue_later(ids.clone(), self.settings.queue_panel_later);
        }
        self.dirty = true;
        eprintln!(
            "playlist: {} downloads added in {} ms",
            ids.len(),
            started.elapsed().as_millis()
        ); // not translated: a log line
    }

    /// Start a video item: yt-dlp (installed and verified) finds it, DDM's engine downloads its
    /// streams (#769). Its place (a stem of its own if another row there has its stem, #442)
    /// and its folder are asked of the disk on the run's own thread as it starts
    /// (`StartLook::video`, `VideoJob::begin`; #743): the row runs from now, and follows.
    pub(super) fn start_video(&mut self, id: u64, show: bool) {
        // yt-dlp and ffmpeg as last found in DDM's tools folder, read beside the window (#743).
        // Not looked at yet (a moment after DDM starts): it starts once they are
        // (`poll_tools_wait`).
        let Some((_, tool, ffmpeg)) = crate::videodlg::found_tools(Some(&self.ui_ctx)) else {
            if !self.tools_wait.iter().any(|(i, _)| *i == id) {
                self.tools_wait.push((id, show));
            }
            return;
        };
        // No yt-dlp yet, and DDM is getting it by itself (autohelpers.rs): the video waits,
        // "Waiting" in the list, and starts once it is there (`poll_tools_wait`).
        if tool.is_none() && crate::autohelpers::gets_ytdlp() {
            let new = !self.tools_wait.iter().any(|(i, _)| *i == id);
            if new {
                self.tools_wait.push((id, show));
            }
            if let Some(item) = self.item(id)
                && (new || item.status != Status::Queued)
            {
                item.status = Status::Queued;
                item.error = None;
                if new {
                    item.note(crate::autohelpers::video_waits());
                }
                self.dirty = true;
            }
            return;
        }
        let rate = self.helper_starts(id);
        // The engine downloads the streams with what a file from the page gets (#769): Options'
        // connections, proxy, network adapter and DNS, the limit for all downloads (shared with
        // every download, not a helper's share), the hosts' learned limits; the video's own
        // speed limit, else the Speed Limiter's for one file. `rate` is yt-dlp's when it
        // downloads the video itself.
        let one_file = self.helper_limits(id).0;
        let engine = self.items.iter().find(|i| i.id == id).map(|i| {
            let mut opts = self.request_parts(&i.url, None, &i.context, true).opts;
            opts.speed_limit = one_file;
            mdm_tools::streams::Engine {
                handle: self.rt.clone(),
                opts,
            }
        });
        let Some(item) = self.item(id) else { return };
        let Some(spec) = item.video.clone() else {
            return;
        };
        let Some(tool) = tool else {
            item.status = Status::Error;
            item.error = Some(
                crate::i18n::t(
                    "yt-dlp is not installed (Tasks → Download video from a page installs it)",
                )
                .into(),
            );
            self.dirty = true;
            return;
        };
        let Some(look) = self.start_look(id) else {
            return;
        };
        let Some(item) = self.item(id) else { return };
        // The quality menu's look-up, for this first run only, while it is fresh (a run started
        // again, or later than the menu keeps its look-ups, reads the page afresh).
        let preloaded = item
            .preloaded
            .take()
            .filter(|p| p.slot.born().elapsed() < crate::videolookup::TTL);
        let started = mdm_tools::video::VideoJob::begin(
            &tool.exe,
            item.url.as_str(),
            &spec.selector,
            move || look.video(),
            rate,
            ffmpeg.as_ref().map(|f| f.exe.as_path()),
            item.size,
            &crate::videodlg::from_browser(&item.context),
            &spec.subs,
            preloaded,
            engine,
        );
        match started {
            Ok(job) => {
                // Which helper and which formats, for the log file; the download's own log says
                // what happened in plain words.
                tracing::info!(
                    "video {}: started with yt-dlp {} ({})",
                    item.id,
                    tool.version,
                    spec.selector
                );
                item.note(crate::i18n::t("Started"));
                let url = item.url.clone();
                item.live = Some(Live::new(model::Job::Video { job, url }, item.downloaded));
                item.status = Status::Downloading;
                item.error = None;
                item.last_try = Some(model::now_stamp());
            }
            Err(e) => {
                item.status = Status::Error;
                item.error = Some(e.to_string());
            }
        }
        self.dirty = true;
        if show {
            self.open_started_progress(id);
        }
    }

    /// The videos whose start waited for DDM's tools folder to be looked at a first time
    /// (`start_video`, #743), or for yt-dlp, which DDM was getting (autohelpers.rs): started
    /// once it is there, unless stopped by hand or taken off the list meanwhile.
    pub(super) fn poll_tools_wait(&mut self) {
        if self.tools_wait.is_empty() {
            return;
        }
        let ytdlp_there = match crate::videodlg::found_tools(None) {
            None => return,
            Some((_, ytdlp, _)) => ytdlp.is_some(),
        };
        if !ytdlp_there && crate::autohelpers::gets_ytdlp() {
            return;
        }
        for (id, show) in std::mem::take(&mut self.tools_wait) {
            if self.stopped_by_hand.contains(&id) || !self.items.iter().any(|i| i.id == id) {
                continue;
            }
            self.start(id, show);
        }
    }

    /// Download a gallery → Download Now / Later (#58): one row for the gallery, its folder its
    /// own. Whether that folder holds anything already (a rename of the gallery's folder then
    /// never takes along files that aren't its own: `Item::preexisting_folder`) is asked beside
    /// the window (#743: a sleeping drive or a network folder can take seconds to answer); the
    /// gallery is added once the answer comes (`poll_gallery_adds`).
    pub(super) fn add_gallery(&mut self, add: crate::gallerydlg::GalleryAdd) {
        let (tx, preexisting) = std::sync::mpsc::channel();
        let (dir, ctx) = (add.folder.clone(), self.ui_ctx.clone());
        self.rt.spawn_blocking(move || {
            let _ = tx.send(crate::deferred_rename::folder_has_any_entries(&dir));
            ctx.request_repaint();
        });
        self.gallery_adds.push(GalleryAdding { add, preexisting });
    }

    /// The galleries whose folders have been looked at (`add_gallery`), added as the answers
    /// come.
    pub(super) fn poll_gallery_adds(&mut self) {
        if self.gallery_adds.is_empty() {
            return;
        }
        let mut ready = Vec::new();
        for adding in std::mem::take(&mut self.gallery_adds) {
            match adding.preexisting.try_recv() {
                Ok(there) => ready.push((adding.add, there)),
                Err(std::sync::mpsc::TryRecvError::Empty) => self.gallery_adds.push(adding),
                // The look ended without an answer: as if the folder held something, so a
                // rename never takes along what may not be the gallery's.
                Err(std::sync::mpsc::TryRecvError::Disconnected) => ready.push((adding.add, true)),
            }
        }
        for (add, preexisting) in ready {
            self.gallery_added(add, preexisting);
        }
    }

    /// Gallery `add` added; its folder held something already (`preexisting`, looked at beside
    /// the window) or not.
    pub(super) fn gallery_added(&mut self, add: crate::gallerydlg::GalleryAdd, preexisting: bool) {
        let id = self.push_item(
            add.page,
            add.folder,
            Category::General,
            String::new(),
            None,
            None,
            Default::default(),
            // i18n: where a download came from, in its log: an image gallery, by gallery-dl
            // i18n: how a download was added (its log): an image gallery's files
            crate::i18n::mark("from a gallery"),
        );
        if let Some(item) = self.items.last_mut() {
            item.file_name = add.name;
            item.gallery = Some(model::GallerySpec { limit: add.limit });
            item.preexisting_folder = preexisting;
        }
        self.selection.set(id);
        if add.start_now {
            self.start(id, true);
        } else {
            self.queue_later(vec![id], self.settings.queue_panel_later);
        }
        self.dirty = true;
    }

    /// Start (or start again) a gallery: gallery-dl (installed and verified, or the person's)
    /// lists its files, DDM's engine downloads them (#769). gallery-dl is looked for in DDM's
    /// tools folder beside the window (#743: `current.json`, the program, or the person's own
    /// chosen in Helpers..., may be on a drive slow to answer), afresh each time (one installed a
    /// moment ago is found); the gallery starts once that answers ([`App::gallery_found`]).
    pub(super) fn start_gallery(&mut self, id: u64, show: bool) {
        if !self.gallery_looks.insert(id) {
            return;
        }
        self.ask_beside(move || Beside::GalleryTool {
            id,
            show,
            tool: mdm_tools::tools_dir()
                .as_deref()
                .and_then(mdm_tools::gallery::installed),
        });
    }

    /// [`App::start_gallery`] once gallery-dl has been looked for (`tool`): the gallery starts,
    /// unless it was stopped by hand, taken off the list or started meanwhile. Its folder is
    /// made, and the browser's cookie file written, on the job's own thread (#743).
    pub(super) fn gallery_found(
        &mut self,
        id: u64,
        show: bool,
        tool: Option<mdm_tools::Installed>,
    ) {
        self.gallery_looks.remove(&id);
        if self.stopped_by_hand.contains(&id)
            || !self
                .items
                .iter()
                .any(|i| i.id == id && i.gallery.is_some() && i.live.is_none())
        {
            return;
        }
        self.start_gallery_with(id, show, tool);
        self.share_with_new_helper(id);
    }

    pub(super) fn start_gallery_with(
        &mut self,
        id: u64,
        show: bool,
        tool: Option<mdm_tools::Installed>,
    ) {
        let rate = self.helper_starts(id);
        // The engine downloads the files with what a file from the page gets (#769), as a
        // video's streams: Options' connections (that many files at once), proxy, network
        // adapter and DNS, the limit for all downloads, the hosts' learned limits, the mark of
        // the web (the gallery's page as their source); the gallery's own speed limit, else
        // the Speed Limiter's for one file, for all its files together. `rate` is gallery-dl's
        // for the files it downloads itself.
        let one_file = self.helper_limits(id).0;
        let engine = self.items.iter().find(|i| i.id == id).map(|i| {
            let mut opts = self.request_parts(&i.url, None, &i.context, true).opts;
            opts.speed_limit = one_file;
            mdm_tools::streams::Engine {
                handle: self.rt.clone(),
                opts,
            }
        });
        let Some(item) = self.item(id) else { return };
        let Some(spec) = item.gallery.clone() else {
            return;
        };
        let Some(tool) = tool else {
            item.status = Status::Error;
            item.error = Some(
                crate::i18n::t(
                    // i18n: a gallery download's error: the helper program it needs is missing
                    "gallery-dl is not installed (Add URL with a gallery's address installs it)",
                )
                .into(),
            );
            self.dirty = true;
            return;
        };
        let ask = mdm_tools::gallery::Ask {
            limit: spec.limit,
            rate,
        };
        let started = mdm_tools::gallery::GalleryJob::start(
            &tool,
            item.url.as_str(),
            &item.dest,
            &ask,
            &crate::videodlg::from_browser(&item.context),
            engine,
        );
        match started {
            Ok(job) => {
                tracing::info!(
                    "gallery {}: started with gallery-dl {}",
                    item.id,
                    tool.version
                );
                item.note(crate::i18n::t("Started"));
                let url = item.url.clone();
                // A gallery counts what it saves this run.
                item.downloaded = 0;
                item.size = None;
                item.live = Some(Live::new(model::Job::Gallery { job, url }, 0));
                item.status = Status::Downloading;
                item.error = None;
                item.last_try = Some(model::now_stamp());
            }
            Err(e) => {
                item.status = Status::Error;
                item.error = Some(e.to_string());
            }
        }
        self.dirty = true;
        if show {
            self.open_started_progress(id);
        }
    }
}

/// A video already in the list as `page` in quality `selector` (another quality of the same page
/// is another download, not a duplicate).
pub(super) fn same_video(item: &Item, page: &url::Url, selector: &str) -> bool {
    item.url == *page && item.video.as_ref().is_some_and(|v| v.selector == selector)
}

pub(super) fn video_route(step: DupStep, add: crate::videodlg::VideoAdd) -> VideoRoute {
    match step {
        DupStep::Add => VideoRoute::Add(Box::new(add)),
        DupStep::Ask(_) | DupStep::Look(_) | DupStep::Replace(..) => {
            VideoRoute::Wait(Pending::Video(Box::new(add)))
        }
        DupStep::Existing(_) => VideoRoute::Dropped,
    }
}

/// The menu's look-up for a video being added: it goes with a download started now only, and
/// only while fresh ([`crate::videolookup::TTL`], as at its start): a question answered long
/// after, or a download queued, looks the page up afresh.
pub(super) fn kept_lookup(
    preloaded: Option<mdm_tools::video::Preloaded>,
    goes: VideoGoes,
    now: Instant,
) -> Option<mdm_tools::video::Preloaded> {
    preloaded.filter(|p| {
        goes == VideoGoes::Start
            && now.saturating_duration_since(p.slot.born()) < crate::videolookup::TTL
    })
}

/// What the browser's quality menu looks a video up with, as DDM's tools folder was last found
/// beside the window (`videodlg::found_tools`, #743): yt-dlp's program and whether ffmpeg is
/// there (the qualities that need merging offered); else its answer: not looked at yet (a
/// moment after DDM starts), or yt-dlp still coming (DDM gets it by itself, autohelpers.rs), the
/// Download video window does it (`use-window`: it shows yt-dlp coming and looks the page up
/// once it is there); no data folder to put it in, yt-dlp isn't installed (`no-ytdlp`). Never
/// touches the disk.
pub(super) fn lookup_with(
    found: Option<crate::videodlg::Tools>,
) -> Result<(PathBuf, bool), &'static str> {
    use ipc::api::lookup;
    match found {
        None | Some((Some(_), None, _)) => Err(lookup::USE_WINDOW),
        Some((None, None, _)) => Err(lookup::NO_YTDLP),
        Some((_, Some(tool), ffmpeg)) => Ok((tool.exe, ffmpeg.is_some())),
    }
}

/// A video's folder as it is added (`App::add_video`), read: the names there, lowercase
/// (`naming::names_in`). Looks at the disk: for a thread other than the window's (#743).
pub(super) fn video_folder(dir: &std::path::Path) -> Vec<String> {
    mdm_engine::naming::names_in(dir)
}

/// Where a video named `name` goes in `folder`, which holds `names` (`video_folder`): a stem no
/// file there and no other download of `items` in the folder has, whatever the extension
/// (yt-dlp takes any `<stem>.*` there as its own, #32). Never touches the disk.
pub(super) fn video_place(
    folder: &std::path::Path,
    name: &str,
    names: &[String],
    items: &[Item],
) -> PathBuf {
    mdm_engine::naming::stem_destination_among(folder, name, names, |stem| {
        items.iter().any(|i| {
            i.dest.parent() == Some(folder)
                && i.dest
                    .file_stem()
                    .is_some_and(|s| s.to_string_lossy().to_lowercase() == stem.to_lowercase())
        })
    })
}

/// Where a playlist's videos go in `folder`, which holds `there` (lowercase, read beside the
/// window: `naming::names_in`, #743): stems no file there and no download of `items` in the
/// folder has, each one handed out taken for the next (`naming::Stems`). Never touches the disk.
pub(super) fn playlist_stems(
    folder: &std::path::Path,
    there: &[String],
    items: &[Item],
) -> mdm_engine::naming::Stems {
    let in_use = items
        .iter()
        .filter(|i| i.dest.parent() == Some(folder))
        .filter_map(|i| i.dest.file_stem())
        .map(|s| s.to_string_lossy().into_owned());
    mdm_engine::naming::Stems::among(folder, there, in_use)
}

/// Where a video added goes: Download Now (`start_now`) starts it; Later asks which queue when
/// Options say so (`queue_panel`), else it goes into the main queue. Never in no queue at all,
/// which a cancelled panel would leave it.
pub(super) fn video_goes(start_now: bool, queue_panel: bool) -> VideoGoes {
    if start_now {
        VideoGoes::Start
    } else if queue_panel {
        VideoGoes::AskQueue
    } else {
        VideoGoes::MainQueue
    }
}
