//! Addresses in and out: Tasks → Import and Export, Batch download, links and files dropped (the
//! main window, the drop target), Link grabber's links, Metalink files, and the system's file
//! choosers answered.

use super::*;

/// Links or files dropped on the main window or the drop target, sorted beside the window
/// ([`sort_dropped`], #743): whether each is a file was asked of the disk there, and the lists
/// of addresses read.
#[derive(Debug, Default, PartialEq)]
pub(super) struct Dropped {
    /// Lists of addresses (.txt, .ef2, .htm, .csv files): each one's name and what it held, or
    /// why it couldn't be read.
    pub(super) lists: Vec<(String, Result<Vec<crate::transfer::Imported>, String>)>,
    /// Addresses DDM downloads.
    pub(super) links: Vec<String>,
    /// Metalink files (.metalink, .meta4): each one's path and what it holds, or why it can't
    /// be used (`metalinkdlg::read_file`).
    pub(super) metalinks: Vec<(String, Result<mdm_engine::metalink::Manifest, String>)>,
    /// The rest, in the order dropped: each, and whether it is a magnet link or a .torrent file.
    pub(super) other: Vec<(String, bool)>,
}

impl App {
    /// Batch download → OK: every address goes to the main download queue (IDM's default; its
    /// queue selection panel is not built yet).
    pub(super) fn add_batch(&mut self, added: crate::batch::Added) {
        let mut ids = Vec::new();
        for url in added.urls {
            let name = mdm_engine::naming::file_name(None, &url);
            let category = Category::of(&name);
            let dir = self.settings.category_dir(category, &self.downloads);
            // Its folder isn't looked at here (#743): `number_beside`.
            let dest = mdm_engine::naming::guess(&dir, &name);
            let id = self.push_item(
                url,
                dest,
                category,
                String::new(),
                None,
                added.login.clone(),
                Default::default(),
                crate::i18n::mark("by batch download"),
            );
            self.name_guessed(id, true);
            ids.push(id);
        }
        self.number_beside(&ids);
        self.queue_later(ids, self.settings.queue_panel_batch);
        self.dirty = true;
    }

    /// A Metalink read (Download File Info's hand-off, a file read beside the window): one
    /// file goes on in Download File Info, as an address does (its first source the address,
    /// the others its mirrors); several open Add Metalink. One Add Metalink at a time: another
    /// that comes meanwhile waits its turn.
    pub(super) fn open_metalink(
        &mut self,
        opened: crate::metalinkdlg::Opened,
        ctx: &egui::Context,
    ) {
        let country = crate::regional::country();
        if let [file] = opened.manifest.files.as_slice() {
            let pick = crate::metalinkdlg::Pick::of(file, country.as_deref());
            if let Some(url) = pick.urls.first().cloned() {
                let new = dialogs::NewUrl { url, login: None };
                self.open_file_info_with(new, Default::default(), Some(pick), ctx);
            }
            return;
        }
        if self.metalink_dlg.is_some() {
            self.metalink_waiting.push_back(opened);
            return;
        }
        self.metalink_dlg = Some(crate::metalinkdlg::MetalinkDialog::new(
            opened,
            country.as_deref(),
            self.downloads.clone(),
            self.settings.category_dirs.clone(),
        ));
    }

    /// A Metalink file on this computer (a drop, Tasks → Open Metalink file..., the command
    /// line), read beside the window (#743): `open_metalink` with what it holds, or a message
    /// saying why it can't be used.
    pub(super) fn read_metalink_file(&mut self, path: PathBuf) {
        self.ask_beside(move || Beside::Metalink {
            origin: path.display().to_string(),
            read: crate::metalinkdlg::read_file(&path),
        });
    }

