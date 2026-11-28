//! Download File Info: the address probed (or a Metalink read), the name and folder chosen (Save
//! As's place, looked at beside the window), Start / Later / Cancel.

use super::*;

// ---- Download File Info (client 570 × 208) -------------------------------------------------

/// What Download File Info asks the disk about Save As (#743).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlaceAsk {
    pub dir: PathBuf,
    pub name: String,
    /// A name DDM gave: numbered on when it is taken there (`naming::destination`). A typed one
    /// only has its folder looked at (its download is numbered as it starts if it is taken).
    pub number: bool,
}

/// The disk's answer: where the download goes, and whether its folder is there.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Found {
    pub dest: PathBuf,
    pub folder: bool,
}

/// How the disk is looked at: [`look`], or a test's slow one.
pub(crate) type Look = Arc<dyn Fn(&PlaceAsk) -> Found + Send + Sync>;

/// Save As's place, looked at off the window's thread (#743, #751): a sleeping hard disk, a USB
/// drive or a network folder can take seconds to answer whether a name is taken, and the window
/// waited for it (the freeze report caught 8 s). Save As shows the name as given meanwhile
/// (fitted, without " (1)"), and takes the numbered one when the answer comes, unless it was
/// typed over. A download started before the answer is numbered as it starts
/// (`Destination::Free`, `Item::number_at_start`): nothing is overwritten either way.
pub(crate) struct SaveAsPlace {
    pub(super) look: Look,
    pub(super) repaint: Option<egui::Context>,
    /// The last question, Save As's text it is for (as the answer set it, once taken), and its
    /// answer to come. A new question asks on a thread of its own: one that hangs on a dead
    /// share holds up nothing else.
    pub(super) asking: Option<(PlaceAsk, String, crate::launch::Asked<PlaceAsk, Found>)>,
}

impl SaveAsPlace {
    /// Looked at on the disk; `repaint` shows the answer when it comes.
    pub fn new(repaint: Option<&egui::Context>) -> Self {
        Self::with(Arc::new(look), repaint)
    }

    /// Looked at with `look` (a test's).
    pub fn with(look: Look, repaint: Option<&egui::Context>) -> Self {
        SaveAsPlace {
            look,
            repaint: repaint.cloned(),
            asking: None,
        }
    }

    /// `name` in `dir` as DDM names it: Save As's text for now (fitted, not numbered: no wait
    /// for the disk), its free numbered name asked for.
    pub fn name_in(&mut self, dir: &Path, name: &str) -> String {
        let shown = display_path(&mdm_engine::naming::guess(dir, name));
        let ask = PlaceAsk {
            dir: dir.to_owned(),
            name: name.to_owned(),
            number: true,
        };
        self.ask(ask, &shown);
        shown
    }

    pub(super) fn ask(&mut self, ask: PlaceAsk, shown: &str) {
        if self
            .asking
            .as_ref()
            .is_some_and(|(a, s, _)| *a == ask && s == shown)
        {
            return;
        }
        let asked = crate::launch::Asked::default();
        let look = self.look.clone();
        // Never waits: the answer is taken on a later frame (`follow`).
        let _ = asked.get(
            &ask,
            std::time::Duration::MAX,
            std::time::Duration::ZERO,
            self.repaint.as_ref(),
            move |a| look(a),
        );
        self.asking = Some((ask, shown.to_owned(), asked));
    }

    /// Each frame, never waiting: once the answer has come for what Save As (`save_as`) still
    /// shows, a name DDM gave takes its free numbered name; whether its folder is there. None
    /// while it hasn't come, or when Save As was typed over since.
    pub fn follow(&mut self, save_as: &mut String) -> Option<bool> {
        let (ask, shown, asked) = self.asking.as_mut()?;
        if shown != save_as {
            return None;
        }
        let found = asked.answer(ask)?;
        if ask.number {
            *save_as = display_path(&found.dest);
            shown.clone_from(save_as);
        }
        Some(found.folder)
    }

    /// Whether Save As's folder is there, as looked at for the text it shows (None: not known
    /// yet). A path typed in Save As is asked about here, as it is (its folder only).
    pub fn folder(&mut self, save_as: &mut String) -> Option<bool> {
        if let Some(there) = self.follow(save_as) {
            return Some(there);
        }
        if self.asking.as_ref().is_some_and(|(_, s, _)| s == save_as) {
            return None; // asked: the answer is on its way
        }
        let dest = resolve_path(save_as);
        let (Some(dir), Some(name)) = (dest.parent(), dest.file_name()) else {
            return None;
        };
        let ask = PlaceAsk {
            dir: dir.to_owned(),
            name: name.to_string_lossy().into_owned(),
            number: false,
        };
        self.ask(ask, &save_as.clone());
        self.follow(save_as)
    }
}

