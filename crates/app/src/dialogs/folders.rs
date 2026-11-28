//! Folders and files in the way: a folder to create, files already there, and the gates that ask
//! once before downloads go on (`Gate`).

use super::*;

/// "This folder does not exist … Do you want to create it?" — asked when a download is about to
/// be added to a folder that isn't there (Windows' question, Yes / No). Yes creates it.
pub struct CreateFolder {
    pub(super) folder: PathBuf,
    pub(super) error: Option<String>,
    /// Yes was pressed: the folder is made off the window's thread (#743), its answer to come.
    pub(super) making: Option<crate::launch::Asked<PathBuf, Result<(), String>>>,
}

impl CreateFolder {
    pub fn new(folder: PathBuf) -> Self {
        CreateFolder {
            folder,
            error: None,
            making: None,
        }
    }

    /// `Done` once the folder exists (created on Yes); `Closed` on No, Escape or the close box.
    pub fn show(&mut self, ctx: &egui::Context) -> Outcome<()> {
        let folder = self.folder.display().to_string();
        question(
            ctx,
            "create-folder",
            "Create folder",
            [420.0, 128.0],
            false,
            winui::BG(),
            |ui, o| {
                let p = ui.painter().clone();
                let ink = winui::TEXT();
                winui::text(
                    &p,
                    o.x + 11.0,
                    o.y + 21.0,
                    crate::i18n::t("This folder does not exist:"),
                    ink,
                );
                let (rect, cut) =
                    winui::text_fit(&p, o.x + 11.0, o.y + 39.0, &folder, ink, o.x + 409.0);
                if cut {
                    winui::full_on_hover(ui, rect, "create-folder-path", &folder);
                }
                winui::text(
                    &p,
                    o.x + 11.0,
                    o.y + 57.0,
                    crate::i18n::t("Do you want to create it?"),
                    ink,
                );
                if let Some(e) = &self.error {
                    let (rect, cut) =
                        winui::text_fit(&p, o.x + 11.0, o.y + 75.0, e, winui::ERROR(), o.x + 409.0);
                    if cut {
                        winui::full_on_hover(ui, rect, "create-folder-error", e);
                    }
                }
                let yes = winui::button(
                    ui,
                    r(o, 254.0, 94.0, 73.0, 21.0),
                    "Yes",
                    ButtonKind::Default,
                    false,
                    None,
                )
                .clicked();
                let no = winui::button(
                    ui,
                    r(o, 336.0, 94.0, 73.0, 21.0),
                    "No",
                    ButtonKind::Normal,
                    false,
                    None,
                )
                .clicked();
                // Made beside the window (#743): a drive slow to answer doesn't hold it.
                let made = self.making.as_ref().and_then(|m| m.answer(&self.folder));
                if made.is_some() {
                    self.making = None;
                }
                if let Some(Err(e)) = &made {
                    self.error = Some(crate::i18n::fmt(
                        crate::i18n::t("The folder could not be created: {error}"),
                        &[("error", e)],
                    ));
                }
                if no || winui::escape_pressed(ui) {
                    Outcome::Closed
                } else if made == Some(Ok(())) {
                    Outcome::Done(())
                } else {
                    if (yes || winui::enter_pressed(ui)) && self.making.is_none() {
                        let making = crate::launch::Asked::default();
                        let _ = making.get(
                            &self.folder,
                            std::time::Duration::MAX,
                            std::time::Duration::ZERO,
                            Some(ui.ctx()),
                            |f| std::fs::create_dir_all(f).map_err(|e| e.to_string()),
                        );
                        self.making = Some(making);
                    }
                    Outcome::Open
                }
            },
        )
    }
}

/// "Some of the torrent's files are already in this folder … Use them?" — asked before a torrent
/// is added where files of its names exist (#40: librqbit writes into a file that is there, so
/// an unrelated file of the same name would be overwritten). Yes keeps them for the torrent:
/// what matches it is kept, the rest replaced (how data from elsewhere is resumed or shared).
pub struct ExistingFiles {
    /// The first file found (its full path), and how many in all.
    pub(super) first: String,
    pub(super) count: usize,
}

impl ExistingFiles {
    pub fn new(found: &[PathBuf]) -> Self {
        ExistingFiles {
            first: found
                .first()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            count: found.len(),
        }
    }

