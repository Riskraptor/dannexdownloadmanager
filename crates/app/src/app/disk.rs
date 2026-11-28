//! What the disk says of the list: names already taken by other files (numbered beside the
//! window), finished downloads whose file is gone (File missing), and leftover temporary files.

use super::*;

/// A download added without a look at its folder (`App::number_beside`, #743): its id, the
/// place it was given, and the place free there (the same, else numbered).
pub(super) type Numbered = (u64, PathBuf, PathBuf);

impl App {
    /// Download `id`'s name was given without waiting for its folder to be looked at (Download
    /// File Info, #743): it starts under it, or the next free numbered name if it is taken by
    /// then (`Item::number_at_start`).
    pub(super) fn number_at_start(&mut self, id: u64) {
        if let Some(item) = self.item(id) {
            item.number_at_start = true;
        }
    }

    /// Downloads `ids`, just added without Download File Info under the names they were given
    /// (`naming::guess`: the clipboard, a command line, Add batch, Link grabber, the site
    /// grabber, a paired device): each starts under its name, or the next free numbered one if
    /// it is taken by then (`number_at_start`), so it waits for no disk now (#743); and their
    /// folders are looked at beside the window, a name taken there showing numbered once the
    /// answer comes, as when they were looked at as they were added (`poll_numbering`).
    pub(super) fn number_beside(&mut self, ids: &[u64]) {
        self.number_beside_after(ids, None);
    }

    /// [`Self::number_beside`], once the files of a download they replace (Overwrite, taken off
    /// the list already) are removed there first: their folders are looked at, and they start,
    /// only then (their start waits for it as for Redownload's removal, `StartLook`). Files of
    /// their names being removed already (Delete with its files…, `Removing`) are gone before
    /// their folders are looked at, as when they went on the window's thread.
    pub(super) fn number_beside_after(&mut self, ids: &[u64], removal: Option<Removal>) {
        let places: Vec<(u64, PathBuf)> = ids
            .iter()
            .filter_map(|&id| {
                let item = self.item(id)?;
                item.number_at_start = true;
                Some((id, item.dest.clone()))
            })
            .collect();
        if places.is_empty() {
            return;
        }
        // Before this one's own removal is under way: its look doesn't wait for itself.
        let waits: Vec<_> = places
            .iter()
            .flat_map(|(_, at)| {
                let name = at.file_name().map(|n| n.to_string_lossy().to_lowercase());
                let writes = name.map_or(Writes::Folder, Writes::Named);
                self.removals_in(at.parent(), &writes)
            })
            .collect();
        let removing = removal.as_ref().map(|r| {
            let removing = r.removing();
            let done = removing.done();
            self.removing.push(removing);
            done
        });
        let gone: Vec<std::sync::mpsc::Sender<()>> = match removal {
            Some(_) => places
                .iter()
                .map(|&(id, _)| {
                    let (gone, clearing) = std::sync::mpsc::channel();
                    self.clearing.insert(id, clearing);
                    gone
                })
                .collect(),
            None => Vec::new(),
        };
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = self.ui_ctx.clone();
        self.rt.spawn_blocking(move || {
            for w in waits {
                let _ = w.recv();
            }
            let found = numbered_after(removal, gone, places);
            drop(removing);
            let _ = tx.send(found);
            ctx.request_repaint();
        });
        self.numbering.push(rx);
    }

