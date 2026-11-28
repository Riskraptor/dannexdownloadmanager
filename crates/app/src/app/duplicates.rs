//! The duplicate check of a new download's address (Options → Downloads → "If a duplicate download
//! link is added"): its question, the looks at the disk it waits for, and what then goes on.

use super::*;

/// What was being added when its address turned out to be in the list already.
pub(super) enum Pending {
    /// From the browser or the command line.
    Command(Box<ipc::Command>),
    /// From Add URL.
    Url(dialogs::NewUrl),
    /// A video from Download video from a page, with the browser's quality menu's look-up when
    /// it was opened for its choice: it goes with the new download only, never to the one
    /// already there.
    Video(Box<crate::videodlg::VideoAdd>),
}

/// What Options → Downloads → "If a duplicate download link is added" (0 ask, 1 numbered name,
/// 2 overwrite, 3 open or resume) makes of a new download ([`duplicate_step`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DupStep {
    /// Added: no duplicate, or added again under a numbered name.
    Add,
    /// Added once the one already there has gone, its files and its row: Options' overwrite
    /// (`true`: a video's yt-dlp files too, [`replacement`]), or its finished file is gone
    /// (#342, nothing to duplicate; `false`: its row, state and part file only, as for any file).
    /// Its row leaves the list at once, its files are removed beside the window (#743), and the
    /// new one is added once they are gone.
    Replace(u64, bool),
    /// Not added: the one already there is opened (finished) or resumed.
    Existing(u64),
    /// Not added yet: the user is asked (dialogs::Duplicate).
    Ask(u64),
    /// Not decided yet: finished download `dup`'s file is looked for beside the window (#743:
    /// a sleeping hard disk, a USB drive or a network folder can take seconds to answer), for
    /// #342; the new one waits for the answer, then goes as [`duplicate_step`] says.
    Look(u64),
}

/// What the duplicate question's answer does ([`answered`]).
pub(super) enum Answer {
    /// Open or resume the one already there (3): what was being added is dropped, a video's
    /// look-up with it.
    Existing(u64),
    /// Add it: under a numbered name (1), or over the one there (2, `Some`).
    Add(Option<u64>, Pending),
    /// The one asked about has left the list meanwhile: checked afresh, as if just added.
    Again(Pending),
}

/// A new download held after the duplicate check until an answer from beside the window comes
/// (#743): what was being added, and what it waits for (`App::poll_duplicates`).
pub(super) struct DupWait {
    pub(super) pending: Pending,
    pub(super) on: DupOn,
}

/// What a held duplicate waits for.
pub(super) enum DupOn {
    /// Whether finished download `dup`'s file at `dest` is gone (#342), for Options' rule.
    Look {
        dup: u64,
        dest: PathBuf,
        gone: std::sync::mpsc::Receiver<bool>,
    },
    /// The files of the one it replaces (off the list already) removed.
    Removal(std::sync::mpsc::Receiver<()>),
}

impl App {
    /// Options → Downloads → "If a duplicate download link is added", for a new download that
    /// the first row `same` finds already is: what becomes of it ([`duplicate_seen`]), decided
    /// without the disk (#743). The one already there is opened or resumed here; holding the new
    /// one (asked about, waiting for a finished one's file to be looked for, or for the files of
    /// the one replaced to go) is the caller's ([`Self::after_duplicate`]).
    pub(super) fn duplicate_of(&mut self, same: impl Fn(&Item) -> bool) -> DupStep {
        if std::mem::take(&mut self.duplicate_cleared) {
            return DupStep::Add;
        }
        // Not one waiting for Download File Info's answer: not in the list yet.
        let found = self.items.iter().find(|i| i.listed() && same(i));
        let step = duplicate_seen(self.settings.duplicate_action, found);
        if let DupStep::Existing(dup) = step {
            self.open_or_resume(dup);
        }
        step
    }

    /// What becomes of a new download (`pending`) after the duplicate check's `step`: true when
    /// it goes on now; else it is asked about, waits for an answer from beside the window
    /// (#743: a finished duplicate's file looked for, #342; the files of the one it replaces
    /// removed, its row off the list already), or is dropped (the one there opened or resumed).
    pub(super) fn after_duplicate(
        &mut self,
        step: DupStep,
        pending: impl FnOnce() -> Pending,
    ) -> bool {
        match step {
            DupStep::Add => true,
            DupStep::Existing(_) => false,
            DupStep::Ask(dup) => {
                self.ask_duplicate(dup, pending());
                false
            }
            DupStep::Look(dup) => {
                self.look_duplicate(dup, pending());
                false
            }
            DupStep::Replace(dup, yt_dlp) => {
                self.replace(dup, yt_dlp, pending());
                false
            }
        }
    }

