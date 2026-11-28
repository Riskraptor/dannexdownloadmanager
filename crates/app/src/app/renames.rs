//! Deferred renames: a name chosen while a download runs, applied once it has finished (a file or a
//! torrent's folder), beside the window and guarded against folders others share.

use super::*;

/// A finished download's deferred rename (`Item::rename_on_complete`), to do beside the window
/// (#743): copies of what the window knows (`deferred_rename_beside`).
pub(super) struct RenameJob {
    /// The row (`Item::place_copy`) and every row, for the checks of a folder shared with
    /// another download or a save folder.
    pub(super) me: Item,
    pub(super) items: Vec<Item>,
    pub(super) downloads: PathBuf,
    pub(super) settings: Settings,
    pub(super) rss: crate::rss::Store,
    /// The name asked for.
    pub(super) desired: String,
    /// A video's subtitle files waiting for their mark (`PendingMark::side_files`): renamed
    /// with it.
    pub(super) side_files: Vec<PathBuf>,
}

/// What became of a deferred rename done beside the window (#743).
pub(super) enum Renamed {
    /// Left as it is: why (the key of a reason said in its log), or nothing to say (no name,
    /// or the one it has).
    Refused(Option<&'static str>),
    /// A torrent's new place, held: `torrent_host` moves it there (and shares it on).
    Reserved {
        to: PathBuf,
        hold: Option<mdm_engine::naming::Reservation>,
        is_dir: bool,
    },
    /// Renamed to `to` (a number added when the name was taken meanwhile); a video's subtitles
    /// with it, at these paths.
    Done {
        to: PathBuf,
        is_dir: bool,
        side_files: Option<Vec<PathBuf>>,
    },
    /// The rename failed: the system's reason.
    Failed(String),
}

impl App {
    /// Pure in-memory stamp of everything that can change a progress window's
    /// `rename_disabled_reason`, so disk checks never run per frame while idle.
    pub(super) fn progress_rename_guard_stamp(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.downloads.hash(&mut h);
        self.settings.torrent_keep_sharing.hash(&mut h);
        let mut cat_dirs: Vec<_> = self.settings.category_dirs.iter().collect();
        cat_dirs.sort_unstable();
        cat_dirs.hash(&mut h);
        for c in &self.settings.custom_categories {
            c.id.hash(&mut h);
            c.name.hash(&mut h);
            c.folder.hash(&mut h);
        }
        for b in &self.settings.builtin_categories {
            b.category.hash(&mut h);
            b.deleted.hash(&mut h);
        }
        self.settings.folder_history.hash(&mut h);
        for r in &self.rss.rules {
            r.folder.hash(&mut h);
        }
        for i in &self.items {
            i.id.hash(&mut h);
            i.dest.hash(&mut h);
            i.file_name.hash(&mut h);
            (i.status == Status::Complete).hash(&mut h);
            i.gallery.is_some().hash(&mut h);
            i.preexisting_folder.hash(&mut h);
            if let Some(t) = &i.torrent {
                t.folder.hash(&mut h);
                t.files.hash(&mut h);
            }
            if let Some(r) = &i.remote {
                r.folder.hash(&mut h);
            }
        }
        h.finish()
    }

    /// Apply any confirmed deferred rename (`Item::rename_on_complete`) once download `id`
    /// completes. Returns `true` when the rename is still in flight (done beside the window,
    /// #743; for a torrent, through `torrent_host`, or queued behind it while it is busy): its
    /// answer then marks the files and offers the archive for extraction, with the final path.
    pub(super) fn apply_deferred_rename(&mut self, id: u64) -> bool {
        let confirmed = self
            .progress
            .iter_mut()
            .find(|p| p.id == id)
            .and_then(Progress::take_confirmed_rename);
        if let Some(confirmed) = confirmed
            && let Some(item) = self.item(id)
        {
            item.rename_on_complete = confirmed;
            self.dirty = true;
        }
        let Some(desired) = self
            .items
            .iter()
            .find(|i| i.id == id)
            .and_then(|i| i.rename_on_complete.clone())
        else {
            return false;
        };
        self.start_deferred_rename(id, desired)
    }