    /// Add Metalink's Download Now / Later: each file chosen becomes a download in the main
    /// queue, at its place under the folder, from its first source with the others its mirrors
    /// and its hashes; Download Now runs the queue (as the Link grabber's Start does).
    pub(super) fn add_metalink(&mut self, add: crate::metalinkdlg::MetalinkAdd) {
        if self.settings.change_folder_on_last
            && self
                .settings
                .keep_category_dir(add.category, &add.dir, &self.downloads)
        {
            self.dirty = true;
        }
        let mut added = Vec::new();
        for (url, dest, size, pick) in crate::metalinkdlg::downloads(&add) {
            let id = self.push_item(
                url,
                dest,
                add.category,
                String::new(),
                size,
                None,
                Default::default(),
                // i18n: how a download was added (in its log: "Added {source}"): one of the files a Metalink listed
                crate::i18n::mark("from a Metalink"),
            );
            // The publisher's own addresses: never through a debrid account.
            self.set_direct(id, true);
            if let Some(i) = self.items.last_mut().filter(|i| i.id == id) {
                i.mirrors = pick.urls[1..].to_vec();
                i.checksum = pick.checksum;
                i.pieces = pick.pieces;
            }
            self.queues[0].items.push(id);
            added.push(id);
        }
        // Names already taken in their folders are numbered beside the window (#743).
        self.number_beside(&added);
        self.dirty = true;
        if add.start_now && !added.is_empty() {
            self.start_queue(0);
        }
    }

    /// Tasks → Show drop target: the box while it's on; what is dropped on it is added as if
    /// given on the command line (an address, a magnet link, a .torrent file).
    pub(super) fn drop_target(&mut self, ctx: &egui::Context) {
        if !self.settings.drop_target {
            self.drop_target = None;
            return;
        }
        // First shown: in the main window's lower-right corner, over the list's empty end (not
        // over a toolbar button), on the same screen.
        let pos = self
            .settings
            .drop_target_pos
            .map(|(x, y)| pos2(x as f32, y as f32))
            .or_else(|| {
                ctx.input(|i| i.viewport().outer_rect)
                    .map(|r| pos2(r.right() - 56.0 - 40.0, r.bottom() - 56.0 - 56.0))
            });
        let dt = self
            .drop_target
            .get_or_insert_with(|| crate::droptarget::DropTarget::new(pos));
        for ev in dt.show(ctx) {
            use crate::droptarget::Event;
            match ev {
                Event::Drop(s) => self.dropped(vec![s], ctx),
                Event::ShowMain => self.show_window(ctx),
                Event::Context(screen) => self.drop_target_menu(screen, ctx),
                Event::Moved(p) => {
                    self.settings.drop_target_pos = Some((p.x.round() as i32, p.y.round() as i32));
                    self.dirty = true;
                }
            }
        }
    }

    /// The drop target's menu at `screen` (a point on the screen, logical points).
    pub(super) fn drop_target_menu(&mut self, screen: Pos2, ctx: &egui::Context) {
        // Popups take a point in the main window.
        let origin = ctx
            .input(|i| i.viewport().inner_rect)
            .map_or(Pos2::ZERO, |r| r.min);
        self.open_popup(Popup::DropTarget, screen - origin.to_vec2());
    }

    /// Tasks → Export: every download's address, as an IDM list (.ef2) or plain text.
    pub(super) fn export(&mut self, ef2: bool) {
        let (ext, name) = if ef2 {
            ("ef2", "downloads.ef2")
        } else {
            ("txt", "downloads.txt")
        };
        let title = crate::i18n::t("Export").to_owned();
        let key = if ef2 {
            "app-export-ef2"
        } else {
            "app-export-txt"
        };
        winui::start_pick(&self.ui_ctx, key, move || {
            // Off-screen test runs have no file dialog to answer.
            if let Some(path) =
                std::env::var_os("MDM_TEST_EXPORT_TO").filter(|_| crate::offscreen())
            {
                return Some(PathBuf::from(path));
            }
            rfd::FileDialog::new()
                .set_title(title)
                .add_filter(ext.to_ascii_uppercase(), &[ext])
                .set_file_name(name)
                .save_file()
        });
    }

    /// Tasks → Export, the file chosen: the addresses written to it, beside the window (#743:
    /// it may be on a drive slow to answer); one that couldn't be written says so then
    /// (`Beside::Exported`).
    pub(super) fn export_to(&mut self, path: &std::path::Path, ef2: bool) {
        let text = if ef2 {
            crate::transfer::export_ef2(&self.items)
        } else {
            crate::transfer::export_text(&self.items)
        };
        let path = path.to_owned();
        self.ask_beside(move || {
            Beside::Exported(std::fs::write(&path, text).map_err(|e| e.to_string()))
        });
    }

