//! What comes in from outside: the command line, a second launch, the browser's native host
//! (`handle_command`), the automation API's questions and actions (`answer`, `act`) and the
//! downloads a paired device sends (`remote_add`).

use super::*;

impl App {
    /// A command from the command line, a second launch, or the browser (ipc.rs).
    pub(super) fn handle_command(&mut self, cmd: ipc::Command, ctx: &egui::Context) {
        if cmd.exit {
            self.run(Cmd::Exit, None, ctx);
            return;
        }
        // Without a window (#157): an address, magnet or .torrent is added as a paired device's
        // is (no Download File Info, no Add torrent); nothing else opens anything.
        if self.headless {
            if cmd.start_queue {
                self.start_queue(0);
            }
            if cmd.url.is_some() {
                let answer = self.remote_add(cmd);
                tracing::info!("headless: added: {answer}");
            }
            return;
        }
        // The startup entry's (#226): this start began in the tray (main.rs), or DDM already
        // ran; either way the window stays where it is.
        if cmd.startup {
            return;
        }
        if let Some(name) = cmd.show.as_deref() {
            self.show_window(ctx);
            self.show_named(name, ctx);
            return;
        }
        // The download button over a Google Drive file's player: the file itself, as it was
        // uploaded (segmented and resumable), not the player's re-encoded streams (#34).
        let mut cmd = cmd;
        if let Some(file) = drive_file_behind_player(&cmd) {
            cmd.video_page = None;
            cmd.url = Some(file);
        }
        // What arrives from outside (the browser, the clipboard, a drop, the command line) opens
        // its own window only: the main window stays where it is (in the notification area,
        // minimized, behind the program in front), as IDM's does. Dialogs are windows of their
        // own, lifted above the other programs' (dialogs::keep_theme), and not owned by a main
        // window that isn't in front (win32::dialog_owned_by_main), which would come up with them.
        //
        // The browser's download button over a video: the Download video window, looking the
        // page up at once.
        if let Some(page) = cmd.video_page.clone() {
            self.open_video_page(page, cmd);
            return;
        }
        // Its menu: never on this site.
        if let Some(host) = cmd.panel_hide_site.as_deref() {
            self.settings.video_panel_sites =
                mdm_ipc::capture::add_site(&self.settings.video_panel_sites, host);
            self.dirty = true;
            return;
        }
        // The browser's right-click menu: a page's (or a selection's) links, to tick in Import.
        if !cmd.links.is_empty() {
            // Each with the text the page shows for it, when the extension sent them.
            let texts = cmd
                .link_texts
                .iter()
                .map(String::as_str)
                .chain(std::iter::repeat(""));
            let found = cmd
                .links
                .iter()
                .zip(texts)
                .map(|(url, text)| crate::transfer::Imported {
                    url: url.clone(),
                    referer: cmd.referer.clone(),
                    cookies: page_data_for(url, cmd.referer.as_deref(), cmd.cookies.as_deref()),
                    user_agent: cmd.user_agent.clone(),
                    private: cmd.private,
                    text: text.trim().to_owned(),
                })
                .collect();
            // i18n: in Import, where the addresses came from: a page in the browser
            let source = crate::i18n::t("Links from the browser").to_owned();
            self.show_import(found, source);
            return;
        }
        // The browser's toolbar button: this site's downloads stay with the browser.
        if let Some(host) = cmd.exclude_site.as_deref() {
            self.settings.excluded_sites =
                mdm_ipc::capture::add_site(&self.settings.excluded_sites, host);
            self.dirty = true;
            return;
        }
        if cmd.start_queue {
            self.start_queue(0);
        }
        self.command_goes_on(cmd, None, ctx);
    }

