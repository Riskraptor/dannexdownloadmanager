//! Dannex Download Manager's browser native messaging host (Chrome, Edge, Brave, Firefox).
//!
//! - Started by the browser: reads native messages (u32 LE length + JSON) from stdin, hands each
//!   download to the running Dannex Download Manager over mdm-ipc (starting Dannex Download Manager if needed), answers
//!   `{"ok": …}`. A download Options says to leave to the browser (file type, excluded site or
//!   address, browser, integration off: `mdm_ipc::capture`) is answered `{"ok": false}` without
//!   starting anything, and the extension lets the browser have it. `{"rules": true}` asks for
//!   the file types to offer: `{"ok": true, "types": "EXE ZIP …"}`, with the keys a click is
//!   held with (`keys`: `{prevent, force}`, Options → General → Keys...) and the right-click
//!   items to show (`menu`).
//! - The download button over videos: `{"panel": true, "url": page}` asks whether it may show on
//!   that page (`{"ok": …}`, Options' rules, with `panel`: `{video, audio, minSeconds, corner}`,
//!   General → Download panels → Edit...); `{"video": true, "url": page}` opens the Download
//!   video window for it (`src`: the video's own file, or its stream's manifest; with
//!   `explicit`, the toolbar button's list of the tab's media asked, wherever the button shows);
//!   `{"hideSite": host}` keeps it off that site from now on;
//!   `{"openOptions": true}` opens Options where it is set up.
//! - The button's quality menu (announced by `"videoLookup": true` in the answer to `hello` and
//!   `rules`): `{"videoLookup": {page, src, title, referrer, storeId, prefetch}}` has the running
//!   app look the video up (`prefetch`: ahead of a click), `{"videoPoll": {token}}` asks how it
//!   goes, `{"videoPick": {token, choice, storeId}}` downloads the choice made, `{"videoCancel":
//!   {token}}` says a look-up asked ahead is no longer wanted. Each is answered `{"ok": true,
//!   "lookup": {state, …}}`
//!   (`mdm_ipc::api::VideoLookupAnswer`), never with a `why`; none of them starts the app
//!   (`use-window`: the button's click then opens the Download video window as before). A
//!   question waits here while the look-up runs, up to 1.5 s ([`POLL_WAIT`]).
//! - The extension's own window of the app's downloads is gone (extensions up to 0.6.5 asked for
//!   it with `{"downloads": true}` and `{"downloadsAct": …}`): such a message is refused as one
//!   the host doesn't know, and nothing is asked of the app.
//! - `dannexdownloadmanager-nmhost --register` / `--unregister`: writes / removes the host
//!   manifests (Chromium's names the extension's origin, Firefox's its add-on ID). Windows: beside
//!   this exe, with the per-user (HKCU) registry entries the browsers look up; with `--all-users`
//!   (the installer, installing for everyone, elevated) under HKLM instead. Linux and macOS: in
//!   each installed browser's own host folder (the app does it at every start, as there is no
//!   installer step), or the system-wide ones with `--all-users`. A browser that reads a
//!   system-wide manifest naming this same program (the .deb's or .rpm's) gets none in the home
//!   folder: the package manager removes the package's with it (#266).
//! - `dannexdownloadmanager-nmhost --safari` (macOS): the same host for Safari, started by the
//!   app's Safari extension (tools/safari), which passes each message on; nothing to register.
//! - `dannexdownloadmanager-nmhost --package-manifests <root>` (Linux, the .deb and .rpm build):
//!   writes the system-wide manifests under `<root>`, naming the packaged helper
//!   (`/usr/bin/dannexdownloadmanager-nmhost`). Nothing else.

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use mdm_ipc::extension::{FIREFOX_ID, STORE_ID, chromium_ids, copy_of, listed_store_id};
use serde::{Deserialize, Serialize};

#[cfg(windows)]
mod win32;

// The host's parts: the messages, who called, the answers, the video button's menu, the hand-off
// to the app, and the browsers' registration. Each takes what they share (and these imports)
// with `use super::*`.
mod answer;
mod caller;
mod deliver;
mod menu;
mod register;
mod wire;

use self::answer::*;
use self::caller::*;
use self::deliver::*;
use self::menu::*;
use self::register::*;
use self::wire::*;

const HOST_NAME: &str = "com.dannexdownloadmanager.host";

fn main() {
    started();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let all_users = args.iter().any(|a| a == "--all-users");
    let result = match args.first().map(String::as_str).unwrap_or_default() {
        "--register" => register(all_users),
        "--unregister" => unregister(all_users),
        "--package-manifests" => package_manifests(args.get(1).map(String::as_str)),
        // Safari's extension has no host manifest: its app extension (tools/safari) starts this
        // program from inside the app, which is the only caller there is.
        "--safari" => {
            SAFARI.store(true, std::sync::atomic::Ordering::Relaxed);
            serve("safari").map_err(|e| e.to_string())
        }
        // Browsers start the host naming the caller: serve only our own extension (the
        // manifests' allowed_origins / allowed_extensions say the same; this is a second check).
        _ if our_caller(&args) => serve(caller_copy(&args)).map_err(|e| e.to_string()),
        _ => Err(format!(
            "refusing caller {args:?}: not the Dannex Download Manager extension"
        )),
    };
    if let Err(e) = result {
        eprintln!("dannexdownloadmanager-nmhost: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests;