    /// What was dropped on the main window (its OLE target on Windows; winit's dropped files on
    /// macOS and Linux, which hand over files only).
    pub(super) fn main_drops(&mut self, ctx: &egui::Context) {
        #[cfg_attr(windows, allow(unused_mut))]
        let mut got = self.drops.poll(ctx, crate::MAIN_TITLE);
        #[cfg(not(windows))]
        got.extend(ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().display().to_string())
                .filter(|p| !p.is_empty())
                .collect::<Vec<_>>()
        }));
        if !got.is_empty() {
            self.dropped(got, ctx);
        }
    }

    /// Links or files dropped (on the main window, on the drop target): one address, a magnet
    /// link or a .torrent file goes the usual way (Download File Info, Add torrent); several
    /// addresses, or a list file (.txt, .ef2, .htm, .csv), show in Import to tick first.
    /// Whether each is a file, and a list file's addresses, are asked of the disk beside the
    /// window (#743, [`sort_dropped`]); what was dropped goes on once the answer comes
    /// ([`Self::dropped_sorted`]).
    pub(super) fn dropped(&mut self, items: Vec<String>, _ctx: &egui::Context) {
        self.ask_beside(move || Beside::Dropped(Box::new(sort_dropped(items))));
    }

    /// What was dropped, sorted beside the window ([`sort_dropped`]): each list shown in Link
    /// grabber; one address the usual way, several in Link grabber; a magnet link or a .torrent
    /// file in Add torrent; the rest said.
    pub(super) fn dropped_sorted(&mut self, sorted: Dropped, ctx: &egui::Context) {
        let Dropped {
            lists,
            links,
            metalinks,
            other,
        } = sorted;
        for (file, read) in lists {
            self.show_list(file, read);
        }
        // Metalink files: each one's file in Download File Info, or its files in Add Metalink.
        for (origin, read) in metalinks {
            match read {
                Ok(manifest) => {
                    self.open_metalink(crate::metalinkdlg::Opened { manifest, origin }, ctx);
                }
                Err(e) => self.notice = Some((crate::i18n::mark("Metalink"), e)),
            }
        }
        // Link grabber is a window of its own: the main window stays where it is.
        if links.len() > 1 {
            let found = links
                .into_iter()
                .map(|url| crate::transfer::Imported {
                    url,
                    ..Default::default()
                })
                .collect();
            // i18n: in Import, where the addresses came from: dragged onto the window
            let source = crate::i18n::t("Dropped links").to_owned();
            self.show_import(found, source);
        } else {
            for url in links {
                self.handle_command(
                    ipc::Command {
                        url: Some(url),
                        ..Default::default()
                    },
                    ctx,
                );
            }
        }
        for (s, torrent) in other {
            if torrent {
                // As a command with it (`handle_command`), its file looked at already.
                self.command_goes_on(
                    ipc::Command {
                        url: Some(s),
                        ..Default::default()
                    },
                    Some(true),
                    ctx,
                );
            } else {
                // A file DDM can't download or import (a picture, a program).
                self.notice = Some((
                    crate::i18n::mark("Dannex Download Manager"),
                    crate::i18n::fmt(
                        crate::i18n::t(
                            "Nothing to download in {item}: drop links, a .torrent file, or a list of addresses (.txt, .ef2).",
                        ),
                        &[("item", &crate::i18n::isolate(&s))],
                    ),
                ));
            }
        }
    }

    /// Tasks → Import: read a file and show the addresses found, to check before adding.
    pub(super) fn import(&mut self, ef2: bool) {
        // i18n: title of a file dialog and of a message box: Tasks → Import, a list of addresses read from a file (the command's name, not a past tense such as "imported")
        let title = crate::i18n::t("Import").to_owned();
        // i18n: the kind of file the Import dialog lists (text files with addresses)
        let lists = crate::i18n::t("Lists of addresses").to_owned();
        let all = crate::i18n::t("All files").to_owned();
        winui::start_pick(&self.ui_ctx, "app-import", move || {
            // Off-screen test runs have no file dialog to answer.
            if let Some(path) =
                std::env::var_os("MDM_TEST_IMPORT_FROM").filter(|_| crate::offscreen())
            {
                return Some(PathBuf::from(path));
            }
            let dialog = rfd::FileDialog::new().set_title(title);
            let dialog = if ef2 {
                dialog.add_filter("EF2", &["ef2"])
            } else {
                dialog.add_filter(lists, &["txt", "htm", "html", "csv"])
            };
            dialog.add_filter(all, &["*"]).pick_file()
        });
    }

    /// The system dialogs opened from the main window, answered (winui::start_pick): their
    /// work goes on.
    pub(super) fn poll_picks(&mut self, ctx: &egui::Context) {
        if let Some(f) = winui::picked(ctx, "app-torrent-file") {
            self.open_torrent_dialog(Some(f.display().to_string()), ctx);
        }
        if let Some(f) = winui::picked(ctx, "app-metalink-file") {
            self.read_metalink_file(f);
        }
        if let Some(path) = winui::picked(ctx, "app-export-ef2") {
            self.export_to(&path, true);
        }
        if let Some(path) = winui::picked(ctx, "app-export-txt") {
            self.export_to(&path, false);
        }
        if let Some(path) = winui::picked(ctx, "app-import") {
            // Read beside the window (#743): shown once the answer comes (`show_list`).
            self.ask_beside(move || {
                let (file, read) = read_list(&path);
                Beside::Imported { file, read }
            });
        }
        if let Some(path) = winui::picked(ctx, "app-theme-file") {
            self.add_theme_file(&path);
        }
        if let Some(path) = winui::picked(ctx, "app-theme-export") {
            // Named after the file; written beside the window (#743).
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let text = crate::themefile::export(crate::theme::current(), &name);
            self.ask_beside(move || {
                Beside::ThemeExported(std::fs::write(&path, text).map_err(|e| e.to_string()))
            });
        }
        if let Some(target) = winui::picked(ctx, "app-move-rename")
            && let Some(id) = self.move_rename_for.take()
        {
            self.start_move(id, target);
        }
        if let Some(dir) = winui::picked(ctx, "app-move-many") {
            let ids = std::mem::take(&mut self.move_many_for);
            self.move_many_to(ids, dir);
        }
    }

    /// The addresses in list file `file`, as read beside the window (`read_list`, #743), shown
    /// in Import to tick before adding; one that couldn't be read, or holds none, says so.
    pub(super) fn show_list(
        &mut self,
        file: String,
        read: Result<Vec<crate::transfer::Imported>, String>,
    ) {
        let found = match read {
            Ok(found) => found,
            Err(e) => {
                self.notice = Some((
                    crate::i18n::mark("Import"),
                    crate::i18n::fmt(
                        crate::i18n::t("The file could not be read: {error}"),
                        &[("error", &e)],
                    ),
                ));
                return;
            }
        };
        if found.is_empty() {
            self.notice = Some((
                crate::i18n::mark("Import"),
                crate::i18n::fmt(
                    crate::i18n::t("No web addresses were found in {file}."),
                    &[("file", &file)],
                ),
            ));
            return;
        }
        self.show_import(found, file);
    }

    /// Links to tick before adding (the browser's page links, Import, dropped links): into Link
    /// grabber open already (several dropped lists, a list and links), else a new one; those in
    /// the list already go unticked under "Already in the list" (imported twice, they were
    /// doubled).
    pub(super) fn show_import(&mut self, found: Vec<crate::transfer::Imported>, source: String) {
        let known: std::collections::HashSet<String> =
            self.items.iter().map(|i| i.url.to_string()).collect();
        let (downloads, s) = (&self.downloads, &self.settings);
        let grabber = self.grabber.get_or_insert_with(|| {
            let mut g = crate::grabber::Grabber::new(
                downloads.clone(),
                s.category_dirs.clone(),
                s.grabber_hide_pages,
                s.extract_delete,
            );
            g.look_up = s.grabber_check;
            g.row_h = crate::grabber::row_height(s.density);
            g
        });
        grabber.add(found, source, |u| known.contains(u.as_str()));
    }

    /// Link grabber's ticked links: each package's to its folder, tagged with the package (its
    /// archives are extracted when all are in), into the main queue; started too on Start (else
    /// only queued, as IDM's import).
    pub(super) fn add_grabbed(&mut self, start: bool, packages: Vec<crate::grabber::Picked>) {
        // Package numbers after those in the list.
        let last = self
            .items
            .iter()
            .filter_map(|i| i.package.as_ref().map(|p| p.id))
            .max()
            .unwrap_or(0);
        let mut added = Vec::new();
        for (id, pk) in (last + 1..).zip(packages) {
            let tag = crate::model::PackageTag {
                id,
                name: pk.name.clone(),
                extract: pk.extract,
                delete_after: pk.delete,
                password: pk.password.clone(),
            };
            for (d, name, size) in pk.links {
                let Ok(url) = url::Url::parse(&d.url) else {
                    continue;
                };
                // The package's category, else the file's own.
                let category = pk.category.unwrap_or_else(|| Category::of(&name));
                let dir = if pk.folder.as_os_str().is_empty() {
                    self.settings.category_dir(category, &self.downloads)
                } else {
                    pk.folder.clone()
                };
                // Its folder isn't looked at here (#743): `number_beside`.
                let dest = mdm_engine::naming::guess(&dir, &name);
                let context = ipc::Command {
                    referer: d.referer,
                    cookies: d.cookies,
                    user_agent: d.user_agent,
                    private: d.private,
                    ..Default::default()
                };
                let id = self.push_item(
                    url,
                    dest,
                    category,
                    // The text the page showed for it, as the list's Description.
                    d.text,
                    size,
                    None,
                    context,
                    crate::i18n::mark("by Link grabber"),
                );
                // Just pushed: the last row (a search per link held a big batch up).
                if let Some(i) = self.items.last_mut().filter(|i| i.id == id) {
                    i.package = Some(tag.clone());
                }
                // A name the server didn't give (none looked up) may still change.
                if size.is_none() {
                    self.name_guessed(id, true);
                }
                self.queues[0].items.push(id);
                added.push(id);
            }
        }
        self.number_beside(&added);
        self.dirty = true;
        // Start runs the main queue (its files at once, as Options → Queue sets).
        if start && !added.is_empty() {
            self.start_queue(0);
        }
    }

    /// Link grabber's look-ups (Options → Archives → "Check links"): the next links, each
    /// with its own request (the page's referer and cookies), on the runtime.
    pub(super) fn grabber_checks(&mut self, ctx: &egui::Context) {
        if !self.settings.grabber_check {
            return;
        }
        let Some(g) = &mut self.grabber else { return };
        let answers = g.answers();
        for (i, url, d) in g.next_checks() {
            let context = ipc::Command {
                referer: d.referer.clone(),
                cookies: d.cookies.clone(),
                user_agent: d.user_agent.clone(),
                ..Default::default()
            };
            let parts = self.request_parts(&url, None, &context, false);
            let (out, ctx) = (answers.clone(), ctx.clone());
            self.rt.spawn(async move {
                let res = mdm_engine::probe(url, parts.headers, &parts.opts)
                    .await
                    .map_err(|e| crate::grabber::state_of(&e));
                // The window may be gone, and its answers with it.
                let _ = out.send((i, res));
                ctx.request_repaint();
            });
            if let Some(g) = &mut self.grabber {
                g.checking(i);
            }
        }
    }
}