    /// The names `number_beside` found taken: a download still waiting under the name it was
    /// given (not started, renamed nor moved meanwhile) shows the numbered one it will start
    /// under. Its name stays the address's guess if it was one.
    pub(super) fn poll_numbering(&mut self) {
        let mut found = Vec::new();
        self.numbering.retain(|rx| match rx.try_recv() {
            Ok(answer) => {
                found.extend(answer);
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(_) => false,
        });
        for (id, given, free) in found {
            if free == given {
                continue;
            }
            let Some(item) = self.item(id) else { continue };
            if item.live.is_some()
                || !item.number_at_start
                || item.downloaded > 0
                || item.dest != given
            {
                continue;
            }
            if let Some(name) = free.file_name() {
                item.file_name = name.to_string_lossy().into_owned();
            }
            item.dest = free;
            self.dirty = true;
        }
    }

    /// Finished downloads whose file is gone show "File missing" (with Download Again): looked
    /// for beside the window every few seconds (#743), one look at a time. An answer counts only
    /// while the row's file is still the one looked for (moved or renamed meanwhile: the next
    /// look's).
    pub(super) fn poll_missing(&mut self) {
        if let Some(rx) = &self.missing_look {
            match rx.try_recv() {
                Ok(found) => {
                    self.missing_look = None;
                    for (id, dest, gone) in found {
                        if let Some(item) = self.item(id)
                            && item.dest == dest
                            && item.status == Status::Complete
                        {
                            item.missing = gone;
                        }
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(_) => self.missing_look = None,
            }
        }
        if Instant::now() < self.missing_due {
            return;
        }
        self.missing_due = Instant::now() + Duration::from_secs(12);
        let look: Vec<(u64, PathBuf)> = self
            .items
            .iter()
            .filter(|i| {
                i.status == Status::Complete
                    && i.live.is_none()
                    && i.gallery.is_none()
                    && i.torrent.is_none()
                    && i.remote.is_none()
            })
            .map(|i| (i.id, i.dest.clone()))
            .collect();
        if look.is_empty() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = self.ui_ctx.clone();
        self.rt.spawn_blocking(move || {
            let found: Vec<_> = look
                .into_iter()
                .map(|(id, dest)| {
                    let gone = model::finished_file_gone(Status::Complete, &dest);
                    (id, dest, gone)
                })
                .collect();
            let changed = !found.is_empty();
            let _ = tx.send(found);
            if changed {
                ctx.request_repaint();
            }
        });
        self.missing_look = Some(rx);
    }

    /// Look for leftover temporary files (`crate::leftovers`) beside the window and remove them:
    /// at start those a day old (said only to the log), on Downloads → Clean Up Temporary Files
    /// those a few minutes old (one being written now is no leftover), its answer said.
    pub(super) fn sweep_leftovers(&mut self, asked: bool) {
        if self.leftovers.is_some() {
            return;
        }
        let mut folders = self.kept_folders();
        folders.extend(
            self.items
                .iter()
                .filter_map(|i| i.dest.parent().map(PathBuf::from)),
        );
        let temp = Some(PathBuf::from(self.settings.temp_dir.trim())).filter(|d| d.is_absolute());
        let owners = self
            .items
            .iter()
            .map(|i| leftovers::Owner {
                dest: i.dest.clone(),
                video: i.video.is_some(),
            })
            .collect();
        let plan = leftovers::Plan {
            folders,
            temp,
            owners,
            older_than: Duration::from_secs(if asked { 5 * 60 } else { 24 * 60 * 60 }),
        };
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = self.ui_ctx.clone();
        self.rt.spawn_blocking(move || {
            let swept = leftovers::sweep(&plan);
            let _ = tx.send(swept);
            ctx.request_repaint();
        });
        self.leftovers = Some((rx, asked));
    }

    /// The start's sweep once the window is up, and a sweep's answer.
    pub(super) fn poll_leftovers(&mut self) {
        if std::mem::take(&mut self.sweep_at_start) {
            self.sweep_leftovers(false);
        }
        let Some((rx, asked)) = &self.leftovers else {
            return;
        };
        let asked = *asked;
        let swept = match rx.try_recv() {
            Ok(swept) => swept,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(_) => leftovers::Swept::default(),
        };
        self.leftovers = None;
        if swept.count > 0 || swept.failed > 0 {
            tracing::info!(
                "leftover temporary files: {} removed ({} bytes), {} in use",
                swept.count,
                swept.bytes,
                swept.failed
            );
        }
        if !asked {
            return;
        }
        let mut text = if swept.count == 0 {
            // i18n: Downloads → Clean Up Temporary Files found nothing to remove.
            crate::i18n::t("No leftover temporary files were found.").to_owned()
        } else {
            crate::i18n::fmt(
                // i18n: Downloads → Clean Up Temporary Files' answer: {count} files, {size} like "1.2 GB".
                crate::i18n::t("Removed {count} leftover temporary files ({size} freed)."),
                &[
                    ("count", &swept.count.to_string()),
                    ("size", &model::size(swept.bytes)),
                ],
            )
        };
        if swept.failed > 0 {
            text.push('\n');
            text.push_str(&crate::i18n::fmt(
                // i18n: Clean Up Temporary Files: some files are open in another program.
                crate::i18n::t("{count} could not be removed (in use by another program)."),
                &[("count", &swept.failed.to_string())],
            ));
        }
        // i18n: The title of Clean Up Temporary Files' answer.
        dialogs::tell(crate::i18n::mark("Clean Up Temporary Files"), text);
    }

    /// File missing (#5) about download `id`, unless it is being asked already.
    pub(super) fn ask_missing(&mut self, id: u64, name: &str, dest: &std::path::Path) {
        if self.missing_ask.is_none() {
            self.missing_ask = Some(crate::missing::Missing::new(id, name, dest));
        }
    }

    /// Files Open or Open folder didn't find (`crate::missing::report`): a finished download's
    /// row says "File missing" and File missing asks what to do; any other path gets what it
    /// always got (Open says the file is gone, Open folder opens its folder).
    pub(super) fn files_gone(&mut self) {
        for gone in crate::missing::take() {
            let Some(id) = crate::missing::row_of(&self.items, &gone) else {
                if gone.folder {
                    dialogs::open_folder_of_gone(&gone.path);
                } else {
                    dialogs::open_gone(&gone.path);
                }
                continue;
            };
            let Some(item) = self.item(id) else { continue };
            item.missing = true;
            let name = item.file_name.clone();
            self.ask_missing(id, &name, &gone.path);
        }
    }

    /// File missing's answer, and the file a Locate... chooser picked: the row then names that
    /// file (its folder and name), no longer missing.
    pub(super) fn missing_dialogs(&mut self, ctx: &egui::Context) {
        self.files_gone();
        if let Some(d) = &mut self.missing_ask {
            match d.show(ctx) {
                Outcome::Open => {}
                Outcome::Closed => self.missing_ask = None,
                Outcome::Done(choice) => {
                    let (id, path) = (d.id, d.path.clone());
                    self.missing_ask = None;
                    match choice {
                        // Nothing of it is on the disk to remove first: it starts again.
                        crate::missing::Choice::Redownload => self.redownload(id),
                        crate::missing::Choice::Locate => {
                            crate::missing::start_locate(ctx, id, &path);
                            if !self.locating.contains(&id) {
                                self.locating.push(id);
                            }
                        }
                        crate::missing::Choice::OpenFolder => {
                            dialogs::open_folder_of_gone(&path);
                        }
                    }
                }
            }
        }
        for id in std::mem::take(&mut self.locating) {
            match crate::missing::located(ctx, id) {
                Some(path) => self.relocate(id, path),
                // Still open (or cancelled: then it never answers and is dropped below).
                None if crate::missing::locating(ctx, id) => self.locating.push(id),
                None => {}
            }
        }
    }

    /// Download `id`'s file is at `path` now (File missing → Locate...): the row follows it.
    pub(super) fn relocate(&mut self, id: u64, path: PathBuf) {
        let Some(item) = self.item(id) else { return };
        if let Some(name) = path.file_name() {
            item.file_name = name.to_string_lossy().into_owned();
        }
        item.note(crate::i18n::fmt(
            // i18n: event log entry: the finished download's file was found elsewhere with File missing → Locate...; {path} is where it is now
            crate::i18n::t("Located at {path}"),
            &[("path", &crate::i18n::ltr(&path.to_string_lossy()))],
        ));
        item.dest = path;
        item.missing = false;
        self.dirty = true;
    }
}

/// Where each download added under `places` (id, place) would be on the disk now: its name, or
/// the next free numbered one if a file (or a part file, or a download in this process) has it,
/// as `naming::destination` gives. Each on its own, as when they were added one by one: rows of
/// one name wait under it, the first to start keeps it (#442). For a thread other than the
/// window's.
pub(super) fn numbered(places: Vec<(u64, PathBuf)>) -> Vec<Numbered> {
    places
        .into_iter()
        .map(|(id, at)| {
            let free = match (at.parent(), at.file_name()) {
                (Some(dir), Some(name)) => {
                    mdm_engine::naming::unique_path(dir, &name.to_string_lossy())
                }
                _ => at.clone(),
            };
            (id, at, free)
        })
        .collect()
}

/// [`numbered`] once the files of the download they replace (`removal`, Overwrite's) are gone,
/// each of `gone` told then (their starts wait for it, `StartLook::clearing`). For a thread
/// other than the window's (#743).
pub(super) fn numbered_after(
    removal: Option<Removal>,
    gone: Vec<std::sync::mpsc::Sender<()>>,
    places: Vec<(u64, PathBuf)>,
) -> Vec<Numbered> {
    if let Some(removal) = removal {
        removal.run();
    }
    for g in gone {
        let _ = g.send(());
    }
    numbered(places)
}