pub struct FileInfo {
    pub url: Url,
    pub login: Option<(String, String)>,
    /// Shown relative to the user profile, like IDM (`Downloads\Video\a.mp4`).
    pub save_as: String,
    pub category: Category,
    pub description: String,
    /// "Always save … files to this folder"; starts as Options → Save to → "Change folder for
    /// … category on last selected" says.
    pub remember: bool,
    pub(super) url_text: String,
    /// The URL box holds no address DDM downloads: Start and Later wait.
    pub(super) url_bad: bool,
    /// Save As has been given the focus (once, when the dialog opens).
    pub(super) focused: bool,
    /// To look an edited address up again, with the headers and options the download's own
    /// requests carry (app::RequestParts).
    pub(super) rt: tokio::runtime::Handle,
    pub(super) request: crate::app::RequestParts,
    pub(super) probe: Arc<Mutex<Option<Probed>>>,
    pub(super) info: Option<Result<ProbeInfo, String>>,
    /// A `.metalink` / `.meta4` URL is read first; its one file is then probed just like a
    /// normal address. The descriptor itself is never put in the downloads list.
    pub(super) metalink: Option<Arc<Mutex<Option<MetalinkRead>>>>,
    pub(super) metalink_error: Option<String>,
    /// Alternate locations and the publisher's expected hashes from the Metalink's file.
    pub(super) mirrors: Vec<Url>,
    pub(super) checksum: Option<mdm_engine::verify::Hash>,
    pub(super) pieces: Option<mdm_engine::verify::Pieces>,
    /// A Metalink of several files, read: for Add Metalink. The dialog closes, and the app
    /// opens that window with it (`App::open_metalink`).
    pub handoff: Option<crate::metalinkdlg::Opened>,
    /// The look's open answer, for the download to read instead of asking again (#28).
    pub(super) kept: Option<mdm_engine::KeptAnswer>,
    pub(super) edited: bool,
    pub(super) downloads: PathBuf,
    /// Options → Save to folders per category (label, folder).
    pub(super) dirs: Vec<(String, String)>,
    pub(super) category_open: bool,
    /// Browser request context for the download this dialog adds.
    pub context: mdm_ipc::Command,
    /// Start or Later held while the folder is looked at, and asked to be created when missing.
    pub(super) asking: FolderGate<Start>,
    /// The download already running while the dialog shows (Options → Downloads → "Start
    /// downloading immediately while displaying Download File Info dialog").
    pub early: Option<u64>,
    /// The folders downloads were saved to before, newest first (the Save As drop list).
    pub folders: Vec<String>,
    pub(super) folders_open: bool,
    /// "+" was clicked: the app opens the new-category window.
    pub new_category: bool,
    /// The downloads waiting their turn behind this one (a browser selection's arrive one by
    /// one): Cancel asks whether they are cancelled too (#394). The app sets it each frame.
    pub waiting: usize,
    /// Cancel was pressed while some waited: the question is up, this dialog waits for it.
    pub(super) cancel_ask: bool,
    /// The question's answer was Yes: the waiting ones are cancelled with this one (`Closed`
    /// comes with it; the app clears the waiting ones).
    pub cancel_all: bool,
    /// The debrid accounts (#44): which one this address goes through, if any.
    pub(super) route: crate::debrid::Route,
    /// The account that takes the address ("Real-Debrid (labuser)"), as last looked at: the
    /// "Download through" line shows while there is one.
    pub(super) through: Option<String>,
    /// "Download through …" unticked: downloaded directly instead (`Item::direct`). Unticked
    /// from the start when the browser had already started the download.
    pub direct: bool,
    /// Whether Save As names a folder, looked at off the window's thread: it is asked every
    /// frame, and a folder on a network share that doesn't answer would freeze the window.
    pub(super) dest_dir: crate::launch::Asked<PathBuf, bool>,
    /// Save As's name numbered in its folder, and whether the folder is there, looked at off the
    /// window's thread (#743).
    pub(super) place: SaveAsPlace,
    /// When it opened: the log says how long the address took to answer (#3).
    pub opened: std::time::Instant,
}

pub enum Start {
    Now,
    Later,
}

/// Download File Info's size line under the file's icon: the most it may take (px), centred on
/// x 515 between the boxes' right edge and the window's.
pub(super) const SIZE_COLUMN: f32 = 88.0;