    /// [`Self::handle_command`] from its look for a magnet link or a .torrent file on: `file`,
    /// whether its `.torrent` path is a file, as looked at beside the window (#743: a path on a
    /// sleeping hard disk, a USB drive or a network share can take seconds to answer); None
    /// while not looked at yet: the command waits for the answer (`Beside::TorrentPath`).
    pub(super) fn command_goes_on(
        &mut self,
        cmd: ipc::Command,
        file: Option<bool>,
        ctx: &egui::Context,
    ) {
        // A Metalink file on this computer (command line, file association, a drop): read
        // beside the window, then its file in Download File Info or its files in Add Metalink.
        let url = cmd.url.clone().unwrap_or_default();
        if let Some(path) = crate::metalinkdlg::metalink_path(&url) {
            self.read_metalink_file(path);
            return;
        }
        // A magnet link or a .torrent file (browser, command line, file association): Add
        // torrent shows what it holds.
        let torrent = mdm_torrent::Source::magnet(&url).is_some()
            || match (torrent_path(&url), file) {
                (None, _) => false,
                (Some(path), None) => {
                    self.ask_beside(move || Beside::TorrentPath {
                        file: path.is_file(),
                        cmd: Box::new(cmd),
                    });
                    return;
                }
                (Some(_), Some(file)) => file,
            };
        if torrent {
            self.open_torrent_dialog(Some(url.trim().to_owned()), ctx);
            return;
        }
        let url = cmd
            .url
            .as_deref()
            .and_then(|u| url::Url::parse(u.trim()).ok())
            .filter(mdm_engine::supported);
        let Some(url) = url else {
            if !cmd.start_queue {
                // A plain second launch brings the window to the front, as IDM does.
                self.show_window(ctx);
            }
            return;
        };
        // A .torrent on the web (#39; the browser's download of one): Add torrent fetches it with
        // the browser's request (its cookies) and shows what it holds. Silent (`/n`, a script's
        // command line) asks nothing: the .torrent itself is downloaded, as any file.
        let final_url = cmd
            .final_url
            .as_deref()
            .and_then(|u| url::Url::parse(u).ok());
        let torrent = |u: &url::Url| {
            crate::torrents::is_torrent_url(u, cmd.name.as_deref(), cmd.mime.as_deref())
        };
        if !cmd.silent && (torrent(&url) || final_url.as_ref().is_some_and(torrent)) {
            let parts = self.request_parts(&url, None, &cmd, crate::debrid::browser_started(&cmd));
            self.open_torrent_dialog_with(
                Some(url.to_string()),
                Some(crate::torrents::Remote { url, parts }),
                ctx,
            );
            return;
        }
        if !self.duplicate_gate(&url, || Pending::Command(Box::new(cmd.clone()))) {
            return;
        }
        // Options → Downloads → "Show start download dialog" off: start (or only queue) at once.
        // A Metalink's address still goes to Download File Info, which reads it (its file, or
        // its files for Add Metalink), as a .torrent's opens Add torrent; silent (`/n`) asks
        // nothing: the .meta4 itself is downloaded, as any file.
        let metalink = !cmd.silent
            && crate::metalinkdlg::is_metalink_url(&url, cmd.name.as_deref(), cmd.mime.as_deref());
        let skip_dialog = !self.settings.show_start_dialog && !metalink;
        if (cmd.later && !metalink) || cmd.silent || skip_dialog {
            let guessed = cmd.name.is_none();
            let name = cmd
                .name
                .clone()
                .unwrap_or_else(|| mdm_engine::naming::file_name(None, &url));
            let name = mdm_engine::naming::sanitize(&name);
            // The automation API's `category` (by its key) and `queue` (by name), when given.
            let category = cmd
                .category
                .as_deref()
                .and_then(|k| Category::all().into_iter().find(|c| c.key() == k))
                .unwrap_or_else(|| Category::of(&name));
            let queue = cmd
                .queue
                .as_deref()
                .and_then(|n| self.queues.iter().position(|q| q.name == n))
                .unwrap_or(0);
            let dir = cmd
                .dir
                .clone()
                .map(PathBuf::from)
                .unwrap_or_else(|| self.settings.category_dir(category, &self.downloads));
            // Its folder isn't looked at here (#743): `number_beside`.
            let dest = mdm_engine::naming::guess(&dir, &name);
            let later = cmd.later || (skip_dialog && self.settings.only_add_to_queue);
            let source = if cmd.referer.is_some() || cmd.cookies.is_some() {
                crate::i18n::mark("from the browser")
            } else {
                crate::i18n::mark("from the command line")
            };
            let size = cmd.size;
            let id = self.push_item(url, dest, category, String::new(), size, None, cmd, source);
            self.name_guessed(id, guessed);
            self.number_beside(&[id]);
            if later {
                self.queues[queue].items.push(id);
            } else {
                self.start(id, true);
            }
        } else {
            self.open_file_info(dialogs::NewUrl { url, login: None }, cmd, ctx);
        }
    }

