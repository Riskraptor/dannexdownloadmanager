//! A finished download whose file was deleted or moved outside DDM (#5). Open and Open folder
//! look for the file on a thread of their own (#743); when it is gone they never hand the path
//! to the system (Windows only says it can't find it), they [`report`] it. The app then marks the
//! row "File missing" and asks what to do ([`Missing`]): Redownload, Locate... (where it is now),
//! Open folder (where it was), or Cancel. A path that is no download's (a torrent's file, a crash
//! report) gets what it got before: Open says the file is gone, Open folder opens its folder.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use egui::vec2;

use crate::dialogs::Outcome;
use crate::winui::{self, ButtonKind, r};

/// A file Open or Open folder didn't find.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gone {
    pub path: PathBuf,
    /// Asked by Open folder (else Open).
    pub folder: bool,
}

static GONE: Mutex<Vec<Gone>> = Mutex::new(Vec::new());

/// Open (`folder` false) or Open folder found no file at `path`: for the app to take
/// ([`take`]). From any thread.
pub fn report(path: &Path, folder: bool) {
    GONE.lock().unwrap().push(Gone {
        path: path.to_owned(),
        folder,
    });
}

/// What [`report`] was given since the last call, oldest first.
pub fn take() -> Vec<Gone> {
    std::mem::take(&mut *GONE.lock().unwrap())
}

/// What the person chose in [`Missing`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Redownload,
    Locate,
    OpenFolder,
}

/// "File missing": a finished download's file is no longer where it was saved.
pub struct Missing {
    pub id: u64,
    name: String,
    pub path: PathBuf,
}