    /// `Done` on Yes; `Closed` on No, Escape or the close box.
    pub fn show(&mut self, ctx: &egui::Context) -> Outcome<()> {
        let first = crate::i18n::ltr(&self.first);
        // On its own line under the path, so a long path never cuts it off.
        let more = (self.count > 1).then(|| {
            // i18n: "files already there" question, under the first path: {n} = how many more
            crate::i18n::tn(
                (self.count - 1) as u64,
                "and {n} other file",
                "and {n} other files",
            )
        });
        question(
            ctx,
            "existing-files",
            "Files already there",
            [460.0, 206.0],
            false,
            winui::BG(),
            |ui, o| {
                let p = ui.painter().clone();
                let ink = winui::TEXT();
                let (left, right) = (o.x + 11.0, o.x + 449.0);
                let lines = winui::paragraph(
                    &p,
                    left,
                    o.y + 21.0,
                    crate::i18n::t("Some of the torrent's files are already in this folder:"),
                    ink,
                    right,
                    18.0,
                );
                let path_y = o.y + 21.0 + lines as f32 * 18.0;
                crate::fileicons::paint_at(
                    ui,
                    &p,
                    crate::fileicons::Kind::File(&self.first),
                    egui::pos2(left, path_y - 12.0),
                    16.0,
                );
                let (rect, cut) = winui::text_fit(
                    &p,
                    left + crate::fileicons::INDENT,
                    path_y,
                    &first,
                    ink,
                    right,
                );
                if cut {
                    winui::full_on_hover(ui, rect, "existing-files-path", &first);
                }
                let mut ask_y = path_y + 24.0;
                if let Some(more) = &more {
                    winui::text(&p, left, path_y + 18.0, more, winui::MUTED());
                    ask_y += 18.0;
                }
                // The two sentences wrap (translations run longer), above the buttons.
                let asked = winui::paragraph(
                    &p,
                    left,
                    ask_y,
                    crate::i18n::t(
                        "Use them for this torrent? What matches it is kept, the rest replaced.",
                    ),
                    ink,
                    right,
                    18.0,
                );
                winui::paragraph(
                    &p,
                    left,
                    ask_y + asked as f32 * 18.0,
                    crate::i18n::t("No goes back, to choose another folder."),
                    winui::MUTED(),
                    right,
                    18.0,
                );
                let yes = winui::button(
                    ui,
                    r(o, 294.0, 172.0, 73.0, 21.0),
                    "Yes",
                    ButtonKind::Normal,
                    false,
                    None,
                )
                .clicked();
                // No is the default: a file the user didn't mean to give up stays as it is.
                let no = winui::button(
                    ui,
                    r(o, 376.0, 172.0, 73.0, 21.0),
                    "No",
                    ButtonKind::Default,
                    false,
                    None,
                )
                .clicked();
                if yes {
                    Outcome::Done(())
                } else if no || winui::escape_pressed(ui) || winui::enter_pressed(ui) {
                    Outcome::Closed
                } else {
                    Outcome::Open
                }
            },
        )
    }
}

/// A dialog's "start" (`T`) held while what it would start into is looked at on the disk, off
/// the window's thread (#743: a sleeping hard disk, a USB drive or a network folder can take
/// seconds to answer, and the window waited for it; the dialog stays as it is meanwhile), then
/// while a question about it (`Q`) is up. Lives in the dialog, across frames.
pub struct Gate<T, A, Q> {
    pub(super) held: Option<T>,
    /// Where it was looked at, and the answer (`A`) to come.
    pub(super) look: Option<(PathBuf, crate::launch::Asked<PathBuf, A>)>,
    pub(super) question: Option<Q>,
}

/// [`ensure_folder`]'s: whether the folder is there; Create folder.
pub type FolderGate<T> = Gate<T, bool, CreateFolder>;

/// [`ensure_files_free`]'s: the torrent's files found there; Existing files.
pub type FilesGate<T> = Gate<T, Vec<PathBuf>, ExistingFiles>;

impl<T, A, Q> Default for Gate<T, A, Q> {
    fn default() -> Self {
        Gate {
            held: None,
            look: None,
            question: None,
        }
    }
}