impl FileInfo {
    /// Opens the dialog and looks the URL up in the background (name, size), like IDM, with
    /// `request`'s headers (the download's own). A download from the browser needs no look-up:
    /// the browser has had the answer, and its name (a link's `download` name, a Unicode
    /// `filename*`) is the one it checked; asking again could get another.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        new: NewUrl,
        context: mdm_ipc::Command,
        request: crate::app::RequestParts,
        route: crate::debrid::Route,
        downloads: PathBuf,
        dirs: Vec<(String, String)>,
        rt: &tokio::runtime::Handle,
        ctx: &egui::Context,
    ) -> Self {
        let direct = crate::debrid::browser_started(&context);
        let through = route.through(&new.url);
        let from_browser = context
            .name
            .as_deref()
            .map(mdm_engine::naming::sanitize)
            .filter(|n| !n.is_empty());
        let name = from_browser
            .clone()
            .unwrap_or_else(|| mdm_engine::naming::file_name(None, &new.url));
        let category = Category::of(&name);
        let folder = crate::model::category_folder(category, &downloads, &dirs);
        let mut place = SaveAsPlace::new(Some(ctx));
        let save_as = place.name_in(&folder, &name);
        let metalink = crate::metalinkdlg::is_metalink_url(
            &new.url,
            context.name.as_deref(),
            context.mime.as_deref(),
        )
        .then(|| start_metalink(rt, ctx, &new.url, &request));
        let (probe, info) = if metalink.is_some() {
            (Arc::new(Mutex::new(None)), None)
        } else {
            match from_browser {
                Some(file_name) => (
                    Arc::new(Mutex::new(None)),
                    Some(Ok(ProbeInfo {
                        final_url: context
                            .final_url
                            .as_deref()
                            .and_then(|u| Url::parse(u).ok())
                            .unwrap_or_else(|| new.url.clone()),
                        size: context.size,
                        file_name,
                        // The browser had the answer: nothing was asked of the server here.
                        server: None,
                    })),
                ),
                None => (start_probe(rt, ctx, &new.url, &request), None),
            }
        };
        FileInfo {
            dest_dir: Default::default(),
            place,
            url_text: new.url.to_string(),
            url_bad: false,
            focused: false,
            rt: rt.clone(),
            request,
            url: new.url,
            login: new.login,
            save_as,
            category,
            description: String::new(),
            remember: false,
            probe,
            info,
            metalink,
            metalink_error: None,
            mirrors: Vec::new(),
            checksum: None,
            pieces: None,
            handoff: None,
            kept: None,
            edited: false,
            downloads,
            dirs,
            category_open: false,
            context,
            asking: FolderGate::default(),
            early: None,
            folders: Vec::new(),
            folders_open: false,
            new_category: false,
            waiting: 0,
            cancel_ask: false,
            cancel_all: false,
            route,
            through,
            direct,
            opened: std::time::Instant::now(),
        }
    }

    /// Look the address up again as it will now be downloaded: through the debrid account
    /// or directly (#44), its name and size can differ.
    pub(super) fn look_again(&mut self, ctx: &egui::Context) {
        self.request.opts.unlocker = self.route.unlocker(&self.url, self.direct);
        self.through = self.route.through(&self.url);
        self.probe = start_probe(&self.rt, ctx, &self.url, &self.request);
        self.info = None;
        // The old way's answer is closed.
        self.kept = None;
    }

    /// Takes a completed Metalink read. One file: its first source is this dialog's address,
    /// looked up as any address is, the others its mirrors, its hashes checked. Several: they
    /// go to Add Metalink (`handoff`; the app closes this dialog and opens that window).
    pub(super) fn follow_metalink(&mut self, ctx: &egui::Context) {
        let Some(look) = &self.metalink else { return };
        let Some(result) = look.lock().unwrap().take() else {
            return;
        };
        self.metalink = None;
        match result {
            Ok(manifest) if manifest.files.len() > 1 => {
                self.handoff = Some(crate::metalinkdlg::Opened {
                    manifest,
                    origin: self.url.to_string(),
                });
            }
            Ok(mut manifest) => {
                let file = manifest.files.remove(0);
                let pick =
                    crate::metalinkdlg::Pick::of(&file, crate::regional::country().as_deref());
                self.apply_metalink(pick, ctx);
            }
            Err(error) => {
                self.metalink_error = Some(error.clone());
                self.info = Some(Err(error));
            }
        }
    }

    /// The one file of a Metalink (`pick`, its sources in the order to use them) is what this
    /// dialog downloads: from its first source, the others its mirrors, its hashes checked, and
    /// looked up as any address is. Browser credentials deliberately do not cross from the
    /// descriptor to a potentially unrelated mirror.
    pub fn apply_metalink(&mut self, pick: crate::metalinkdlg::Pick, ctx: &egui::Context) {
        let Some(primary) = pick.urls.first().cloned() else {
            return;
        };
        self.mirrors = pick.urls[1..].to_vec();
        self.checksum = pick.checksum;
        self.pieces = pick.pieces;
        self.url = primary.clone();
        self.url_text = primary.to_string();
        self.url_bad = false;
        self.metalink_error = None;
        self.kept = None;
        self.info = None;
        // A manifest's mirrors are independently chosen locations, not a redirect from the
        // browser page that supplied the descriptor. Never leak its cookies, form body or HTTP
        // authorization to them.
        self.context.cookies = None;
        self.context.referer = None;
        self.context.name = None;
        self.context.size = None;
        self.context.mime = None;
        self.context.final_url = None;
        self.context.post = None;
        self.context.post_type = None;
        self.request.headers = Default::default();
        self.request.opts.unlocker = None;
        self.request.opts.browser_final = None;
        self.request.opts.post = None;
        self.request.opts.expected_type = None;
        self.request.opts.mirrors = self.mirrors.clone();
        self.request.opts.checksum = self.checksum.clone();
        self.request.opts.pieces = self.pieces.clone().map(Arc::new);
        // Metalink locations are direct file URLs; an account that happens to recognise the
        // descriptor's host must not replace them with a different download.
        self.direct = true;
        self.through = None;
        let name = pick
            .path
            .file_name()
            .map(|n| mdm_engine::naming::sanitize(&n.to_string_lossy()))
            .unwrap_or_default();
        if !name.is_empty() && !self.edited {
            self.category = Category::of(&name);
            let folder = crate::model::category_folder(self.category, &self.downloads, &self.dirs);
            self.save_as = self.place.name_in(&folder, &name);
        }
        self.probe = start_probe(&self.rt, ctx, &primary, &self.request);
    }

    /// The account that takes the address appeared or changed (the services' host lists are
    /// read when DDM starts, and Options may change them while the dialog is open): looked up
    /// again its way. A download the browser had started keeps the browser's answer, and goes
    /// directly unless the line is ticked.
    pub(super) fn follow_route(&mut self, ctx: &egui::Context) {
        let through = self.route.through(&self.url);
        if through == self.through {
            return;
        }
        if self.direct {
            self.through = through;
        } else {
            self.look_again(ctx);
        }
    }

    /// The URL box changed: a new address DDM downloads is the one to download (looked up
    /// again; its file name too, unless Save As was edited); anything else holds Start and
    /// Later back.
    pub(super) fn url_edited(&mut self, ctx: &egui::Context) {
        // An explicit edit wins over a Metalink read still finishing in the background.
        self.metalink = None;
        self.metalink_error = None;
        self.mirrors.clear();
        self.checksum = None;
        self.pieces = None;
        self.request.opts.mirrors.clear();
        self.request.opts.checksum = None;
        self.request.opts.pieces = None;
        match crate::model::parse_address(&self.url_text) {
            Ok(url) if mdm_engine::supported(&url) => {
                self.url_bad = false;
                if url == self.url {
                    return;
                }
                let from_browser = crate::debrid::browser_started(&self.context);
                if !self.edited {
                    let name = mdm_engine::naming::file_name(None, &url);
                    self.category = Category::of(&name);
                    let dir =
                        crate::model::category_folder(self.category, &self.downloads, &self.dirs);
                    self.save_as = self.place.name_in(&dir, &name);
                }
                // Another address: the browser's name, size and form are not its own, and on
                // another site neither are its cookies and page (never sent there).
                if url.origin() != self.url.origin() {
                    self.context.cookies = None;
                    self.context.referer = None;
                    self.request.headers.remove(reqwest::header::COOKIE);
                    self.request.headers.remove(reqwest::header::REFERER);
                }
                self.context.name = None;
                self.context.size = None;
                self.context.mime = None;
                self.context.final_url = None;
                self.context.post = None;
                self.context.post_type = None;
                self.request.opts.post = None;
                self.request.opts.browser_final = None;
                self.request.headers.remove(reqwest::header::CONTENT_TYPE);
                // Another address isn't the download the browser had started: it goes through
                // the debrid account that takes its host (#44), unless the user unticked it.
                if from_browser {
                    self.direct = false;
                }
                self.url = url;
                self.look_again(ctx);
            }
            _ => self.url_bad = true,
        }
    }

    /// Save As keeps its file name in `folder`.
    pub(super) fn set_folder(&mut self, folder: &Path) {
        let name = Path::new(&self.save_as)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        // Fitted to the new folder too (#28): a deeper one leaves a long name less room.
        self.save_as = self.place.name_in(folder, &name);
        self.edited = true;
    }

    /// "Always save … files to this folder": the category and the folder to keep for it.
    pub fn remembered(&self) -> Option<(Category, String)> {
        self.remember.then(|| {
            let dir = Path::new(&self.save_as)
                .parent()
                .map(|d| d.display().to_string())
                .unwrap_or_default();
            (self.category, dir)
        })
    }

    /// The look's open answer, for the download of this address to start from (#28); taken
    /// once.
    pub fn take_kept(&mut self) -> Option<mdm_engine::KeptAnswer> {
        self.kept.take()
    }

    /// The Metalink locations that accompany the primary `url` when the dialog adds it.
    pub fn mirrors(&self) -> Vec<Url> {
        self.mirrors.clone()
    }

    /// The Metalink's expected whole-file hash, automatically checked by the engine.
    pub fn checksum(&self) -> Option<mdm_engine::verify::Hash> {
        self.checksum.clone()
    }

    /// The Metalink's piece hashes: a bad piece is fetched again (`Item::pieces`).
    pub fn pieces(&self) -> Option<mdm_engine::verify::Pieces> {
        self.pieces.clone()
    }

    /// The address was looked up and answered (name and size known): the moment IDM starts an
    /// early download.
    pub fn probed(&self) -> bool {
        matches!(self.info, Some(Ok(_)))
    }

    /// The name in Save As is only the address's guess: not edited, and no answer (the
    /// browser's, the look-up's) gave one. The server names the file when it starts (#28).
    pub fn name_guessed(&self) -> bool {
        !self.edited && !self.probed()
    }

    pub fn size(&self) -> Option<u64> {
        self.info
            .as_ref()
            .and_then(|i| i.as_ref().ok())
            .and_then(|i| i.size)
    }

    /// The chosen destination as an absolute path.
    pub fn dest(&self) -> PathBuf {
        resolve_path(&self.save_as)
    }

    pub(crate) fn set_category(&mut self, c: Category) {
        self.category = c;
        let name = Path::new(&self.save_as)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let folder = crate::model::category_folder(c, &self.downloads, &self.dirs);
        self.save_as = self.place.name_in(&folder, &name);
    }

    /// Whether Save As's folder is there, as looked at off the window's thread for the name it
    /// shows now (None: not known yet). A name DDM gave has its free numbered name by then.
    pub fn folder_found(&mut self) -> Option<bool> {
        self.place.folder(&mut self.save_as)
    }

    /// The dialog; Start (or Download Later) waits for a missing folder to be created.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        icons: &mut crate::chrome::ShellIcons,
    ) -> Outcome<Start> {
        let folder = self
            .dest()
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let out = self.dialog(ctx, icons);
        let out = ensure_folder(ctx, &mut self.asking, out, &folder);
        // #394: more downloads wait their turn behind this one (the browser's selected links):
        // Cancel (the button, Escape, the window's ×) asks whether they are cancelled too.
        if matches!(out, Outcome::Closed) && self.waiting > 0 && !self.cancel_ask {
            self.cancel_ask = true;
        }
        if self.cancel_ask {
            // The question answers for the dialog: a Start pressed meanwhile waits for it, as
            // with the Create folder question.
            return match cancel_waiting(ctx, self.waiting) {
                Outcome::Done(all) => {
                    self.cancel_ask = false;
                    self.cancel_all = all;
                    Outcome::Closed
                }
                // The question's own ×: as No, this one alone is skipped.
                Outcome::Closed => {
                    self.cancel_ask = false;
                    Outcome::Closed
                }
                Outcome::Open => Outcome::Open,
            };
        }
        out
    }

    pub(super) fn dialog(
        &mut self,
        ctx: &egui::Context,
        icons: &mut crate::chrome::ShellIcons,
    ) -> Outcome<Start> {
        self.follow_metalink(ctx);
        // A Metalink of several files: Add Metalink lists them (the app opens it with them).
        if self.handoff.is_some() {
            return Outcome::Closed;
        }
        if self.info.is_none()
            && let Some(res) = self.probe.lock().unwrap().take()
        {
            let res = res.map(|(pi, kept)| {
                self.kept = kept;
                pi
            });
            tracing::info!(
                "start time: Download File Info: the address answered in {} ms ({})",
                crate::starttime::ms(self.opened),
                match &res {
                    Ok(pi) if pi.size.is_some() => "its size known",
                    Ok(_) => "no size",
                    Err(_) => "an error",
                }
            );
            if let (Ok(pi), false) = (&res, self.edited) {
                self.category = Category::of(&pi.file_name);
                let dir = crate::model::category_folder(self.category, &self.downloads, &self.dirs);
                self.save_as = self.place.name_in(&dir, &pi.file_name);
            }
            self.info = Some(res);
        }
        // The name numbered in its folder, once the disk has answered (#743).
        self.place.follow(&mut self.save_as);
        self.follow_route(ctx);
        // The "Download through" line (#44) takes a row of its own while an account takes the
        // address: the window grows by it.
        let dy = if self.through.is_some() { 26.0 } else { 0.0 };
        window(
            ctx,
            "file-info",
            "Download File Info",
            [570.0, 248.0 + dy],
            true,
            winui::BG(),
            |ui, o| {
                let p = ui.painter().clone();
                // The labels' column: right-aligned at 88, or after the widest label in a longer
                // language; the boxes start 8 px after it and keep their right edges.
                let lx = ["URL", "Category", "Save As", "Description", "Server"]
                    .iter()
                    .map(|l| 12.0 + winui::text_width(&p, crate::i18n::t(l)).ceil())
                    .fold(88.0_f32, f32::max);
                let fx = lx + 8.0;
                winui::text_right(
                    &p,
                    o.x + lx,
                    o.y + 18.0,
                    crate::i18n::t("URL"),
                    winui::TEXT(),
                );
                let url_rc = r(o, fx, 5.0, 462.0 - fx, 23.0);
                let url_box = winui::edit(ui, url_rc, &mut self.url_text, true, false);
                // An edited address is the one downloaded (it was ignored before).
                if url_box.changed() {
                    self.url_edited(ui.ctx());
                }
                if self.url_bad {
                    winui::error_frame(&p, url_rc);
                }

                // i18n: noun: label before the list of categories
                winui::text_right(
                    &p,
                    o.x + lx,
                    o.y + 46.0,
                    crate::i18n::t("Category"),
                    winui::TEXT(),
                );
                let cat_rect = r(o, fx, 36.0, 174.0, 21.0);
                let shown = self.category.shown();
                let cat = winui::combo(ui, cat_rect, &shown, "category");
                if cat.clicked() || winui::combo_keys(ui, &cat) {
                    self.category_open = !self.category_open;
                }
                let category_was_open = self.category_open;
                // "+": a new category (the app's category window).
                if winui::small_button(ui, r(o, fx + 180.0, 35.0, 22.0, 21.0), "+").clicked() {
                    self.new_category = true;
                }

                winui::text_right(
                    &p,
                    o.x + lx,
                    o.y + 77.0,
                    crate::i18n::t("Save As"),
                    winui::TEXT(),
                );
                let save_rc = r(o, fx, 63.0, 431.0 - fx, 21.0);
                let save = winui::combo_edit(ui, save_rc, &mut self.save_as);
                // Opened, the keys are in Save As (its name is what one changes here).
                if !self.focused {
                    save.request_focus();
                    self.focused = true;
                }
                if save.changed() {
                    self.edited = true;
                }
                // The arrow: the folders saved to before.
                if winui::combo_arrow(ui, save_rc, "save-as", self.folders_open).clicked()
                    || winui::combo_keys(ui, &save)
                {
                    self.folders_open = !self.folders_open;
                }
                // Up / Down step through the folders saved to before (the file name stays).
                let dir = Path::new(&self.save_as).parent().map(|d| d.to_path_buf());
                let at = self
                    .folders
                    .iter()
                    .position(|f| dir.as_deref() == Some(resolve_path(f).as_path()));
                if !self.folders_open
                    && let Some(i) = winui::history_keys(ui, &save, self.folders.len(), at)
                {
                    let folder = resolve_path(&self.folders[i]);
                    self.set_folder(&folder);
                    self.edited = true;
                }
                winui::keep_keys(ui, &save, false, true, self.folders_open);
                let folders_were_open = self.folders_open;
                if self.folders_open {
                    let shown: Vec<String> = self
                        .folders
                        .iter()
                        .map(|f| display_path(Path::new(f)))
                        .collect();
                    if let Some(i) = winui::drop_list(
                        ui,
                        "save-as-history",
                        save_rc,
                        &shown,
                        crate::i18n::t("No folders yet"),
                        &mut self.folders_open,
                    ) {
                        let folder = PathBuf::from(&self.folders[i]);
                        self.set_folder(&folder);
                    }
                }
                // "...": choose the folder.
                if winui::small_button(ui, r(o, 439.0, 64.0, 22.0, 21.0), "...").clicked() {
                    let now = self
                        .dest()
                        .parent()
                        .map(Path::to_path_buf)
                        .unwrap_or_default();
                    // i18n: title of the folder picker
                    let title = crate::i18n::t("Select a folder").to_owned();
                    winui::start_pick(ui.ctx(), "file-info-folder", move || {
                        let mut pick = rfd::FileDialog::new().set_title(title);
                        if now.is_dir() {
                            pick = pick.set_directory(&now);
                        }
                        pick.pick_folder()
                    });
                }
                if let Some(dir) = winui::picked(ui.ctx(), "file-info-folder") {
                    self.set_folder(&dir);
                }

                let caption = crate::i18n::fmt(
                    // i18n: Download File Info: checkbox; the folder in Save As becomes this category's folder from now on; {category} = the category's name (Video, Music...)
                    crate::i18n::t("Always save {category} files to this folder"),
                    &[("category", &self.category.shown())],
                );
                let gap_end = o.x + fx + 26.0 + winui::text_width(&p, &caption) + 5.0;
                winui::group_box(
                    &p,
                    r(o, fx, 97.0, 462.0 - fx, 45.0),
                    Some((o.x + fx + 8.0, gap_end)),
                );
                winui::checkbox(
                    ui,
                    o + vec2(fx + 8.0, 90.0),
                    &mut self.remember,
                    &caption,
                    true,
                );
                let mut folder = format!(
                    "{}\\",
                    display_path(&crate::model::category_folder(
                        self.category,
                        &self.downloads,
                        &self.dirs
                    ))
                    .trim_end_matches('\\')
                );
                let folder_rc = r(o, fx + 8.0, 112.0, 444.0 - fx, 23.0);
                winui::edit(ui, folder_rc, &mut folder, false, false);
                // Greyed as IDM shows it, and still copyable.
                winui::copyable(ui, folder_rc, "remembered-folder", &folder, false);

                // i18n: noun: label before a free-text box describing the file
                winui::text_right(
                    &p,
                    o.x + lx,
                    o.y + 161.0,
                    crate::i18n::t("Description"),
                    winui::TEXT(),
                );
                winui::edit(
                    ui,
                    r(o, fx, 148.0, 462.0 - fx, 23.0),
                    &mut self.description,
                    true,
                    false,
                );

                // Through a debrid account (#44): ticked, the service unlocks the link; unticked,
                // the address is downloaded directly. Either way it is looked up again.
                if let Some(through) = &self.through {
                    let mut on = !self.direct;
                    let caption = crate::i18n::fmt(
                        // i18n: Download File Info: a check box, ticked when the download goes through the user's debrid account; {service} the service and the account, "Real-Debrid (labuser)". Unticked: downloaded directly
                        crate::i18n::t("Download through {service}"),
                        &[("service", through)],
                    );
                    winui::checkbox_until(
                        ui,
                        o + vec2(fx, 186.0),
                        &mut on,
                        &caption,
                        true,
                        Some(o.x + 462.0),
                    );
                    if on == self.direct {
                        self.direct = !on;
                        self.look_again(ui.ctx());
                    }
                }

                // Where the server is (#140), as the look saw it: "..." while it looks; blank when
                // nothing was asked (the browser had the answer) or the look failed.
                // i18n: label before where a download's server is (its country and round trip)
                winui::text_right(
                    &p,
                    o.x + lx,
                    o.y + 193.0 + dy,
                    crate::i18n::t("Server"),
                    winui::TEXT(),
                );
                match &self.info {
                    None => crate::whereis::line(
                        ui,
                        o.x + fx,
                        o.y + 193.0 + dy,
                        o.x + 462.0,
                        None,
                        "file-info",
                    ),
                    Some(Ok(pi)) => {
                        if let Some(server) = &pi.server {
                            let seen = crate::whereis::Seen::from(server);
                            crate::whereis::line(
                                ui,
                                o.x + fx,
                                o.y + 193.0 + dy,
                                o.x + 462.0,
                                Some(&seen),
                                "file-info",
                            );
                        }
                    }
                    Some(Err(_)) => {}
                }

                // Held back while the address or the file's name can't be downloaded to (an
                // empty Save As, or a folder, made it the profile folder).
                let name_bad = {
                    let typed = self.save_as.trim();
                    let dest = self.dest();
                    // A local disk answers within the wait; a share that doesn't reads as no
                    // folder meanwhile (the download says so if it can't write there).
                    let folder = || {
                        self.dest_dir
                            .get(
                                &dest,
                                std::time::Duration::from_secs(1),
                                std::time::Duration::from_millis(50),
                                Some(ui.ctx()),
                                |d| d.is_dir(),
                            )
                            .unwrap_or(false)
                    };
                    typed.is_empty()
                        || typed.ends_with(['\\', '/'])
                        || dest.file_name().is_none()
                        || folder()
                };
                let blocked = if let Some(error) = self.metalink_error.clone() {
                    Some(error)
                } else if self.url_bad {
                    Some("Invalid address".to_owned())
                } else if name_bad {
                    // i18n: why Start Download is greyed in Download File Info
                    Some(crate::i18n::mark("Type the file's name in Save As").to_owned())
                } else {
                    None
                };
                let (later_kind, start_kind) = if blocked.is_some() {
                    (ButtonKind::Disabled, ButtonKind::Disabled)
                } else {
                    (ButtonKind::Normal, ButtonKind::Default)
                };
                // The row as wide as its labels in this language need (#192), ending where it
                // ends in English.
                let [later_at, start_at, cancel_at] = winui::button_row(
                    &p,
                    &[
                        ("Download Later", 97.0, 109.0),
                        ("Start Download", 225.0, 109.0),
                        ("Cancel", 352.0, 109.0),
                    ],
                    19.0,
                    11.0,
                    200.0,
                )[..] else {
                    unreachable!("three buttons")
                };
                // 12 px under the Server line, 12 px over the window's edge.
                let later = winui::button(
                    ui,
                    r(o, later_at.0, 215.0 + dy, later_at.1, 21.0),
                    "Download Later",
                    later_kind,
                    false,
                    Some(9),
                );
                let later =
                    winui::why_disabled(later, blocked.is_some(), blocked.as_deref().unwrap_or(""));
                let start = winui::button(
                    ui,
                    r(o, start_at.0, 215.0 + dy, start_at.1, 21.0),
                    "Start Download",
                    start_kind,
                    true,
                    Some(0),
                );
                let start =
                    winui::why_disabled(start, blocked.is_some(), blocked.as_deref().unwrap_or(""));
                let cancel = winui::button(
                    ui,
                    r(o, cancel_at.0, 215.0 + dy, cancel_at.1, 21.0),
                    "Cancel",
                    ButtonKind::Normal,
                    false,
                    Some(0),
                );

                // Right column, centred on x ≈ 516: the file's icon and its size.
                // The shell's own 32 px icon for this file type, as IDM shows.
                let ext = Path::new(&self.save_as)
                    .extension()
                    .map(|e| e.to_string_lossy().to_ascii_lowercase())
                    .unwrap_or_default();
                icons.paint(
                    ui,
                    crate::chrome::IconKey::ExtLarge(ext),
                    o + vec2(500.0, 45.0),
                );
                let (size, reason) = match &self.info {
                    // While the address is asked (#3): what is going on, the size in its place
                    // as soon as it answers (the window is up meanwhile, nothing waits for it).
                    None => (
                        // i18n: Download File Info, where the file's size will show: DDM is asking the server for the file's name and size (short: the column is about 90 pixels wide)
                        crate::i18n::t("Checking...").to_owned(),
                        Some(
                            // i18n: Download File Info: tooltip over "Checking..." where the size will show
                            crate::i18n::t("Asking the server for the file's name and size")
                                .to_owned(),
                        ),
                    ),
                    Some(Ok(pi)) => (
                        // i18n: a download's size is not known (the server didn't say)
                        pi.size
                            .map_or(crate::i18n::t("unknown").to_owned(), dialog_size),
                        None,
                    ),
                    // i18n: in Download File Info, where the size goes: the address didn't answer
                    Some(Err(e)) => (crate::i18n::t("Error").to_owned(), Some(e.clone())),
                };
                // Muted while asked; a long translation cut to the column (whole on hover).
                let asking = self.info.is_none();
                let ink = if asking {
                    winui::MUTED()
                } else {
                    winui::TEXT()
                };
                let size = winui::ellipsize(&p, &winui::tahoma(), &size, SIZE_COLUMN);
                let w = winui::text_width(&p, &size);
                let at = winui::text(&p, o.x + 515.0 - w / 2.0, o.y + 103.0, &size, ink);
                // Why the address didn't answer (not found, sign-in needed…): on hover, copyable.
                if let Some(reason) = reason {
                    winui::copyable(ui, at, "file-info-error", &reason, true);
                }
                // No Preview button (IDM has one under the size): a part file is written by many
                // connections at once and isn't a file a player can open, so it would only ever
                // be greyed.

                // The categories in a list window of their own (the shared drop list: keys,
                // Escape, never cut off by this short window).
                if self.category_open {
                    let all = Category::all();
                    let labels: Vec<String> = all.iter().map(|c| c.shown()).collect();
                    if let Some(i) = winui::drop_list(
                        ui,
                        "file-info-category",
                        cat_rect,
                        &labels,
                        "",
                        &mut self.category_open,
                    ) && let Some(c) = all.get(i)
                    {
                        self.set_category(*c);
                    }
                }

                // Escape closes an open drop list first, then the dialog.
                let key =
                    |k| !folders_were_open && !category_was_open && ui.input(|i| i.key_pressed(k));
                if cancel.clicked() || key(egui::Key::Escape) {
                    Outcome::Closed
                } else if blocked.is_some() {
                    Outcome::Open
                } else if start.clicked() || key(egui::Key::Enter) {
                    Outcome::Done(Start::Now)
                } else if later.clicked() {
                    Outcome::Done(Start::Later)
                } else {
                    Outcome::Open
                }
            },
        )
    }
}