    /// Download with the browser (#376): a download the site's bot check refused opens in the
    /// browser it came from (or the system's), and the browser's download that follows stays
    /// the browser's (crate::handback). The row stays as it is, held as one stopped by hand: a
    /// queue's retries leave it (one getting through would download the file a second time),
    /// until it is started again.
    pub(super) fn download_with_browser(&mut self, id: u64) {
        let Some(item) = self.item(id) else { return };
        if !crate::handback::offered(item) {
            return;
        }
        let ask = crate::handback::Ask::of(item);
        self.stopped_by_hand.insert(id);
        let Some(item) = self.item(id) else { return };
        // i18n: event log entry, past tense: "Download with the browser" opened the download's address in the web browser, which downloads the file itself (the site lets only browsers through)
        item.note(crate::i18n::t(
            "Opened in the browser, which downloads it itself",
        ));
        self.dirty = true;
        crate::handback::open(ask);
    }

    /// The automation API's answer (a JSON line, shaped by `ipc::api`): `api` turns it on or
    /// off; `query: "list"` gives every download and the total speed, `action` acts on
    /// downloads ([`App::act`]) — only while it is on.
    ///
    /// A paired device's command (the remote control, #141) is answered while Options → Remote
    /// is on, whatever the automation API's switch: it may list, add and act, nothing else.
    pub(super) fn answer(&mut self, cmd: &ipc::Command) -> String {
        use ipc::api::{ApiAnswer, DownloadInfo, ErrorAnswer, ListAnswer, line};
        // The browser's quality menu (its native host asks): whatever the automation API's
        // switch, and never for a paired device.
        if !cmd.remote
            && (cmd.video_lookup.is_some()
                || cmd.video_poll.is_some()
                || cmd.video_pick.is_some()
                || cmd.video_cancel.is_some())
        {
            return line(&self.quality_menu(cmd));
        }
        if cmd.pair && !cmd.remote {
            return self.pair_answer();
        }
        if cmd.remote {
            if !self.settings.remote_on {
                return line(&ErrorAnswer {
                    error: "the remote control is off".into(),
                    code: Some("remote-off".into()),
                });
            }
            if cmd.url.is_some() || cmd.torrent_file.is_some() {
                return self.remote_add(cmd.clone());
            }
        } else if let Some(on) = cmd.api {
            self.settings.automation_api = on;
            self.dirty = true;
            return line(&ApiAnswer { api: on });
        } else if !self.settings.automation_api {
            return line(&ErrorAnswer {
                error: "the automation API is off (dannexdownloadmanager-cli api on)".into(),
                code: Some(ipc::api::API_OFF.into()),
            });
        }
        // Open folder shows a window on this computer: never for a paired device.
        if cmd.remote && cmd.action.as_deref() == Some("folder") {
            return line(&ErrorAnswer {
                error: "unknown action \"folder\"".into(),
                code: None,
            });
        }
        if let Some(action @ ("start_queue" | "stop_queue")) = cmd.action.as_deref() {
            let at = cmd
                .queue
                .as_deref()
                .and_then(|n| self.queues.iter().position(|q| q.name == n));
            let Some(q) = at else {
                return line(&ErrorAnswer {
                    error: "no such queue".into(),
                    code: Some("unknown-queue".into()),
                });
            };
            if action == "start_queue" {
                self.start_queue(q);
            } else {
                self.stop_queue(q);
            }
            self.dirty = true;
            return line(&ipc::api::ActAnswer::default());
        }
        if let Some(action) = cmd.action.as_deref() {
            return match self.act(action, &cmd.ids) {
                Ok(done) => line(&done),
                Err(e) => line(&e),
            };
        }
        match cmd.query.as_deref() {
            Some("list") => {
                // The list's downloads (not one waiting for Download File Info's answer).
                let downloads = self
                    .items
                    .iter()
                    .filter(|i| i.listed())
                    .map(|i| DownloadInfo {
                        id: i.id,
                        file: i.file_name.clone(),
                        // A paired device sees no folder of the computer's, nor a password in
                        // an address.
                        url: if cmd.remote {
                            let mut u = i.url.clone();
                            let _ = u.set_password(None);
                            u.to_string()
                        } else {
                            i.url.to_string()
                        },
                        path: if cmd.remote {
                            String::new()
                        } else {
                            i.dest.to_string_lossy().into_owned()
                        },
                        status: format!("{:?}", i.status),
                        size: i.size,
                        downloaded: i.downloaded,
                        speed: if i.live.is_some() { i.rate() as u64 } else { 0 },
                        added: i.added.clone(),
                        category: i.category.key().into_owned(),
                        error: i.error.clone().filter(|_| i.status == Status::Error),
                    })
                    .collect();
                let speed: f64 = self
                    .items
                    .iter()
                    .filter(|i| i.live.is_some() && i.listed())
                    .map(|i| i.rate())
                    .sum();
                let queues = self
                    .queues
                    .iter()
                    .map(|q| ipc::api::QueueInfo {
                        name: q.name.clone(),
                        running: q.running,
                        ids: q.items.clone(),
                    })
                    .collect();
                let categories = Category::all()
                    .into_iter()
                    .map(|c| ipc::api::CategoryInfo {
                        key: c.key().into_owned(),
                        name: c.label().into_owned(),
                    })
                    .collect();
                line(&ListAnswer {
                    downloads,
                    speed: speed as u64,
                    queues,
                    categories,
                    free: self.free_space(),
                })
            }
            other => line(&ErrorAnswer {
                error: format!("unknown query {other:?}"),
                code: None,
            }),
        }
    }