impl<T, A: Clone + PartialEq + Send + 'static, Q> Gate<T, A, Q> {
    /// The dialog's "start", let go: the gate starts over.
    pub(super) fn release(&mut self) -> Outcome<T> {
        let held = self.held.take();
        *self = Gate::default();
        held.map_or(Outcome::Open, Outcome::Done)
    }

    /// `out` held until `at` has been looked at with `look` (made only when asked; it runs on
    /// a thread of its own) and the question `ask` makes of the answer (None: nothing to ask)
    /// was answered Yes by `show`. No goes back to the dialog.
    pub(super) fn pass<L: FnOnce(&PathBuf) -> A + Send + 'static>(
        &mut self,
        ctx: &egui::Context,
        out: Outcome<T>,
        at: &Path,
        look: impl FnOnce() -> L,
        ask: impl FnOnce(A) -> Option<Q>,
        show: impl FnOnce(&mut Q, &egui::Context) -> Outcome<()>,
    ) -> Outcome<T> {
        match out {
            Outcome::Closed => {
                *self = Gate::default();
                return Outcome::Closed;
            }
            Outcome::Done(v) if self.held.is_none() => self.held = Some(v),
            // While it is looked at, or the question is up, the dialog's own "start" waits.
            _ => {}
        }
        if self.held.is_none() {
            return Outcome::Open;
        }
        if self.question.is_none() {
            // Asked once for each place (the dialog's folder may change meanwhile); never waited.
            if self.look.as_ref().is_none_or(|(p, _)| p != at) {
                let asked = crate::launch::Asked::default();
                let _ = asked.get(
                    &at.to_owned(),
                    std::time::Duration::MAX,
                    std::time::Duration::ZERO,
                    Some(ctx),
                    look(),
                );
                self.look = Some((at.to_owned(), asked));
            }
            let found = self.look.as_ref().and_then(|(p, a)| a.answer(p));
            match found.map(ask) {
                None => return Outcome::Open,
                Some(None) => return self.release(),
                Some(question) => self.question = question,
            }
        }
        let Some(question) = self.question.as_mut() else {
            return Outcome::Open;
        };
        match show(question, ctx) {
            Outcome::Open => Outcome::Open,
            Outcome::Closed => {
                *self = Gate::default();
                Outcome::Open
            }
            Outcome::Done(()) => self.release(),
        }
    }
}

/// Holds a dialog's "start" (`out`) while files the torrent would write into already exist in
/// `folder` (looked for when the dialog says start, off the window's thread, by what `found`
/// makes): asks [`ExistingFiles`] and keeps the result meanwhile; No goes back to the dialog.
/// `gate` lives in the dialog, across frames.
pub fn ensure_files_free<T, F: FnOnce() -> Vec<PathBuf> + Send + 'static>(
    ctx: &egui::Context,
    gate: &mut FilesGate<T>,
    out: Outcome<T>,
    folder: &Path,
    found: impl FnOnce() -> F,
) -> Outcome<T> {
    gate.pass(
        ctx,
        out,
        folder,
        || {
            let found = found();
            move |_: &PathBuf| found()
        },
        |found| (!found.is_empty()).then(|| ExistingFiles::new(&found)),
        ExistingFiles::show,
    )
}

/// Holds a dialog's "start" (`out`) until `folder` exists: looked at off the window's thread,
/// and when it is missing, asks to create it ([`CreateFolder`]); No goes back to the dialog.
/// `gate` lives in the dialog, across frames.
pub fn ensure_folder<T>(
    ctx: &egui::Context,
    gate: &mut FolderGate<T>,
    out: Outcome<T>,
    folder: &Path,
) -> Outcome<T> {
    ensure_folder_with(ctx, gate, out, folder, |f| f.is_dir())
}

/// [`ensure_folder`], the folder looked at with `is_dir` (a test's).
pub(super) fn ensure_folder_with<T>(
    ctx: &egui::Context,
    gate: &mut FolderGate<T>,
    out: Outcome<T>,
    folder: &Path,
    is_dir: impl FnOnce(&Path) -> bool + Send + 'static,
) -> Outcome<T> {
    gate.pass(
        ctx,
        out,
        folder,
        || move |f: &PathBuf| f.as_os_str().is_empty() || is_dir(f.as_path()),
        |there| (!there).then(|| CreateFolder::new(folder.to_owned())),
        CreateFolder::show,
    )
}