/// Links or files dropped, sorted as the window used to on its own thread: lists of addresses
/// (a file of their kind there) read, addresses DDM downloads, magnet links and .torrent files
/// (a file there), the rest. Looks at and reads the disk: for a thread other than the window's
/// (#743).
pub(super) fn sort_dropped(items: Vec<String>) -> Dropped {
    let list_file = |s: &str| {
        let p = std::path::Path::new(s);
        p.is_file()
            && p.extension().is_some_and(|e| {
                ["txt", "ef2", "htm", "html", "csv"]
                    .contains(&e.to_string_lossy().to_ascii_lowercase().as_str())
            })
    };
    let mut sorted = Dropped::default();
    for s in items {
        if list_file(&s) {
            sorted.lists.push(read_list(std::path::Path::new(&s)));
        } else if let Some(path) = crate::metalinkdlg::metalink_path(&s).filter(|p| p.is_file()) {
            sorted.metalinks.push((
                path.display().to_string(),
                crate::metalinkdlg::read_file(&path),
            ));
        } else if url::Url::parse(s.trim()).is_ok_and(|u| mdm_engine::supported(&u)) {
            sorted.links.push(s);
        } else {
            let torrent = mdm_torrent::Source::magnet(&s).is_some()
                || (s.to_ascii_lowercase().ends_with(".torrent")
                    && std::path::Path::new(&s).is_file());
            sorted.other.push((s, torrent));
        }
    }
    sorted
}

/// A list of addresses read (Tasks → Import, a list file dropped): its file's name and the
/// addresses in it, or why it couldn't be read. Reads the disk: for a thread other than the
/// window's (#743).
pub(super) fn read_list(
    path: &std::path::Path,
) -> (String, Result<Vec<crate::transfer::Imported>, String>) {
    let file = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let read = std::fs::read(path)
        .map(|bytes| crate::transfer::import(&String::from_utf8_lossy(&bytes)))
        .map_err(|e| e.to_string());
    (file, read)
}