    /// `{"pair": true}` (`dannexdownloadmanager-cli pair`, #157): a new pairing code, the
    /// page's addresses and a QR code for a terminal; an error while the remote control is off
    /// or not listening.
    pub fn pair_answer(&mut self) -> String {
        use ipc::api::{ErrorAnswer, PairAnswer, line};
        let mut urls = self.remote.as_ref().map(|r| r.urls()).unwrap_or_default();
        // A container knows only its own address: the one people open, when given.
        if let Some(public) = crate::remote::public_url()
            && !urls.is_empty()
        {
            urls.insert(0, public);
        }
        let (Some(remote), Some(first)) = (&self.remote, urls.first()) else {
            return line(&ErrorAnswer {
                error: "the remote control isn't listening (Options → Remote)".into(),
                code: Some("remote-off".into()),
            });
        };
        let code = remote.start_pairing();
        line(&PairAnswer {
            qr: crate::remote::qr_text(&format!("{first}#pair={code}")),
            code,
            minutes: (mdm_remote::pairing::VALID.as_secs() / 60) as u32,
            urls,
        })
    }

    /// Free space where downloads go (General's folder), looked up off this thread at most every
    /// 10 s (a network share can be slow to answer).
    pub(super) fn free_space(&mut self) -> Option<u64> {
        let (checked, free) = *self.free_space.lock().unwrap();
        if checked.is_none_or(|t| t.elapsed() >= Duration::from_secs(10)) {
            self.free_space.lock().unwrap().0 = Some(Instant::now());
            let dir = self
                .settings
                .category_dir(Category::General, &self.downloads);
            let slot = self.free_space.clone();
            self.rt.spawn_blocking(move || {
                // The folder may not exist yet: its nearest existing parent's volume.
                let at = dir
                    .ancestors()
                    .find(|d| d.is_dir())
                    .map(std::path::Path::to_owned);
                let free = at
                    .and_then(|d| mdm_engine::disk::volume_of(&d))
                    .map(|v| v.free);
                slot.lock().unwrap().1 = free;
            });
        }
        free
    }