    /// Hand download `id`'s deferred rename to a thread of its own (`deferred_rename_beside`,
    /// #743): its folder checked, a free name held and the file or folder renamed there, never on
    /// the window's thread. Its answer comes to `poll_deferred_renames`. Returns true (in flight).
    pub(super) fn start_deferred_rename(&mut self, id: u64, desired: String) -> bool {
        if self.renaming.iter().any(|(r, _, _)| *r == id) {
            return true;
        }
        let Some(item) = self.items.iter().find(|i| i.id == id) else {
            return false;
        };
        let from = item.dest.clone();
        let torrent = item.torrent.is_some();
        let job = RenameJob {
            me: item.place_copy(),
            items: self.items.iter().map(Item::place_copy).collect(),
            downloads: self.downloads.clone(),
            settings: self.settings.clone(),
            rss: self.rss.clone(),
            desired,
            side_files: item
                .pending_mark
                .as_ref()
                .map(|pm| pm.side_files.clone())
                .unwrap_or_default(),
        };
        // Cleared before the rename runs, so an abrupt exit never leaves a stale
        // `rename_on_complete` pointing at the old `dest` (a torrent's stays until
        // `torrent_host` takes it: quitting before then doesn't lose it).
        if !torrent && let Some(item) = self.item(id) {
            item.rename_on_complete = None;
            self.dirty = true;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = self.ui_ctx.clone();
        self.rt.spawn_blocking(move || {
            let _ = tx.send(deferred_rename_beside(job));
            ctx.request_repaint();
        });
        self.renaming.push((id, from, rx));
        true
    }

    /// Deferred renames done beside the window (`start_deferred_rename`): the download takes its
    /// new place, or says why not; a torrent's goes on to `torrent_host`.
    pub(super) fn poll_deferred_renames(&mut self) {
        let mut done = Vec::new();
        self.renaming.retain(|(id, from, rx)| match rx.try_recv() {
            Ok(r) => {
                done.push((*id, from.clone(), r));
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(_) => false,
        });
        for (id, from, r) in done {
            self.apply_deferred_renamed(id, &from, r, true);
        }
    }

    /// Download `id`'s deferred rename, done beside the window (`deferred_rename_beside`), as it
    /// came back. `notify_ui`: a failure is told in a window, and the archive offered for
    /// extraction (not as DDM exits).
    pub(super) fn apply_deferred_renamed(
        &mut self,
        id: u64,
        from: &std::path::Path,
        r: Renamed,
        notify_ui: bool,
    ) {
        match r {
            Renamed::Reserved { to, hold, is_dir } => {
                if self.rename_torrent(id, from, to, hold, is_dir) {
                    return;
                }
            }
            Renamed::Refused(reason) => {
                if let Some(item) = self.item(id) {
                    item.rename_on_complete = None;
                    if let Some(key) = reason {
                        item.note(crate::i18n::t(key));
                    }
                }
                self.dirty = true;
            }
            Renamed::Done {
                to,
                is_dir,
                side_files,
            } => {
                if let Some(item) = self.item(id) {
                    item.rename_on_complete = None;
                    apply_renamed_paths_to_pending_mark(item, from, &to, is_dir, side_files);
                    // Unless the row went elsewhere meanwhile (Move/Rename): it keeps that place.
                    if item.dest == from {
                        take_place(item, to.clone());
                        for c in self.complete.iter_mut().filter(|c| c.id == id) {
                            c.path = to.clone();
                        }
                    }
                }
                self.dirty = true;
            }
            Renamed::Failed(err) => {
                let msg = crate::i18n::fmt(
                    crate::i18n::t("The file could not be moved: {error}"),
                    &[("error", &err)],
                );
                if let Some(item) = self.item(id) {
                    item.rename_on_complete = None;
                    item.note(msg.clone());
                }
                if notify_ui {
                    self.notice = Some((crate::i18n::mark("Move/Rename"), msg));
                }
                self.dirty = true;
            }
        }
        self.flush_pending_mark(id);
        if notify_ui {
            self.extracts.finished(id, &self.items, &self.settings);
            self.virus_check(id);
        }
    }

    /// A finished torrent's deferred rename, its new place `to` held (`hold`): `torrent_host`
    /// moves its files there (and shares it on). True while that runs, or waits for the torrent
    /// to be free (`deferred_torrent_queue`).
    pub(super) fn rename_torrent(
        &mut self,
        id: u64,
        from: &std::path::Path,
        to: PathBuf,
        hold: Option<mdm_engine::naming::Reservation>,
        is_dir: bool,
    ) -> bool {
        let Some(spec) = self
            .items
            .iter()
            .find(|i| i.id == id)
            .and_then(|i| i.torrent.clone())
        else {
            return false;
        };
        let (tx, rx) = std::sync::mpsc::channel();
        let known = self.session_hashes();
        match self.torrent_host.rename_finished(
            &self.rt,
            known,
            &spec,
            from.to_path_buf(),
            to,
            hold,
            is_dir,
            self.settings.torrent_keep_sharing,
            tx,
        ) {
            Ok(()) => {
                // Clear before the background rename moves the folder so an abrupt exit
                // never leaves a stale `rename_on_complete` pointing at the old `dest`.
                if let Some(item) = self.item(id) {
                    item.rename_on_complete = None;
                }
                self.renaming_torrents
                    .push((id, from.to_path_buf(), is_dir, rx));
                self.dirty = true;
            }
            Err(hold) => {
                drop(hold);
                // Keep `item.rename_on_complete` persisted in `downloads.json` while waiting
                // for `torrent_host` so quitting DDM before it starts does not lose it.
                if !self.deferred_torrent_queue.contains(&id) {
                    self.deferred_torrent_queue.push(id);
                }
                self.dirty = true;
            }
        }
        true
    }

    pub(super) fn apply_finished_torrent_rename(
        &mut self,
        id: u64,
        from: &std::path::Path,
        is_dir: bool,
        r: Result<PathBuf, String>,
        sharing_error: Option<String>,
        notify_ui: bool,
    ) {
        match r {
            Ok(actual_to) => {
                if let Some(item) = self.item(id) {
                    item.rename_on_complete = None;
                    apply_renamed_paths_to_pending_mark(item, from, &actual_to, is_dir, None);
                    take_place(item, actual_to.clone());
                }
                for c in &mut self.complete {
                    if c.id == id {
                        c.path = actual_to.clone();
                    }
                }
                self.dirty = true;
            }
            Err(e) => {
                let msg = crate::i18n::fmt(
                    crate::i18n::t("The file could not be moved: {error}"),
                    &[("error", &e)],
                );
                if let Some(item) = self.item(id) {
                    item.rename_on_complete = None;
                    item.note(msg.clone());
                }
                if notify_ui {
                    self.notice = Some((crate::i18n::mark("Move/Rename"), msg));
                }
                self.dirty = true;
            }
        }
        if let Some(why) = sharing_error
            && let Some(item) = self.item(id)
        {
            item.note(crate::i18n::fmt(
                // i18n: event log entry on a finished torrent when re-attaching it to keep sharing failed; {error} = why
                crate::i18n::t("Sharing stopped: {error}"),
                &[("error", &why)],
            ));
            self.dirty = true;
        }
        self.flush_pending_mark(id);
        if notify_ui {
            self.extracts.finished(id, &self.items, &self.settings);
            self.virus_check(id);
        }
    }

    /// Retry queued deferred torrent renames once `torrent_host` is free, and apply finished ones.
    pub(super) fn poll_deferred_torrent_renames(&mut self) {
        if !self.deferred_torrent_queue.is_empty() {
            let queued = std::mem::take(&mut self.deferred_torrent_queue);
            for id in queued {
                let busy = self
                    .items
                    .iter()
                    .find(|i| i.id == id)
                    .and_then(|i| i.torrent.as_ref())
                    .is_some_and(|spec| self.torrent_host.busy(&spec.hash));
                if busy {
                    if !self.deferred_torrent_queue.contains(&id) {
                        self.deferred_torrent_queue.push(id);
                    }
                } else if !self.renaming_torrents.iter().any(|(rid, ..)| *rid == id) {
                    let renaming = self.apply_deferred_rename(id);
                    if !renaming {
                        self.flush_pending_mark(id);
                        self.extracts.finished(id, &self.items, &self.settings);
                        self.virus_check(id);
                    }
                }
            }
        }

        let mut done = Vec::new();
        self.renaming_torrents
            .retain(|(id, from, is_dir, rx)| match rx.try_recv() {
                Ok(r) => {
                    done.push((*id, from.clone(), *is_dir, r));
                    false
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => true,
                Err(_) => false,
            });
        for (id, from, is_dir, (r, sharing_error)) in done {
            self.apply_finished_torrent_rename(id, &from, is_dir, r, sharing_error, true);
        }
    }

    pub(super) fn flush_pending_mark(&mut self, id: u64) {
        let Some(pm) = self.item(id).and_then(|i| i.pending_mark.take()) else {
            return;
        };
        let files: Vec<PathBuf> = pm
            .main_file
            .into_iter()
            .chain(pm.side_files)
            .chain(pm.gallery_files)
            .collect();
        if !files.is_empty() {
            mark_finished(files, pm.url, pm.referer);
        }
    }
}

/// Update `item.pending_mark` after `from` has been renamed to `actual_to` (rewriting saved file
/// paths inside a renamed gallery folder) so `flush_pending_mark` marks the final paths on disk.
/// `side_files`: a video's `yt-dlp` subtitle files as renamed alongside it
/// (`deferred_rename::rename_video_side_files`, beside the window: `deferred_rename_beside`);
/// None leaves them as they were.
pub(super) fn apply_renamed_paths_to_pending_mark(
    item: &mut Item,
    from: &std::path::Path,
    actual_to: &std::path::Path,
    is_dir: bool,
    side_files: Option<Vec<PathBuf>>,
) {
    let Some(pm) = item.pending_mark.as_mut() else {
        return;
    };
    if is_dir {
        for gf in &mut pm.gallery_files {
            if let Ok(rel) = gf.strip_prefix(from) {
                *gf = actual_to.join(rel);
            }
        }
    } else {
        if pm.main_file.is_some() {
            pm.main_file = Some(actual_to.to_path_buf());
        }
        if let Some(sides) = side_files {
            pm.side_files = sides;
        }
    }
}

/// Do `job`'s deferred rename: the folder checks (`deferred_rename_disabled_reason_with`), a free
/// name held (`deferred_rename::reserve_target_path`), and the rename itself, a video's
/// subtitles with it; a torrent's only up to its new place held. Looks at and changes the disk:
/// on a thread other than the window's (a sleeping hard disk, a USB drive or a network folder
/// can take seconds to answer, #743).
pub(super) fn deferred_rename_beside(job: RenameJob) -> Renamed {
    let from = &job.me.dest;
    let is_dir = crate::deferred_rename::is_folder_target(&job.me);
    if let Some(reason) = deferred_rename_disabled_reason_with(
        &job.me,
        &job.downloads,
        &job.settings,
        &job.rss,
        &job.items,
    ) {
        return Renamed::Refused(Some(reason));
    }
    let Some((to, hold)) = crate::deferred_rename::reserve_target_path(from, &job.desired, is_dir)
    else {
        return Renamed::Refused(None);
    };
    if job.me.torrent.is_some() {
        return Renamed::Reserved { to, hold, is_dir };
    }
    match crate::deferred_rename::rename_reserved_or_number(from, to, hold, is_dir) {
        Ok(to) => {
            let side_files = (!is_dir && !job.side_files.is_empty()).then(|| {
                crate::deferred_rename::rename_video_side_files(from, &to, &job.side_files)
            });
            Renamed::Done {
                to,
                is_dir,
                side_files,
            }
        }
        Err(e) => Renamed::Failed(e.to_string()),
    }
}

/// Whether `folder` is a root/category/save folder or contains another download's files, making a
/// folder-level deferred rename unsafe for `id`.
pub(super) fn is_protected_or_shared_folder_with(
    id: u64,
    folder: &std::path::Path,
    downloads: &std::path::Path,
    settings: &Settings,
    rss: &crate::rss::Store,
    items: &[Item],
) -> bool {
    use crate::deferred_rename::{path_is_at_or_inside_dir, paths_refer_to_same_dir};

    if paths_refer_to_same_dir(folder, downloads)
        || paths_refer_to_same_dir(
            folder,
            &settings.category_dir(model::Category::General, downloads),
        )
    {
        return true;
    }
    for cat in model::Category::BUILT_IN {
        if paths_refer_to_same_dir(folder, &settings.category_dir(cat, downloads)) {
            return true;
        }
    }
    for (_, dir) in &settings.category_dirs {
        if !dir.is_empty() && paths_refer_to_same_dir(folder, std::path::Path::new(dir)) {
            return true;
        }
    }
    for c in &settings.custom_categories {
        if paths_refer_to_same_dir(
            folder,
            &settings.category_dir(model::Category::Custom(c.id), downloads),
        ) {
            return true;
        }
        if let Some(dir) = c.folder.as_deref().filter(|d| !d.is_empty())
            && paths_refer_to_same_dir(folder, std::path::Path::new(dir))
        {
            return true;
        }
    }
    for dir in &settings.folder_history {
        if !dir.is_empty() && paths_refer_to_same_dir(folder, std::path::Path::new(dir)) {
            return true;
        }
    }
    for rule in &rss.rules {
        if !rule.folder.is_empty()
            && paths_refer_to_same_dir(folder, std::path::Path::new(&rule.folder))
        {
            return true;
        }
    }
    for sys in [
        dirs::home_dir(),
        dirs::desktop_dir(),
        dirs::download_dir(),
        dirs::document_dir(),
    ]
    .into_iter()
    .flatten()
    {
        if paths_refer_to_same_dir(folder, &sys) {
            return true;
        }
    }
    items.iter().filter(|o| o.id != id).any(|other| {
        path_is_at_or_inside_dir(&other.dest, folder)
            || other
                .torrent
                .as_ref()
                .is_some_and(|t| path_is_at_or_inside_dir(&t.folder, folder))
            || other
                .remote
                .as_ref()
                .is_some_and(|r| path_is_at_or_inside_dir(&r.folder, folder))
    })
}

/// Whether a folder download's destination holds files that this download did not write.
pub(super) fn folder_has_untracked_item_files(item: &Item) -> bool {
    if item.preexisting_folder {
        return true;
    }
    if let Some(files) = &item.written_files {
        return crate::deferred_rename::folder_has_untracked_files(&item.dest, files);
    }
    if let Some(spec) = &item.torrent
        && spec.folder == item.dest
        && item.dest.is_dir()
    {
        let manifest = crate::torrents::torrent_file(&spec.hash)
            .and_then(|f| std::fs::read(f).ok())
            .and_then(|b| mdm_torrent::files_from_bytes(&b).ok());
        if let Some(entries) = manifest {
            let allowed: Vec<PathBuf> = entries
                .into_iter()
                .enumerate()
                .filter(|(i, f)| {
                    f.padding || spec.files.as_ref().is_none_or(|chosen| chosen.contains(i))
                })
                .map(|(_, f)| item.dest.join(f.path))
                .collect();
            return crate::deferred_rename::folder_has_untracked_files(&item.dest, &allowed);
        }
    }
    false
}

/// Translated reason key explaining why deferred rename is unavailable for `item`, or `None`
/// when `item` can be renamed safely on completion.
pub(super) fn deferred_rename_disabled_reason_with(
    item: &Item,
    downloads: &std::path::Path,
    settings: &Settings,
    rss: &crate::rss::Store,
    items: &[Item],
) -> Option<&'static str> {
    if crate::deferred_rename::is_folder_target(item) {
        if is_protected_or_shared_folder_with(item.id, &item.dest, downloads, settings, rss, items)
        {
            // i18n: tooltip on the disabled Rename box when the folder holds another download or is a save/category folder
            return Some(crate::i18n::mark(
                "This folder is shared with another download or save folder",
            ));
        }
        if folder_has_untracked_item_files(item) {
            // i18n: tooltip on the disabled Rename box when the folder already holds files this download did not write
            return Some(crate::i18n::mark(
                "This folder holds files from outside this download",
            ));
        }
        return None;
    }
    if crate::deferred_rename::is_split_archive_volume(item, items) {
        // i18n: tooltip on the disabled Rename box when the file is one part of a split archive (.part1.rar, .7z.001…)
        return Some(crate::i18n::mark(
            "Split archive volumes keep their names for extraction",
        ));
    }
    if item.torrent.is_some() {
        // i18n: tooltip on the disabled Rename box for a single-file torrent
        return Some(crate::i18n::mark("A single-file torrent keeps its name"));
    }
    None
}
