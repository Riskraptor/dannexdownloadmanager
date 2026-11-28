//! Downloads taken off the list, with their files or without: the files deleted beside the window
//! (#743), the folders DDM made for them pruned, and Delete's questions.

use super::*;

/// What replacing a download takes from the disk ([`replacement`]), as the window knows it:
/// read and removed beside the window ([`Removal::files`], [`Removal::run`]; #743).
pub(super) struct Removal {
    /// Its finished file, state and part file.
    pub(super) files: Vec<PathBuf>,
    /// A video's, with Options' overwrite: its folder and stem, whose files yt-dlp left are
    /// found there (`mdm_tools::video::own_files`), but none named after another download's
    /// name or stem and a dot there (`named`, lowercase).
    pub(super) yt_dlp: Option<(PathBuf, String, Vec<String>)>,
    /// The other downloads' files (lowercase): never removed.
    pub(super) others: Vec<String>,
}

impl Removal {
    /// The files to remove: those known, and a video's yt-dlp left, found in its folder. Reads
    /// the disk: for a thread other than the window's.
    pub(super) fn files(self) -> Vec<PathBuf> {
        let low = |p: &std::path::Path| p.to_string_lossy().to_lowercase();
        let mut files = self.files;
        if let Some((dir, stem, named)) = &self.yt_dlp {
            for f in mdm_tools::video::own_files(dir, stem, true) {
                let name = f
                    .file_name()
                    .map(|n| n.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                if !named.iter().any(|n| name.starts_with(n)) && !files.contains(&f) {
                    files.push(f);
                }
            }
            // The engine's pieces folders for its streams (#769), removed whole.
            for f in mdm_tools::video::own_folders(dir, stem) {
                let name = f
                    .file_name()
                    .map(|n| n.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                if !named.iter().any(|n| name.starts_with(n)) && !files.contains(&f) {
                    files.push(f);
                }
            }
        }
        files.retain(|f| !self.others.contains(&low(f)));
        files
    }

    /// Remove them (one that isn't there, or can't go, is left as it is). On a thread other
    /// than the window's.
    pub(super) fn run(self) {
        for f in self.files() {
            let _ = remove_path(&f);
        }
    }

    /// What it removes, as the window knows it before ([`Removing`]): its known files, and a
    /// video's stem whose files yt-dlp left are found beside the window.
    pub(super) fn removing(&self) -> Removing {
        Removing::of(
            &self.files,
            self.yt_dlp.as_ref().map(|(_, stem, _)| stem.as_str()),
        )
    }
}

/// What a download about to start may write in its folder, for the removals under way there to
/// be waited for ([`Removing::hits`], #743).
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Writes {
    /// A file of this name (lowercase), and its part and state files (`<name>.*`).
    Named(String),
    /// A video of this stem (lowercase): yt-dlp takes any `<stem>.*` as its own (#32).
    Stem(String),
    /// A name not known yet (the server names it, #28; a folder's names read for a new one's):
    /// anything there.
    Folder,
}

impl Writes {
    /// What download `item` writes as it starts, in its folder.
    pub(super) fn of(item: &Item) -> Writes {
        let low = |s: &std::ffi::OsStr| s.to_string_lossy().to_lowercase();
        match (item.dest.file_name(), item.dest.file_stem()) {
            (_, Some(stem)) if item.video.is_some() => Writes::Stem(low(stem)),
            (Some(name), _) if !item.name_from_url => Writes::Named(low(name)),
            _ => Writes::Folder,
        }
    }
}

/// A download's files being removed beside the window (#743: Delete with its files, a cancelled
/// or restarted download's, Overwrite's): a start that would write among them waits until they
/// are gone (`App::start_look`, `StartLook::clearing`), and so does a look at their folder for a
/// new download's name (`App::number_beside_after`, `App::add_video`, `App::add_playlist`), as
/// when they went on the window's thread: nothing written meanwhile is removed, and no name is
/// numbered on for a file about to go.
pub(super) struct Removing {
    /// Their folder (lowercase).
    pub(super) folder: String,
    /// Their names (lowercase), and a video's stem whose `<stem>.*` files go too (Overwrite).
    pub(super) names: Vec<String>,
    pub(super) stem: Option<String>,
    /// The waits to tell once they are gone; None once they are.
    pub(super) waits: std::sync::Arc<std::sync::Mutex<Option<Vec<std::sync::mpsc::Sender<()>>>>>,
}

/// Told when a [`Removing`]'s files are gone: dropped by the thread that removed them (its end
/// tells them however it ends: a panic doesn't leave a start waiting for ever).
pub(super) struct RemovingDone(
    pub(super) std::sync::Arc<std::sync::Mutex<Option<Vec<std::sync::mpsc::Sender<()>>>>>,
);

impl Drop for RemovingDone {
    fn drop(&mut self) {
        let waits = self.0.lock().unwrap_or_else(|e| e.into_inner()).take();
        for w in waits.into_iter().flatten() {
            let _ = w.send(());
        }
    }
}

impl Removing {
    /// `files` (one download's, in one folder) and a video's `stem` (its yt-dlp files) being
    /// removed; the handle the remover drops once they are gone.
    pub(super) fn of(files: &[PathBuf], stem: Option<&str>) -> Removing {
        let low = |s: &std::ffi::OsStr| s.to_string_lossy().to_lowercase();
        Removing {
            folder: files
                .first()
                .and_then(|f| f.parent())
                .map(|d| low(d.as_os_str()))
                .unwrap_or_default(),
            names: files
                .iter()
                .filter_map(|f| f.file_name())
                .map(low)
                .collect(),
            stem: stem.map(str::to_lowercase),
            waits: std::sync::Arc::new(std::sync::Mutex::new(Some(Vec::new()))),
        }
    }

    /// The handle that tells the waits when dropped (on the remover's thread).
    pub(super) fn done(&self) -> RemovingDone {
        RemovingDone(self.waits.clone())
    }

    /// Still under way.
    pub(super) fn under_way(&self) -> bool {
        self.waits
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }

    /// Whether a download writing `writes` in `folder` would write among its files: a name of
    /// it, or one after it and a dot, or a video's stem and a dot, letter case aside.
    pub(super) fn hits(&self, folder: &std::path::Path, writes: &Writes) -> bool {
        if folder.to_string_lossy().to_lowercase() != self.folder {
            return false;
        }
        let after =
            |name: &str, first: &str| name == first || name.starts_with(&format!("{first}."));
        match writes {
            Writes::Folder => true,
            Writes::Named(n) => {
                self.names.iter().any(|f| after(f, n))
                    || self.stem.as_deref().is_some_and(|s| after(n, s))
            }
            Writes::Stem(s) => {
                self.names.iter().any(|f| f.starts_with(&format!("{s}.")))
                    || self
                        .stem
                        .as_deref()
                        .is_some_and(|t| after(s, t) || after(t, s))
            }
        }
    }

    /// A wait for its files to go, for a download writing `writes` in `folder`; None when it
    /// writes none of them, or they are gone already.
    pub(super) fn wait(
        &self,
        folder: &std::path::Path,
        writes: &Writes,
    ) -> Option<std::sync::mpsc::Receiver<()>> {
        if !self.hits(folder, writes) {
            return None;
        }
        let mut waits = self.waits.lock().unwrap_or_else(|e| e.into_inner());
        let (tell, wait) = std::sync::mpsc::channel();
        waits.as_mut()?.push(tell);
        Some(wait)
    }
}

/// One download's Delete with its files, handed from the window's thread to beside it
/// (`App::remove_download`, `App::delete_beside`, #743): its files ([`replacement`]: a video's
/// yt-dlp leftovers and pieces folders too), its finished file when it was finished (a failure to
/// remove that one is said), its name for that message, and the folder DDM made for it, removed
/// once empty ([`Prune`]).
pub(super) struct Deleting {
    pub(super) removal: Removal,
    pub(super) finished: Option<PathBuf>,
    pub(super) name: String,
    pub(super) prune: Option<Prune>,
}

/// The folders DDM made for downloads as they started, beside the window (`StartLook`): which
/// download, and the topmost folder that wasn't there. Taken into the rows by the window
/// (`App::take_made`).
pub(super) static MADE_FOLDERS: std::sync::Mutex<Vec<(u64, PathBuf)>> =
    std::sync::Mutex::new(Vec::new());

/// A folder DDM made for a download (`made`) and the folder its files were in (`folder`, inside
/// it): once they are gone, the empty ones from `folder` up to `made` go too ([`prune_made`]).
/// Never one of `keep` (the downloads folder, the categories', the user's own).
pub(super) struct Prune {
    pub(super) folder: PathBuf,
    pub(super) made: PathBuf,
    pub(super) keep: Vec<PathBuf>,
}

impl Prune {
    pub(super) fn run(&self) {
        prune_made(&self.folder, &self.made, &self.keep);
    }
}

impl App {
    /// Take a download off the list; its files stay.
    pub(super) fn delete(&mut self, id: u64) {
        self.remove_downloads(&[id], false);
    }

    /// Take downloads off the list (not while they run; see [`Self::remove_download`]). When
    /// the selection goes with them, the row that comes after them is selected, as in Explorer,
    /// so Delete can be pressed again.
    pub(super) fn remove_downloads(&mut self, ids: &[u64], files: bool) {
        let before = self.shown_ids();
        self.extracts.forget(ids);
        let deleting = ids
            .iter()
            .filter_map(|id| self.remove_download(*id, files))
            .collect();
        self.delete_beside(deleting);
        let after = self.shown_ids();
        self.selection.after_removal(&before, &after);
    }

    /// Download `id` went through a debrid account and ended (done, or taken off the list): the
    /// service forgets its link (TorBox's list of web downloads, #391). A paused one keeps it,
    /// to go on with.
    pub(super) fn release_link(&self, id: u64) {
        let Some(i) = self.items.iter().find(|i| i.id == id) else {
            return;
        };
        if !self.settings.debrid_on
            || i.direct
            || i.remote.is_some()
            || i.torrent.is_some()
            || i.video.is_some()
            || i.gallery.is_some()
        {
            return;
        }
        let (debrid, url) = (self.debrid.clone(), i.url.clone());
        self.rt.spawn(async move {
            if let Err(e) = debrid.release(&url).await {
                tracing::warn!("debrid: the link stays in the service's list: {e:?}");
            }
        });
    }

    /// Take a download off the list (not while it runs); with `files`, its file goes from disk
    /// too: a finished file, or the data so far (a torrent's files when its session runs). The
    /// row leaves at once; its files are handed back, for the caller to remove beside the window
    /// (`delete_beside`, #743: a sleeping hard disk, a USB drive or a network folder can take
    /// seconds to answer), a download that would write among them meanwhile (added again,
    /// started) waiting for them to go (`Removing`).
    #[must_use]
    pub(super) fn remove_download(&mut self, id: u64, files: bool) -> Option<Deleting> {
        let mut deleting = None;
        if self.item(id).is_some_and(|i| i.live.is_none()) {
            self.release_link(id);
            self.take_made();
            if let Some(spec) = self.item(id).and_then(|i| i.torrent.clone()) {
                self.torrent_host.forget(&self.rt, &spec.hash, files);
            } else if files
                && let Some(i) = self.items.iter().find(|i| i.id == id)
                // A gallery's folder stays (it may hold other files; #58).
                && i.gallery.is_none()
                && let Some(removal) = replacement(&self.items, id, true)
            {
                // The file itself: one that can't go (open in a player) is said, not skipped
                // silently (`delete_files`); the row still leaves the list. A video's leftovers
                // (yt-dlp's, the engine's pieces folders) go with it.
                deleting = Some(Deleting {
                    removal,
                    finished: (i.status == Status::Complete).then(|| i.dest.clone()),
                    name: i.file_name.clone(),
                    prune: self.prune_for(i),
                });
            }
            self.items.retain(|i| i.id != id);
            for q in &mut self.queues {
                q.items.retain(|i| *i != id);
            }
            self.progress.retain(|p| p.id != id);
            self.clearing.remove(&id);
            self.selection.forget(id);
            self.dirty = true;
        }
        deleting
    }

    /// Delete with its files, for the downloads taken off the list (`remove_download`): their
    /// files removed beside the window, one after the other on one thread (#743), each under way
    /// meanwhile (`Removing`: a start among them waits for its own to go). A failure's message
    /// (`delete_files`) shows once it is done.
    pub(super) fn delete_beside(&mut self, deleting: Vec<Deleting>) {
        if deleting.is_empty() {
            return;
        }
        self.removing.retain(Removing::under_way);
        let mut done = Vec::with_capacity(deleting.len());
        for d in &deleting {
            let removing = d.removal.removing();
            done.push(removing.done());
            self.removing.push(removing);
        }
        let ctx = self.ui_ctx.clone();
        self.rt.spawn_blocking(move || {
            for (d, done) in deleting.into_iter().zip(done) {
                let files = d.removal.files();
                delete_files(&files, d.finished.as_deref(), &d.name);
                if let Some(prune) = &d.prune {
                    prune.run();
                }
                drop(done);
            }
            ctx.request_repaint();
        });
    }

    /// Run `remove` beside the window (#743), its files (`removing`) under way meanwhile: a start
    /// that would write among them, or a look at their folder for a new download's name, waits
    /// for them to go ([`Removing`]). A message it leaves (`dialogs::tell`) shows once it is done.
    pub(super) fn remove_beside(
        &mut self,
        removing: Removing,
        remove: impl FnOnce() + Send + 'static,
    ) {
        self.removing.retain(Removing::under_way);
        let done = removing.done();
        self.removing.push(removing);
        let ctx = self.ui_ctx.clone();
        self.rt.spawn_blocking(move || {
            remove();
            drop(done);
            ctx.request_repaint();
        });
    }

    /// The folders downloads made as they started (`note_made`), into their rows: each keeps the
    /// first it made (a later start finds it there).
    pub(super) fn take_made(&mut self) {
        let made = std::mem::take(&mut *MADE_FOLDERS.lock().unwrap_or_else(|e| e.into_inner()));
        for (id, dir) in made {
            if let Some(item) = self.item(id)
                && item.made_dir.is_none()
            {
                item.made_dir = Some(dir);
                self.dirty = true;
            }
        }
    }

    /// The folders never removed with a download, though DDM may have made them: the downloads
    /// folder, every category's, Options' temporary folder and the user's own (home, Desktop,
    /// Downloads, Documents).
    pub(super) fn kept_folders(&self) -> Vec<PathBuf> {
        let downloads = &self.downloads;
        let mut keep = vec![downloads.clone()];
        keep.push(
            self.settings
                .category_dir(model::Category::General, downloads),
        );
        for cat in model::Category::BUILT_IN {
            keep.push(self.settings.category_dir(cat, downloads));
        }
        for c in &self.settings.custom_categories {
            keep.push(
                self.settings
                    .category_dir(model::Category::Custom(c.id), downloads),
            );
        }
        keep.extend(
            self.settings
                .category_dirs
                .iter()
                .filter(|(_, d)| !d.is_empty())
                .map(|(_, d)| PathBuf::from(d)),
        );
        if !self.settings.temp_dir.is_empty() {
            keep.push(PathBuf::from(&self.settings.temp_dir));
        }
        keep.extend(
            [
                dirs::home_dir(),
                dirs::desktop_dir(),
                dirs::download_dir(),
                dirs::document_dir(),
            ]
            .into_iter()
            .flatten(),
        );
        keep
    }

    /// What of `item`'s folder goes with it once its files are gone: the folder DDM made for it,
    /// if it made one (`Item::made_dir`), only while empty.
    pub(super) fn prune_for(&self, item: &Item) -> Option<Prune> {
        let made = item.made_dir.clone()?;
        let folder = item.dest.parent()?.to_owned();
        Some(Prune {
            folder,
            made,
            keep: self.kept_folders(),
        })
    }

    /// The waits for the removals under way that a look at `folder` for a new download's name
    /// writing `writes` has (#743): the names are read once those files are gone.
    pub(super) fn removals_in(
        &mut self,
        folder: Option<&std::path::Path>,
        writes: &Writes,
    ) -> Vec<std::sync::mpsc::Receiver<()>> {
        self.removing.retain(Removing::under_way);
        waits_for(&self.removing, folder, writes)
    }

    /// Delete / Remove / Delete All Completed: asks first (unless "Don't show this dialog
    /// again" was ticked) whether only the list entries go or the files too; `files` ticks that
    /// box (Shift+Delete). Running downloads are left alone.
    pub(super) fn ask_delete(&mut self, ids: Vec<u64>, files: bool) {
        let gone: Vec<&Item> = ids
            .iter()
            .filter_map(|id| self.items.iter().find(|i| i.id == *id && i.live.is_none()))
            .collect();
        let Some(first) = gone.first() else { return };
        // "Also the files" starts off every time; only Shift+Delete ticks it, this once (user,
        // 2026-11-25: a remembered tick erased files on later deletes).
        if self.settings.confirm_delete {
            let one = gone.len() == 1;
            self.confirm_delete = Some(dialogs::ConfirmDelete::new(
                gone.iter().map(|i| i.id).collect(),
                first.file_name.clone(),
                one && first.status != Status::Complete,
                files,
            ));
        } else {
            let ids: Vec<u64> = gone.into_iter().map(|i| i.id).collect();
            self.remove_downloads(&ids, files);
        }
    }

    pub(super) fn delete_completed(&mut self) {
        let done = self
            .items
            .iter()
            .filter(|i| i.status == Status::Complete && i.listed())
            .map(|i| i.id)
            .collect();
        self.ask_delete(done, false);
    }
}

/// The waits a download writing `writes` in `folder` has for the removals under way (#743).
pub(super) fn waits_for(
    removing: &[Removing],
    folder: Option<&std::path::Path>,
    writes: &Writes,
) -> Vec<std::sync::mpsc::Receiver<()>> {
    let Some(folder) = folder else {
        return Vec::new();
    };
    removing
        .iter()
        .filter_map(|r| r.wait(folder, writes))
        .collect()
}

/// Download `id` is about to write in `folder`: the topmost of it and its parents that isn't
/// there yet is DDM's to make, and to remove again once empty (`Item::made_dir`). Reads the disk:
/// beside the window.
pub(super) fn note_made(id: u64, folder: Option<&std::path::Path>) {
    let Some(mut dir) = folder else {
        return;
    };
    let mut top = None;
    while !dir.as_os_str().is_empty() && !dir.exists() {
        top = Some(dir);
        match dir.parent() {
            Some(p) => dir = p,
            None => break,
        }
    }
    if let Some(top) = top.filter(|t| t.parent().is_some()) {
        MADE_FOLDERS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((id, top.to_owned()));
    }
}

/// Remove `folder` and its parents up to `made` (a folder DDM made), each only while it is empty
/// and isn't one of `keep`; nothing when `folder` isn't `made` or inside it. Beside the window.
pub(super) fn prune_made(folder: &std::path::Path, made: &std::path::Path, keep: &[PathBuf]) {
    use crate::deferred_rename::{path_is_at_or_inside_dir, paths_refer_to_same_dir};
    let mut dir = folder.to_path_buf();
    loop {
        if !path_is_at_or_inside_dir(&dir, made)
            || keep.iter().any(|k| path_is_at_or_inside_dir(k, &dir))
        {
            return;
        }
        let last = paths_refer_to_same_dir(&dir, made);
        // Only an empty folder goes: anything in it (the user's own) keeps it.
        if std::fs::remove_dir(&dir).is_err() || last {
            return;
        }
        match dir.parent() {
            Some(p) => dir = p.to_owned(),
            None => return,
        }
    }
}

/// Remove a file, or one of the engine's pieces folders (`<file>.ddm.pieces`, #769) with what is
/// in it. Beside the window.
pub(super) fn remove_path(p: &std::path::Path) -> std::io::Result<()> {
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if (name.ends_with(".ddm.pieces") || name.ends_with(".mdm.pieces")) && p.is_dir() {
        std::fs::remove_dir_all(p)
    } else {
        std::fs::remove_file(p)
    }
}

/// A download's files that Delete with its files removes (#743: beside the window,
/// [`delete_files`]): its finished file when it is finished (`complete`), its part and state
/// files.
pub(super) fn files_of(dest: &std::path::Path, complete: bool) -> Vec<PathBuf> {
    // Its part file beside it, or in Options' temporary directory.
    let mut files = vec![state_path(dest)];
    files.extend(mdm_engine::naming::part_paths(dest));
    if complete {
        files.insert(0, dest.to_owned());
    }
    // Under their former names too (`.mdm.part`, Dannex up to 0.6.5).
    files.extend(mdm_engine::state::legacy_files(dest));
    files
}

/// Delete with its files, beside the window (#743): `files` removed ([`Removal::files`]); the
/// `finished` one that can't go (open in a player) is said, not skipped silently (`file_name`,
/// the row's name); one that isn't there, or a part file that can't go, is left as it is.
pub(super) fn delete_files(files: &[PathBuf], finished: Option<&std::path::Path>, file_name: &str) {
    // A part file kept where an earlier temporary directory was (Options → Save to): its state
    // says where.
    for f in files
        .iter()
        .filter(|f| f.to_string_lossy().ends_with(".ddm.json"))
    {
        if let Some(part) = mdm_engine::state::ResumeState::load(f)
            .ok()
            .and_then(|s| s.part)
        {
            let _ = std::fs::remove_file(part);
        }
    }
    for f in files {
        match remove_path(f) {
            Err(e) if finished == Some(f.as_path()) && e.kind() != std::io::ErrorKind::NotFound => {
                dialogs::tell(
                    crate::i18n::mark("Delete"),
                    crate::i18n::fmt(
                        crate::i18n::t("{file} could not be deleted from the disk: {error}"),
                        &[
                            ("file", &crate::i18n::isolate(file_name)),
                            ("error", &e.to_string()),
                        ],
                    ),
                );
            }
            _ => {}
        }
    }
}

/// What replacing download `id` takes from the disk, found without it (#743: [`Removal`]); None
/// when it isn't in `items` or runs (it is left alone, the new copy then gets a numbered name).
/// Its finished file, state and part file, as for any download; with `yt_dlp` (Options'
/// overwrite, chosen: never #342's automatic one), for a video also what yt-dlp left for it
/// (`mdm_tools::video::own_files`). Never a file another download may own: its file, or one
/// named after its name or stem and a dot (its part files, running or paused) in that folder,
/// letter case aside.
pub(super) fn replacement(items: &[Item], id: u64, yt_dlp: bool) -> Option<Removal> {
    let item = items.iter().find(|i| i.id == id)?;
    if item.live.is_some() {
        return None;
    }
    let low = |p: &std::path::Path| p.to_string_lossy().to_lowercase();
    let others: Vec<&Item> = items.iter().filter(|i| i.id != id).collect();
    let dest = &item.dest;
    let mut files = Vec::new();
    if item.status == Status::Complete {
        files.push(dest.clone());
    }
    files.push(state_path(dest));
    files.extend(mdm_engine::naming::part_paths(dest));
    // Under their former names too (`.mdm.part`, Dannex up to 0.6.5).
    files.extend(mdm_engine::state::legacy_files(dest));
    let yt_dlp = match (dest.parent(), dest.file_stem()) {
        (Some(dir), Some(stem)) if yt_dlp && item.video.is_some() => {
            let folder = low(dir);
            let named = others
                .iter()
                .filter(|o| o.dest.parent().is_some_and(|p| low(p) == folder))
                .flat_map(|o| [o.dest.file_name(), o.dest.file_stem()])
                .flatten()
                .map(|n| format!("{}.", n.to_string_lossy().to_lowercase()))
                .collect();
            Some((dir.to_owned(), stem.to_string_lossy().into_owned(), named))
        }
        _ => None,
    };
    Some(Removal {
        files,
        yt_dlp,
        others: others.iter().map(|o| low(&o.dest)).collect(),
    })
}

/// Remove `files` (one that isn't there, or can't go, is left as it is). For a thread other than
/// the window's (`App::remove_beside`).
pub(super) fn remove_files(files: &[PathBuf]) {
    for f in files {
        let _ = remove_path(f);
    }
}

/// A stopped download's files: its state and part files, and its finished file when it was
/// complete (a Stop-then-remove of a download that finished meanwhile). For a thread other than
/// the window's.
#[cfg(test)]
pub(super) fn remove_files_of(dest: &std::path::Path, complete: bool) {
    remove_files(&files_of(dest, complete));
}