    /// Whether finished download `dup`'s file is gone (#342), looked for beside the window
    /// (#743): `pending` waits for the answer (`poll_duplicates`).
    pub(super) fn look_duplicate(&mut self, dup: u64, pending: Pending) {
        let dest = self
            .items
            .iter()
            .find(|i| i.id == dup)
            .map(|i| i.dest.clone());
        let (tx, gone) = std::sync::mpsc::channel();
        if let Some(at) = dest.clone() {
            let ctx = self.ui_ctx.clone();
            self.rt.spawn_blocking(move || {
                let _ = tx.send(model::finished_file_gone(Status::Complete, &at));
                ctx.request_repaint();
            });
        }
        // Else (it left the list): `tx` is gone, and `pending` is checked afresh at the next look.
        self.dup_waits.push(DupWait {
            pending,
            on: DupOn::Look {
                dup,
                dest: dest.unwrap_or_default(),
                gone,
            },
        });
    }

    /// "Duplicate download link" about `dup`, for `pending`. One question at a time; the others
    /// wait their turn (a second link while the first was asked was dropped).
    pub(super) fn ask_duplicate(&mut self, dup: u64, pending: Pending) {
        let name = self
            .items
            .iter()
            .find(|i| i.id == dup)
            .map(|i| i.file_name.clone())
            .unwrap_or_default();
        let ask = (dup, pending, dialogs::Duplicate::new(name));
        if self.duplicate.is_none() {
            self.duplicate = Some(ask);
        } else {
            self.duplicates_waiting.push_back(ask);
        }
    }

    /// Options → Downloads → "If a duplicate download link is added": true when adding `url`
    /// goes on now. 0 asks (`pending` waits), 1 adds it with a numbered name, 2 replaces the
    /// download already there (`pending` waits for its files to go), 3 opens that one
    /// (finished) or resumes it; a finished one's file is looked for first (#342, #743).
    pub(super) fn duplicate_gate(
        &mut self,
        url: &url::Url,
        pending: impl FnOnce() -> Pending,
    ) -> bool {
        let step = self.duplicate_of(|i| i.url == *url);
        self.after_duplicate(step, pending)
    }

    /// Download video's Download Now / Later, through the same check as any other download:
    /// the same page in the same quality is a duplicate, another quality is not. A question
    /// opens its own window (DDM's stays where it is). What became of it.
    pub(super) fn add_video_checked(&mut self, mut add: crate::videodlg::VideoAdd) -> DupStep {
        // The Subtitles choice is offered again for the next video (#68), whatever comes of
        // this one.
        if let Some(subs) = add.remember.take() {
            self.settings.video_subs = subs;
            self.dirty = true;
        }
        let (page, selector) = (add.page.clone(), add.selector.clone());
        let step = self.duplicate_of(|i| same_video(i, &page, &selector));
        match video_route(step, add) {
            VideoRoute::Add(add) => self.add_video(*add),
            VideoRoute::Wait(pending) => {
                self.after_duplicate(step, || pending);
            }
            VideoRoute::Dropped => {}
        }
        step
    }

    /// The user's answer to "Duplicate download link".
    pub(super) fn resolve_duplicate(
        &mut self,
        dup: u64,
        pending: Pending,
        choice: usize,
        ctx: &egui::Context,
    ) {
        let there = self.items.iter().any(|i| i.id == dup);
        match answered(dup, there, choice, pending) {
            Answer::Existing(dup) => self.open_or_resume(dup),
            Answer::Again(pending) => self.duplicate_again(pending, ctx),
            // Over the one there: added once its files are gone (#743).
            Answer::Add(Some(dup), pending) => self.replace(dup, true, pending),
            Answer::Add(None, pending) => self.go_on(pending, ctx),
        }
    }

    /// What was held at the duplicate check goes on, the check passed: added as it would have
    /// been with no duplicate.
    pub(super) fn go_on(&mut self, pending: Pending, ctx: &egui::Context) {
        match pending {
            Pending::Command(cmd) => {
                self.duplicate_cleared = true;
                self.handle_command(*cmd, ctx);
                self.duplicate_cleared = false;
            }
            Pending::Url(new) => self.open_file_info(new, Default::default(), ctx),
            Pending::Video(add) => self.add_video(*add),
        }
    }

    /// What was asked about a download that has left the list meanwhile: checked afresh, as
    /// when it came (it may be a duplicate of another, or of nothing now).
    pub(super) fn duplicate_again(&mut self, pending: Pending, ctx: &egui::Context) {
        match pending {
            Pending::Command(cmd) => self.handle_command(*cmd, ctx),
            Pending::Url(new) => {
                if self.duplicate_gate(&new.url, || {
                    Pending::Url(dialogs::NewUrl {
                        url: new.url.clone(),
                        login: new.login.clone(),
                    })
                }) {
                    self.open_file_info(new, Default::default(), ctx);
                }
            }
            Pending::Video(add) => {
                self.add_video_checked(*add);
            }
        }
    }