impl Missing {
    pub fn new(id: u64, name: &str, path: &Path) -> Self {
        Missing {
            id,
            name: name.to_owned(),
            path: path.to_owned(),
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Outcome<Choice> {
        let key = "file-missing";
        let extra = crate::dialogs::grow_extra(ctx, key);
        let text = crate::i18n::fmt(
            // i18n: File missing window: {file} = the downloaded file's name; it finished, but another program deleted or moved it since. Redownload, Locate..., Open folder and Cancel are its buttons
            crate::i18n::t(
                "{file} is no longer where it was saved: it was deleted or moved since it finished. Download it again, or show DDM where it is now.",
            ),
            &[("file", &crate::i18n::isolate(&self.name))],
        );
        crate::dialogs::question(
            ctx,
            key,
            // i18n: title of the window asking what to do with a finished download whose file was deleted or moved
            crate::i18n::mark("File missing"),
            [440.0, 104.0 + extra],
            false,
            winui::BG(),
            |ui, o| {
                let p = ui.painter().clone();
                let lines = winui::paragraph(
                    &p,
                    o.x + 11.0,
                    o.y + 22.0,
                    &text,
                    winui::TEXT(),
                    o.x + 429.0,
                    13.0,
                );
                crate::dialogs::grow_to(ui.ctx(), key, lines, 3);
                let o = o + vec2(0.0, extra);
                let row = [
                    // i18n: button in File missing: download the file again
                    (crate::i18n::mark("Redownload"), 81.0),
                    // i18n: button in File missing: choose where the file is now (opens a file chooser)
                    (crate::i18n::mark("Locate..."), 81.0),
                    // i18n: button in File missing: open the folder the file was saved in
                    (crate::i18n::mark("Open folder"), 81.0),
                    (crate::i18n::mark("Cancel"), 73.0),
                ];
                let placed = winui::buttons_to_right(&p, 429.0, 7.0, &row, 140.0);
                let mut button = |i: usize, kind| {
                    let (x, w) = placed[i];
                    winui::button(ui, r(o, x, 72.0, w, 21.0), row[i].0, kind, false, None).clicked()
                };
                let again = button(0, ButtonKind::Default);
                let locate = button(1, ButtonKind::Normal);
                let folder = button(2, ButtonKind::Normal);
                let cancel = button(3, ButtonKind::Normal);
                if cancel || winui::escape_pressed(ui) {
                    Outcome::Closed
                } else if again || winui::enter_pressed(ui) {
                    Outcome::Done(Choice::Redownload)
                } else if locate {
                    Outcome::Done(Choice::Locate)
                } else if folder {
                    Outcome::Done(Choice::OpenFolder)
                } else {
                    Outcome::Open
                }
            },
        )
    }
}

/// The key of download `id`'s Locate... file chooser ([`winui::start_pick`]).
fn locate_key(id: u64) -> String {
    format!("locate-{id}")
}

/// Locate...: a file chooser for download `id`, whose file was `path`, opened in the nearest
/// folder of it that is still there (looked for on the chooser's own thread, #743), listing files
/// of its type first.
pub fn start_locate(ctx: &egui::Context, id: u64, path: &Path) {
    let path = path.to_owned();
    // i18n: title of the file chooser of File missing → Locate...: the person picks where the downloaded file is now
    let title = crate::i18n::t("Where is the file now?").to_owned();
    let all = crate::i18n::t("All files").to_owned();
    winui::start_pick(ctx, &locate_key(id), move || {
        let mut dialog = rfd::FileDialog::new().set_title(title);
        if let Some(dir) = path.ancestors().skip(1).find(|d| d.is_dir()) {
            dialog = dialog.set_directory(dir);
        }
        if let Some(name) = path.file_name() {
            dialog = dialog.set_file_name(name.to_string_lossy());
        }
        if let Some(ext) = path.extension().map(|e| e.to_string_lossy().into_owned()) {
            dialog = dialog.add_filter(ext.to_uppercase(), &[ext.as_str()]);
        }
        dialog.add_filter(all, &["*"]).pick_file()
    });
}

/// The file chosen in download `id`'s Locate... chooser, once.
pub fn located(ctx: &egui::Context, id: u64) -> Option<PathBuf> {
    winui::picked(ctx, &locate_key(id))
}

/// Download `id`'s Locate... chooser is still open (no answer taken yet).
pub fn locating(ctx: &egui::Context, id: u64) -> bool {
    winui::picking(ctx, &locate_key(id))
}

/// The finished download among `items` whose file `gone` names (the row Open or Open folder
/// was asked of); None for a path that is no download's file.
pub fn row_of(items: &[crate::model::Item], gone: &Gone) -> Option<u64> {
    items
        .iter()
        .find(|i| i.status == crate::model::Status::Complete && i.dest == gone.path)
        .map(|i| i.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_are_taken_once_in_order() {
        let _ = take();
        report(Path::new("a.zip"), false);
        report(Path::new("b.zip"), true);
        let got = take();
        assert_eq!(
            got,
            vec![
                Gone {
                    path: "a.zip".into(),
                    folder: false
                },
                Gone {
                    path: "b.zip".into(),
                    folder: true
                }
            ]
        );
        assert!(take().is_empty());
    }

    /// Only a finished download's own file is asked about; an unfinished one at that place (its
    /// file isn't there yet, never "missing"), or a path that is nobody's (a torrent's file, a
    /// crash report), gets what it always got.
    #[test]
    fn a_gone_file_is_a_finished_downloads_own() {
        use crate::model::{Category, Item, Status};
        let url = url::Url::parse("http://example.test/a.zip").unwrap();
        let mut done = Item::new(1, url.clone(), "C:/d/a.zip".into(), Category::General);
        done.status = Status::Complete;
        let mut running = Item::new(2, url, "C:/d/b.zip".into(), Category::General);
        running.status = Status::Stopped;
        let items = [done, running];
        let gone = |p: &str| Gone {
            path: p.into(),
            folder: false,
        };
        assert_eq!(row_of(&items, &gone("C:/d/a.zip")), Some(1));
        assert_eq!(row_of(&items, &gone("C:/d/b.zip")), None);
        assert_eq!(row_of(&items, &gone("C:/d/torrent/file.mkv")), None);
    }
}