/// Look `url` up in the background (name, size) as the download will ask it (`request`), the
/// answer landing in the slot returned.
/// A look's result: what it learned, and its open answer when the download can read it.
pub(super) type Probed = Result<(ProbeInfo, Option<mdm_engine::KeptAnswer>), String>;

/// A finished descriptor read, held for the interface thread to apply.
pub(super) type MetalinkRead = Result<mdm_engine::metalink::Manifest, String>;

/// IDM shows paths under the user profile relative to it: `Downloads\Compressed\a.zip`.
pub fn display_path(p: &Path) -> String {
    match dirs::home_dir().and_then(|h| p.strip_prefix(h).ok().map(Path::to_owned)) {
        Some(rel) => rel.display().to_string(),
        None => p.display().to_string(),
    }
}

/// The inverse of `display_path`: relative paths are under the user profile.
pub fn resolve_path(s: &str) -> PathBuf {
    let p = PathBuf::from(s.trim());
    if p.is_absolute() {
        p
    } else {
        dirs::home_dir().map_or(p.clone(), |h| h.join(p))
    }
}

/// The engine's naming rules on the disk as it is now; on a thread of its own (it may wait).
pub(super) fn look(ask: &PlaceAsk) -> Found {
    Found {
        dest: if ask.number {
            mdm_engine::naming::destination(&ask.dir, &ask.name)
        } else {
            ask.dir.join(&ask.name)
        },
        folder: ask.dir.is_dir(),
    }
}