    /// A download a paired device sent (#141): an address added as a silent command line's is,
    /// or a magnet link or .torrent file as Add torrent's Download Now with every file, in the
    /// category and queue it chose (by file type, and started now, when it chose none). No
    /// window opens. Answered with its id, or `pending` while a torrent's files are looked up.
    pub(super) fn remote_add(&mut self, mut cmd: ipc::Command) -> String {
        use ipc::api::{AddAnswer, ErrorAnswer, line};
        let fail = |code: &str, error: &str| {
            line(&ErrorAnswer {
                error: error.into(),
                code: Some(code.into()),
            })
        };
        let category = match cmd.category.as_deref() {
            None => None,
            Some(key) => match Category::all().into_iter().find(|c| c.key() == key) {
                Some(c) => Some(c),
                None => return fail("unknown-category", "no such category"),
            },
        };
        let queue = match cmd.queue.as_deref() {
            None => None,
            Some(name) => match self.queues.iter().position(|q| q.name == name) {
                Some(q) => Some(q),
                None => return fail("unknown-queue", "no such queue"),
            },
        };
        // Later: in the queue chosen, else the main queue; else started now.
        let queue = cmd.later.then(|| queue.unwrap_or(0));
        let text = cmd.url.clone().unwrap_or_default();
        let torrent = match cmd.torrent_file.take() {
            Some(bytes) => Some((mdm_torrent::Source::File(bytes), false)),
            None => mdm_torrent::Source::magnet(text.trim()).map(|m| (m, true)),
        };
        if let Some((source, from_magnet)) = torrent {
            let slot: crate::remote::Slot = Default::default();
            let (out, host, known) = (
                slot.clone(),
                self.torrent_host.clone(),
                self.session_hashes(),
            );
            self.rt.spawn(async move {
                let res = match host.session(known).await {
                    Ok(t) => t
                        .metadata(&source, crate::torrentdlg::METADATA_WAIT, None)
                        .await
                        .map_err(|e| crate::errors::torrent(&e)),
                    Err(e) => Err(e),
                };
                *out.lock().unwrap() = Some(res);
            });
            self.remote_torrents.push(crate::remote::Lookup {
                slot,
                category,
                queue,
                from_magnet,
            });
            return line(&AddAnswer {
                id: None,
                pending: true,
            });
        }
        let Some(url) = url::Url::parse(text.trim())
            .ok()
            .filter(mdm_engine::supported)
        else {
            return fail("unsupported", "not an address DDM downloads");
        };
        // Already in the list: no one at the computer to ask, so Options' choice, with "ask"
        // taken as "add it again" (under a numbered name). Overwrite: the one there leaves the
        // list now, its files go beside the window (#743), and the new one's folder is looked at,
        // and it starts, once they are gone (`number_beside_after`).
        let mut removal = None;
        if let Some(dup) = self
            .items
            .iter()
            .find(|i| i.url == url && i.listed())
            .map(|i| i.id)
        {
            match self.settings.duplicate_action {
                2 => {
                    removal = replacement(&self.items, dup, true);
                    if removal.is_some() {
                        self.delete(dup);
                    }
                }
                3 => {
                    return line(&AddAnswer {
                        id: Some(dup),
                        pending: false,
                    });
                }
                _ => {}
            }
        }
        let name = mdm_engine::naming::sanitize(&mdm_engine::naming::file_name(None, &url));
        let category = category.unwrap_or_else(|| Category::of(&name));
        let dir = self.settings.category_dir(category, &self.downloads);
        // Its folder isn't looked at here (#743): `number_beside`.
        let dest = mdm_engine::naming::guess(&dir, &name);
        cmd.remote = false;
        let id = self.push_item(
            url,
            dest,
            category,
            String::new(),
            None,
            None,
            cmd,
            // i18n: where a download came from (its Properties): a phone or computer paired in Options → Remote
            crate::i18n::mark("from a paired device"),
        );
        self.name_guessed(id, true);
        self.number_beside_after(&[id], removal);
        match queue {
            Some(q) => self.queues[q].items.push(id),
            None => self.start(id, false),
        }
        self.dirty = true;
        line(&AddAnswer {
            id: Some(id),
            pending: false,
        })
    }

