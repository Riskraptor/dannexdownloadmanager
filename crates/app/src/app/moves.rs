//! Moving downloads: Move/Rename for one or several, a finished file moved on a thread of its own,
//! and the free place a download takes in its new folder.

use super::*;

/// A file move's answer, from its thread.
pub(super) type MoveDone = std::sync::mpsc::Receiver<Result<(), String>>;

impl App {
    /// Move a download to `target` (its name too): a finished file moves on a thread of its
    /// own (a copy to another drive takes a while), one not started just takes the new place.
    /// A failure says why.
    pub(super) fn start_move(&mut self, id: u64, target: PathBuf) {
        let Some(item) = self.item(id) else { return };
        if target == item.dest {
            return;
        }
        if item.status != Status::Complete {
            if item.downloaded == 0 {
                take_place(item, target);
                self.dirty = true;
            }
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let from = item.dest.clone();
        let to = target.clone();
        item.note(crate::i18n::t("Moving the file..."));
        std::thread::spawn(move || {
            let _ = tx.send(move_file(&from, &to));
        });
        self.moving.push((id, target, rx));
    }

    /// Moves that finished: the download takes its new place, or the user is told why not.
    pub(super) fn poll_moves(&mut self) {
        let mut done = Vec::new();
        self.moving.retain(|(id, target, rx)| match rx.try_recv() {
            Ok(r) => {
                done.push((*id, target.clone(), r));
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(_) => false,
        });
        for (id, target, r) in done {
            match r {
                Ok(()) => {
                    if let Some(item) = self.item(id) {
                        take_place(item, target.clone());
                    }
                    for c in &mut self.complete {
                        if c.id == id {
                            c.path = target.clone();
                        }
                    }
                    self.dirty = true;
                }
                Err(e) => {
                    if let Some(item) = self.item(id) {
                        item.note(crate::i18n::t("The file could not be moved"));
                    }
                    self.notice = Some((
                        crate::i18n::mark("Move/Rename"),
                        crate::i18n::fmt(
                            crate::i18n::t("The file could not be moved: {error}"),
                            &[("error", &e)],
                        ),
                    ));
                }
            }
        }
    }

    /// Move/Rename: a Save As box at the file's place; a finished file is moved there, one
    /// that hasn't started gets the new name and folder.
    pub(super) fn move_rename(&mut self, id: u64) {
        let Some(item) = self.items.iter().find(|i| i.id == id) else {
            return;
        };
        if !can_move(item) {
            // Said, not silently ignored (F2, Ctrl+M on a download that can't move).
            self.notice = Some(cant_move_notice());
            return;
        }
        let title = crate::i18n::t("Move/Rename").to_owned();
        let from = item
            .dest
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf();
        let name = item.file_name.clone();
        self.move_rename_for = Some(id);
        winui::start_pick(&self.ui_ctx, "app-move-rename", move || {
            rfd::FileDialog::new()
                .set_title(title)
                .set_directory(from)
                .set_file_name(name)
                .save_file()
        });
    }

    /// Move/Rename with several downloads selected (#333): their folder is asked for, not a name
    /// each.
    pub(super) fn move_many(&mut self, ids: Vec<u64>) {
        let can = ids
            .iter()
            .filter_map(|id| self.items.iter().find(|i| i.id == *id))
            .find(|i| can_move(i));
        let Some(first) = can else {
            self.notice = Some(cant_move_notice());
            return;
        };
        let title = crate::i18n::t("Move/Rename").to_owned();
        let from = first
            .dest
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf();
        self.move_many_for = ids;
        winui::start_pick(&self.ui_ctx, "app-move-many", move || {
            // Off-screen test runs have no folder dialog to answer.
            if let Some(dir) = std::env::var_os("MDM_TEST_MOVE_TO") {
                return Some(PathBuf::from(dir));
            }
            rfd::FileDialog::new()
                .set_title(title)
                .set_directory(from)
                .pick_folder()
        });
    }

    /// The several downloads' new folder: each that can go goes there, under its own name.
    pub(super) fn move_many_to(&mut self, ids: Vec<u64>, dir: PathBuf) {
        let mut stay = false;
        let mut moving = Vec::new();
        for id in ids {
            let Some(item) = self.items.iter().find(|i| i.id == id) else {
                continue;
            };
            if !can_move(item) {
                stay = true;
                continue;
            }
            if item.dest.parent() == Some(dir.as_path()) {
                continue;
            }
            moving.push((id, item.file_name.clone()));
        }
        self.dirty = true;
        if stay {
            self.notice = Some(cant_move_notice());
        }
        if !moving.is_empty() {
            // Their free names there are looked for beside the window (#743: `place_in` asks
            // the disk); they move once found.
            self.ask_beside(move || {
                let mut taken = std::collections::HashSet::new();
                Beside::Moves(
                    moving
                        .into_iter()
                        .map(|(id, name)| (id, place_in(&dir, &name, &mut taken)))
                        .collect(),
                )
            });
        }
    }
}

/// Move a file: a rename, else a copy and the original deleted (a copy whose original can't go,
/// in use, is taken back: the file stays in one place).
pub(super) fn move_file(from: &std::path::Path, to: &std::path::Path) -> Result<(), String> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to).map_err(|e| e.to_string())?;
    std::fs::remove_file(from).map_err(|e| {
        let _ = std::fs::remove_file(to);
        e.to_string()
    })
}

/// Whether Move/Rename can take a download now: a finished file, or nothing of it come in yet.
pub(super) fn can_move(i: &Item) -> bool {
    i.live.is_none() && (i.status == Status::Complete || i.downloaded == 0)
}

/// What is said when a download can't move, or stays while the others of a selection move.
pub(super) fn cant_move_notice() -> (&'static str, String) {
    (
        crate::i18n::mark("Move/Rename"),
        crate::i18n::t(
            "Only a finished download, or one not started yet, can be moved. Stop it and finish it first.",
        )
        .to_owned(),
    )
}

/// Where one download of a selection goes in `dir`: a name free there (on disk, what
/// `naming::destination` checks) and not one the selection has taken (two not started under one
/// name have no file yet to be seen).
pub(super) fn place_in(
    dir: &std::path::Path,
    name: &str,
    taken: &mut std::collections::HashSet<PathBuf>,
) -> PathBuf {
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    let mut at = mdm_engine::naming::destination(dir, name);
    let mut n = 1u32;
    while !taken.insert(at.clone()) {
        at = mdm_engine::naming::destination(dir, &format!("{stem} ({n}){ext}"));
        n += 1;
    }
    at
}

/// A download's new place (its name from it). A name the user gave is no longer the address's
/// guess (`Item::name_from_url`); another folder alone keeps the server's say.
pub(super) fn take_place(item: &mut Item, target: PathBuf) {
    let old_dest = item.dest.clone();
    if let Some(name) = target.file_name() {
        let name = name.to_string_lossy().into_owned();
        if name != item.file_name {
            item.name_from_url = false;
        }
        item.file_name = name;
    }
    if let Some(spec) = item.torrent.as_mut()
        && spec.folder == old_dest
    {
        spec.folder = target.clone();
    }
    if let Some(files) = item.written_files.as_mut() {
        for p in files {
            if let Ok(rel) = p.strip_prefix(&old_dest) {
                *p = target.join(rel);
            }
        }
    }
    item.dest = target;
}
