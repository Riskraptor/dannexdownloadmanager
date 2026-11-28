//! Opening what downloads made: a file, its folder, Open with..., and what is told when a file is
//! gone or can't be opened (`tell`, shown by the main window).

use super::*;

/// Something that failed, to tell the user (the app shows it in its notice): from any window,
/// without a handle on the app.
pub(super) static TOLD: std::sync::Mutex<Vec<(&'static str, String)>> =
    std::sync::Mutex::new(Vec::new());

/// Tell the user `text` under the window title `title` (English, marked; the text translated).
pub fn tell(title: &'static str, text: String) {
    TOLD.lock().unwrap().push((title, text));
}

/// What [`tell`] was given since the last call (the app shows it).
pub fn told() -> Vec<(&'static str, String)> {
    std::mem::take(&mut *TOLD.lock().unwrap())
}

/// Open a downloaded file with its program, or say why not (moved or deleted since, no
/// program for it): on a thread of its own, as the file may be on a drive slow to answer
/// (#743). Returns at once.
pub fn open_file(path: &Path) {
    let path = path.to_owned();
    crate::launch::spawn("open file", move || {
        open_file_now(&path);
    });
}

/// [`open_file`], waited for: true when the file was handed to the system. For a thread other
/// than the window's (Download complete asks it through `launch::Asked`: whether it failed
/// decides whether the box stays open).
pub fn open_file_now(path: &Path) -> bool {
    // Never handed to the system when it's gone (Windows only says it can't find it): the app
    // asks what to do (Redownload, Locate..., crate::missing), or says so for a file that is no
    // download's ([`open_gone`]).
    if !path.exists() {
        crate::missing::report(path, false);
        return false;
    }
    let failed = if let Err(e) = open::that_detached(path) {
        not_opened(path, &e.to_string())
    } else {
        return true;
    };
    // i18n: title of the message when a downloaded file can't be opened
    tell(crate::i18n::mark("Open"), failed);
    false
}

/// A downloaded file's name, as the messages about it write it.
pub(super) fn file_label(path: &Path) -> String {
    crate::i18n::isolate(&path.file_name().unwrap_or_default().to_string_lossy())
}

/// Open of a file that is gone and is no download's own (a torrent's file, a crash report): said,
/// as before (a download's is asked about instead, `crate::missing`).
pub fn open_gone(path: &Path) {
    // i18n: title of the message when a downloaded file can't be opened
    tell(crate::i18n::mark("Open"), file_gone(path));
}

/// Open folder of a file that is gone and is no download's own: its folder, when that is there
/// (beside the window, #743).
pub fn open_folder_of_gone(path: &Path) {
    let path = path.to_owned();
    crate::launch::spawn("show in folder", move || {
        if let Some(dir) = path.parent().filter(|d| d.is_dir()) {
            crate::launch::open(dir);
        }
    });
}

/// Open's and Open with's message for a file moved or deleted since it was saved.
pub(super) fn file_gone(path: &Path) -> String {
    crate::i18n::fmt(
        crate::i18n::t(
            "{file} is no longer where it was saved (moved or deleted). Redownload, in its right-click menu, gets it again.",
        ),
        &[("file", &file_label(path))],
    )
}

/// Open's and Open with's message when the system couldn't open the file (`error`, its words).
pub(super) fn not_opened(path: &Path, error: &str) -> String {
    crate::i18n::fmt(
        crate::i18n::t("{file} could not be opened: {error}"),
        &[("file", &file_label(path)), ("error", error)],
    )
}

/// Show the file selected in the file manager (its folder, when the file is gone). Returns at
/// once: whether the file and its folder are there is looked at on a thread of its own, as a
/// sleeping hard disk, a USB drive or a network folder can take seconds to answer (#751: 8 s,
/// on the window's thread, froze it).
pub fn show_in_folder(path: &Path) {
    let path = path.to_owned();
    crate::launch::spawn("show in folder", move || show_in_folder_now(&path));
}

/// [`show_in_folder`], on the thread it runs on. A file that is gone is never handed to the
/// system: the app asks about a download's (`crate::missing`), and opens the folder of any other
/// ([`open_folder_of_gone`]).
pub(super) fn show_in_folder_now(path: &Path) {
    if !path.exists() {
        crate::missing::report(path, true);
        return;
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Another file manager set up to open folders (#446): Explorer's `/select` would start
        // Explorer anyway. The folder goes to the system, which runs that one; it can't be told
        // which file to select in a way every file manager reads.
        if let Some(opener) = crate::win32::folder_opener() {
            tracing::info!("show in folder: folders open with {opener}");
            if let Some(dir) = path.parent().filter(|d| d.is_dir()) {
                crate::launch::open(dir);
                return;
            }
        }
        // Explorer reads `/select,"C:\a b\c.mp4"` — quoted after the comma. Rust's own quoting
        // would wrap the whole argument ("/select,C:\a b\c.mp4"), which Explorer doesn't
        // understand: it opened its default folder for any path with a space (user report
        // 2026-10-29, a user folder with a space in its name). Windows paths can't contain a
        // quote.
        let _ = std::process::Command::new("explorer")
            .raw_arg(format!("/select,\"{}\"", path.display()))
            .spawn();
    }
    #[cfg(unix)]
    crate::unix::show_in_folder(path);
}

/// Open with...: the system's own chooser of apps for a downloaded file (Windows' "Open with",
/// the desktop portal's on Linux, the list of applications on macOS), which opens it with the app
/// picked; or say why not (moved or deleted since, no chooser). Returns at once: the file is
/// looked for, and the chooser waits for the user, on a thread of its own (#743).
pub fn open_with(path: &Path) {
    let path = path.to_owned();
    crate::launch::spawn("open with", move || {
        if !path.exists() {
            tell(crate::i18n::mark("Open"), file_gone(&path));
            return;
        }
        // An off-screen copy shows nothing (crate::offscreen): its checks read this line.
        if crate::offscreen() {
            tracing::info!("open with: the chooser asked for (off-screen: not shown)");
            return;
        }
        #[cfg(windows)]
        let shown = crate::win32::open_with(&path);
        #[cfg(unix)]
        let shown = crate::unix::open_with(&path);
        if let Err(e) = shown {
            tracing::warn!("open with: {e}");
            tell(crate::i18n::mark("Open"), not_opened(&path, &e));
        }
    });
}