    /// The automation API's actions on downloads by id: pause one running, start one stopped,
    /// queued or failed (or a finished gallery), remove one (its file kept) or delete it with its file; one that runs
    /// is paused first by the caller. `folder` shows one in the file manager, as Open folder.
    pub(super) fn act(
        &mut self,
        action: &str,
        ids: &[u64],
    ) -> Result<ipc::api::ActAnswer, ipc::api::ErrorAnswer> {
        if !matches!(action, "pause" | "start" | "remove" | "delete" | "folder") {
            return Err(ipc::api::ErrorAnswer {
                error: format!("unknown action {action:?}"),
                code: None,
            });
        }
        let mut answer = ipc::api::ActAnswer::default();
        // Their files removed beside the window once all are off the list (#743).
        let mut deleting = Vec::new();
        for &id in ids {
            let Some(item) = self.item(id) else {
                answer.skipped.push(ipc::api::Skipped {
                    id,
                    why: "no such download".into(),
                });
                continue;
            };
            let running = item.live.is_some();
            // As Resume: a finished gallery may start again (#58).
            let startable = running || item.can_start();
            // As Open folder (a finished download's row).
            if action == "folder" {
                dialogs::show_in_folder(&item.dest);
                answer.done.push(id);
                continue;
            }
            let why = match action {
                "pause" if !running => Some("not running"),
                "pause" => {
                    self.stop_by_hand(id);
                    None
                }
                "start" if running => Some("already running"),
                "start" if !startable => Some("complete"),
                "start" => {
                    self.start(id, false);
                    None
                }
                _ if running => Some("running: pause it first"),
                _ => {
                    deleting.extend(self.remove_download(id, action == "delete"));
                    None
                }
            };
            match why {
                Some(why) => answer.skipped.push(ipc::api::Skipped {
                    id,
                    why: why.into(),
                }),
                None => answer.done.push(id),
            }
        }
        self.delete_beside(deleting);
        Ok(answer)
    }
}

/// A command's address as a .torrent file's path (a command line, a file association): one
/// ending in `.torrent`, its quotes taken off; whether it is a file is the disk's to say.
pub(super) fn torrent_path(u: &str) -> Option<PathBuf> {
    u.to_ascii_lowercase()
        .ends_with(".torrent")
        .then(|| PathBuf::from(u.trim_matches('"')))
}

/// The Google Drive file whose player the browser's video button was clicked over (#34): the
/// page itself when it is a Drive file's (`video_page`), or, when the button sent the player's
/// own stream (on Google's hosts), the page it came from (`referer`). None for any other video.
pub(super) fn drive_file_behind_player(cmd: &ipc::Command) -> Option<String> {
    use mdm_engine::sites::gdrive;
    let drive_file = |s: &str| {
        url::Url::parse(s)
            .ok()
            .and_then(|u| gdrive::parse(&u))
            .is_some_and(|l| l.kind == gdrive::Kind::File)
    };
    let page = cmd.video_page.as_deref()?;
    if drive_file(page) {
        return Some(page.to_owned());
    }
    let googles = url::Url::parse(page).ok().and_then(|u| {
        u.host_str().map(|h| {
            let h = h.to_ascii_lowercase();
            [
                ".google.com",
                ".googleapis.com",
                ".googlevideo.com",
                ".googleusercontent.com",
            ]
            .iter()
            .any(|d| h.ends_with(d))
        })
    });
    cmd.referer
        .as_deref()
        .filter(|r| googles == Some(true) && drive_file(r))
        .map(str::to_owned)
}

/// What the browser's quality menu leaves to the Download video window (the button's click, as
/// before it): everything in a run without a window, a Google Drive file's player (#34: the
/// file itself, as uploaded, which the click's command downloads), and an address not on the
/// web (the browser's host sends none).
pub(super) fn menu_needs_window(cmd: &ipc::Command, headless: bool) -> bool {
    headless
        || cmd.video_lookup.as_ref().is_some_and(|ask| {
            !(ask.address.starts_with("http://") || ask.address.starts_with("https://"))
                || drive_file_behind_player(&ipc::Command {
                    video_page: Some(ask.address.clone()),
                    referer: cmd.referer.clone(),
                    ..Default::default()
                })
                .is_some()
        })
}