/// Cancel in Download File Info while more downloads wait their turn behind it (#394): are
/// those cancelled too? `Done(true)`: yes; `Done(false)`: this one alone is skipped.
pub(super) fn cancel_waiting(ctx: &egui::Context, n: usize) -> Outcome<bool> {
    let extra = grow_extra(ctx, "cancel-waiting");
    question(
        ctx,
        "cancel-waiting",
        "Dannex Download Manager",
        [374.0, 100.0 + extra],
        false,
        winui::BG(),
        |ui, o| {
            let p = ui.painter().clone();
            let text = crate::i18n::fmt(
                // i18n: Cancel in Download File Info with downloads waiting behind it
                &crate::i18n::tn(
                    n as u64,
                    "There is {n} more download waiting. Cancel it too?",
                    "There are {n} more downloads waiting. Cancel them too?",
                ),
                &[("n", &crate::i18n::integer(n as u64))],
            );
            let lines = winui::paragraph(
                &p,
                o.x + 11.0,
                o.y + 21.0,
                &text,
                winui::TEXT(),
                o.x + 362.0,
                13.0,
            );
            grow_to(ui.ctx(), "cancel-waiting", lines, 2);
            let o = o + vec2(0.0, extra);
            let yes = winui::button(
                ui,
                r(o, 94.0, 63.0, 73.0, 21.0),
                "Yes",
                ButtonKind::Default,
                false,
                None,
            )
            .clicked();
            let no = winui::button(
                ui,
                r(o, 207.0, 63.0, 73.0, 21.0),
                "No",
                ButtonKind::Normal,
                false,
                None,
            )
            .clicked();
            if no || winui::escape_pressed(ui) {
                Outcome::Done(false)
            } else if yes || winui::enter_pressed(ui) {
                Outcome::Done(true)
            } else {
                Outcome::Open
            }
        },
    )
}

