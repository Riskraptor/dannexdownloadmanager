//! Questions to the disk asked on a thread other than the window's (#743: a sleeping disk, a USB
//! drive or a network folder can take seconds to answer), and their answers acted on as they come.

use super::*;

/// What the disk answered beside the window (#743: a sleeping hard disk, a USB drive or a
/// network folder can take seconds to answer) for the window to act on (`App::ask_beside`,
/// `App::poll_beside`), each as it comes.
pub(super) enum Beside {
    /// A command's `.torrent` path (a command line, a file association, a hand-off): whether it
    /// is a file. One opens Add torrent; else the command goes on as an address.
    TorrentPath { cmd: Box<ipc::Command>, file: bool },
    /// A Metalink file read (`App::read_metalink_file`): where from, and what it holds or why it
    /// can't be used.
    Metalink {
        origin: String,
        read: Result<mdm_engine::metalink::Manifest, String>,
    },
    /// Links or files dropped, sorted.
    Dropped(Box<Dropped>),
    /// Tasks → Import's file read: its name and the addresses in it, or why it couldn't be read.
    Imported {
        file: String,
        read: Result<Vec<crate::transfer::Imported>, String>,
    },
    /// Tasks → Export written, or why not.
    Exported(Result<(), String>),
    /// View → Theme → Export this theme written, or why not.
    ThemeExported(Result<(), String>),
    /// View → Theme's files looked at: the themes folder's time, whether pywal's colours are
    /// there, and the list read again when either changed.
    Themes {
        changed: Option<std::time::SystemTime>,
        wal: bool,
        list: Option<Vec<crate::themefile::Entry>>,
    },
    /// View → Theme → Add a theme file: copied into the themes folder (its id), or why not, and
    /// the themes folder then.
    ThemeAdded {
        added: Result<String, String>,
        changed: Option<std::time::SystemTime>,
        list: Vec<crate::themefile::Entry>,
    },
    /// File Properties → another folder, or Move to folder for a selection: each download's
    /// free name there (`naming::destination`, `place_in`), to move it to.
    Moves(Vec<(u64, PathBuf)>),
    /// A gallery's start: gallery-dl as found in DDM's tools folder (None: not installed).
    GalleryTool {
        id: u64,
        show: bool,
        tool: Option<mdm_tools::Installed>,
    },
}

impl App {
    /// Ask `ask` of the disk on a thread other than the window's (#743), as
    /// `check_torrent_beside` does for a torrent's folder: the answer comes to `poll_beside`.
    pub(super) fn ask_beside(&mut self, ask: impl FnOnce() -> Beside + Send + 'static) {
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = self.ui_ctx.clone();
        self.rt.spawn_blocking(move || {
            let _ = tx.send(ask());
            ctx.request_repaint();
        });
        self.beside.push(rx);
    }

    /// The answers from beside the window (`ask_beside`), each acted on as it comes (those of
    /// one frame in the order asked): a look slow to answer holds up no other.
    pub(super) fn poll_beside(&mut self, ctx: &egui::Context) {
        if self.beside.is_empty() {
            return;
        }
        let mut done = Vec::new();
        self.beside.retain(|rx| match rx.try_recv() {
            Ok(answer) => {
                done.push(answer);
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            // The look ended without an answer (a panic): nothing to act on.
            Err(std::sync::mpsc::TryRecvError::Disconnected) => false,
        });
        if self.beside.is_empty() {
            // No look at the themes can be under way (one that ended without an answer too).
            self.themes_looking = false;
        }
        for answer in done {
            match answer {
                Beside::TorrentPath { cmd, file } => self.command_goes_on(*cmd, Some(file), ctx),
                Beside::Metalink { origin, read } => match read {
                    Ok(manifest) => {
                        let opened = crate::metalinkdlg::Opened { manifest, origin };
                        self.open_metalink(opened, ctx);
                    }
                    Err(e) => {
                        self.notice = Some((
                            // i18n: title of the message when a Metalink file can't be used
                            crate::i18n::mark("Metalink"),
                            e,
                        ));
                    }
                },
                Beside::Dropped(sorted) => self.dropped_sorted(*sorted, ctx),
                Beside::Imported { file, read } => self.show_list(file, read),
                Beside::Exported(Ok(())) | Beside::ThemeExported(Ok(())) => {}
                Beside::ThemeExported(Err(e)) => {
                    self.notice = Some((
                        crate::i18n::mark("Export this theme"),
                        crate::i18n::fmt(
                            // i18n: View → Theme → Export this theme failed; {error} says why
                            crate::i18n::t("The theme could not be saved: {error}"),
                            &[("error", &e)],
                        ),
                    ));
                }
                Beside::Exported(Err(e)) => {
                    self.notice = Some((
                        // i18n: title of a message box and of a file dialog: Tasks → Export, the list written to a file (the command's name, not a past tense such as "exported")
                        crate::i18n::mark("Export"),
                        crate::i18n::fmt(
                            crate::i18n::t("The list could not be saved: {error}"),
                            &[("error", &e)],
                        ),
                    ));
                }
                Beside::Themes { changed, wal, list } => {
                    self.themes_looking = false;
                    if let Some(list) = list {
                        self.themes_changed = changed;
                        self.pywal_there = wal;
                        self.theme_list = list;
                    }
                }
                Beside::ThemeAdded {
                    added,
                    changed,
                    list,
                } => self.theme_added(added, changed, list, ctx),
                Beside::Moves(moves) => {
                    for (id, target) in moves {
                        self.start_move(id, target);
                    }
                }
                Beside::GalleryTool { id, show, tool } => self.gallery_found(id, show, tool),
            }
        }
    }
}