    /// The new download replaces download `id` (Options' overwrite, or #342's finished file
    /// gone): its row leaves the list now (nothing starts it meanwhile), its files go beside the
    /// window (#743, [`replacement`]; `yt_dlp`: a video's files left by yt-dlp too, so that the
    /// new copy takes the same name), and `pending` is added once they are gone
    /// (`poll_duplicates`). A running one is left alone: the new copy then gets a numbered name.
    pub(super) fn replace(&mut self, id: u64, yt_dlp: bool, pending: Pending) {
        let (tx, removed) = std::sync::mpsc::channel();
        if let Some(removal) = replacement(&self.items, id, yt_dlp) {
            self.delete(id);
            self.remove_beside(removal.removing(), move || {
                removal.run();
                let _ = tx.send(());
            });
        }
        // Else nothing to wait for: `tx` is gone, and the new one goes on at the next look.
        self.dup_waits.push(DupWait {
            pending,
            on: DupOn::Removal(removed),
        });
    }

    /// The answers from beside the window that new downloads held at the duplicate check wait
    /// for (#743), in the order they were held: a finished duplicate's file looked for decides
    /// as Options say (#342: gone, it is replaced; one that changed meanwhile, or left the list,
    /// is checked afresh), and the files of one replaced gone let the new one be added.
    pub(super) fn poll_duplicates(&mut self, ctx: &egui::Context) {
        if self.dup_waits.is_empty() {
            return;
        }
        use std::sync::mpsc::TryRecvError;
        let mut ready = Vec::new();
        for wait in std::mem::take(&mut self.dup_waits) {
            let answer = match &wait.on {
                DupOn::Look { gone, .. } => match gone.try_recv() {
                    Ok(gone) => Some(Some(gone)),
                    Err(TryRecvError::Empty) => None,
                    // The look ended without an answer: checked afresh.
                    Err(TryRecvError::Disconnected) => Some(None),
                },
                DupOn::Removal(removed) => match removed.try_recv() {
                    Err(TryRecvError::Empty) => None,
                    Ok(()) | Err(TryRecvError::Disconnected) => Some(None),
                },
            };
            match answer {
                Some(gone) => ready.push((wait, gone)),
                None => self.dup_waits.push(wait),
            }
        }
        for (DupWait { pending, on }, gone) in ready {
            match on {
                DupOn::Removal(_) => self.go_on(pending, ctx),
                DupOn::Look { dup, dest, .. } => {
                    let action = self.settings.duplicate_action;
                    match looked(action, &self.items, dup, &dest, gone) {
                        Some(step) => {
                            if let DupStep::Existing(dup) = step {
                                self.open_or_resume(dup);
                            }
                            let mut held = Some(pending);
                            if self.after_duplicate(step, || held.take().expect("held once"))
                                && let Some(pending) = held
                            {
                                self.go_on(pending, ctx);
                            }
                        }
                        None => self.duplicate_again(pending, ctx),
                    }
                }
            }
        }
    }
}

/// [`DupStep`] for Options' `action` when `found` is the download already there and whether its
/// finished file is gone (#342).
pub(super) fn duplicate_step(action: u8, found: Option<(u64, bool)>) -> DupStep {
    match found {
        None => DupStep::Add,
        Some((dup, true)) => DupStep::Replace(dup, false),
        Some((dup, false)) => match action {
            1 => DupStep::Add,
            2 => DupStep::Replace(dup, true),
            3 => DupStep::Existing(dup),
            _ => DupStep::Ask(dup),
        },
    }
}

/// What finished duplicate `dup`'s look beside the window (#743) comes to: Options' step for
/// `action` with its answer (`gone`: its file at `dest` is gone, #342), or None when the new
/// download is to be checked afresh: the look ended without an answer, or the row left the list
/// or changed meanwhile (not finished now, moved, started).
pub(super) fn looked(
    action: u8,
    items: &[Item],
    dup: u64,
    dest: &std::path::Path,
    gone: Option<bool>,
) -> Option<DupStep> {
    let unchanged = items
        .iter()
        .any(|i| i.id == dup && i.status == Status::Complete && i.dest == dest && i.live.is_none());
    gone.filter(|_| unchanged)
        .map(|gone| duplicate_step(action, Some((dup, gone))))
}

/// [`duplicate_step`] as the window's thread can decide it, without the disk (#743): no
/// duplicate, or one not finished (whose file isn't expected yet: never gone, #342), now; a
/// finished one's file is looked for beside the window first ([`DupStep::Look`]).
pub(super) fn duplicate_seen(action: u8, found: Option<&Item>) -> DupStep {
    match found {
        Some(i) if i.status == Status::Complete => DupStep::Look(i.id),
        found => duplicate_step(action, found.map(|i| (i.id, false))),
    }
}

/// The duplicate question about `dup` (`there`: still in the list) answered `choice`, numbered
/// as Options lists them.
pub(super) fn answered(dup: u64, there: bool, choice: usize, pending: Pending) -> Answer {
    match choice {
        _ if !there => Answer::Again(pending),
        3 => Answer::Existing(dup),
        2 => Answer::Add(Some(dup), pending),
        _ => Answer::Add(None, pending),
    }
}