/// Reads one remote Metalink before Download File Info goes on with its file (one), or hands
/// its files to Add Metalink (several). Bounded before parsing (`MAX_MANIFEST`, also without a
/// `Content-Length`), through the same TLS, DNS and proxy route as downloads, with the
/// descriptor's own request (the browser's cookies); its errors in the interface language.
pub(super) fn start_metalink(
    rt: &tokio::runtime::Handle,
    ctx: &egui::Context,
    url: &Url,
    request: &crate::app::RequestParts,
) -> Arc<Mutex<Option<MetalinkRead>>> {
    use mdm_engine::metalink::{MAX_MANIFEST, MetalinkError};
    let slot = Arc::new(Mutex::new(None));
    let (slot_out, url, ctx) = (slot.clone(), url.clone(), ctx.clone());
    let (headers, opts) = (request.headers.clone(), request.opts.clone());
    rt.spawn(async move {
        let result = async {
            let net = |e: reqwest::Error| crate::errors::describe(&mdm_engine::Error::from(e));
            let too_large = || crate::metalinkdlg::describe(&MetalinkError::TooLarge);
            let client =
                mdm_engine::http_client(&opts, &url).map_err(|e| crate::errors::describe(&e))?;
            let mut response = client.get(url).headers(headers).send().await.map_err(net)?;
            if !response.status().is_success() {
                let status = mdm_engine::Error::Status(response.status());
                return Err(crate::errors::describe(&status));
            }
            if response.content_length().is_some_and(|n| n > MAX_MANIFEST) {
                return Err(too_large());
            }
            let source = response.url().clone();
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(net)? {
                bytes.extend_from_slice(&chunk);
                if bytes.len() as u64 > MAX_MANIFEST {
                    return Err(too_large());
                }
            }
            mdm_engine::metalink::parse_bytes(&bytes, &source)
                .map_err(|e| crate::metalinkdlg::describe(&e))
        }
        .await;
        *slot_out.lock().unwrap() = Some(result);
        ctx.request_repaint();
        crate::starttime::nudge();
    });
    slot
}

pub(super) fn start_probe(
    rt: &tokio::runtime::Handle,
    ctx: &egui::Context,
    url: &Url,
    request: &crate::app::RequestParts,
) -> Arc<Mutex<Option<Probed>>> {
    let probe = Arc::new(Mutex::new(None));
    let (slot, url, ctx) = (probe.clone(), url.clone(), ctx.clone());
    let (headers, opts) = (request.headers.clone(), request.opts.clone());
    rt.spawn(async move {
        // In the interface language (the engine's own words are English).
        let res = mdm_engine::probe_kept(url, headers, &opts)
            .await
            .map_err(|e| crate::errors::describe(&e));
        *slot.lock().unwrap() = Some(res);
        ctx.request_repaint();
        // Its size in the window at once, the main window in the notification area too (#3).
        crate::starttime::nudge();
    });
    probe
}
