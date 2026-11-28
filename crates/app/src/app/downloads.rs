//! File downloads added, started and stopped: the headers and options their requests carry, the
//! early start while Download File Info shows, the look at the disk a start asks for (`StartLook`)
//! and the finished file's marks.

use super::*;

/// A file download's start as decided beside the window (`Download::begin`, #743): what the
/// window knew, copied, and the disk asked there; a sleeping hard disk, a USB drive or a network
/// folder can take seconds to answer, and a frame that waits for it freezes every window.
pub(super) struct StartLook {
    /// The row (`Item::place_copy`).
    pub(super) me: Item,
    /// Its name is the address's guess (`Item::name_from_url`): the server names it.
    pub(super) guessed: bool,
    /// Its name was given without a look at its folder (`Item::number_at_start`): nothing of
    /// its own is there, it starts under that name or the next free one; nothing to ask.
    pub(super) fresh: bool,
    /// The other rows in its folder (copies), and those of them that run.
    pub(super) others: Vec<Item>,
    pub(super) running: std::collections::HashSet<u64>,
    /// Its files being removed (Redownload), or files it would write among being removed
    /// (Delete with its files, a download started again under its name, Overwrite: `Removing`):
    /// they are gone first.
    pub(super) clearing: Vec<std::sync::mpsc::Receiver<()>>,
}

impl StartLook {
    /// Resume whenever there is something to resume (a failure then says why and keeps the
    /// data: starting over would overwrite it, and that is only ever the user's choice); else
    /// from scratch, into a free place of its own if its own is another row's (#442). On a
    /// thread other than the window's.
    pub(super) fn decide(self) -> mdm_engine::Begin {
        use mdm_engine::Begin;
        for gone in self.clearing {
            // Sent once they are gone (or the remover went away).
            let _ = gone.recv();
        }
        let me = self.me;
        let dest = if self.fresh {
            me.dest.clone()
        } else {
            let running = &self.running;
            place_to_take_by(&me, &self.others, |o| {
                running.contains(&o.id) || o.holds_place()
            })
            .unwrap_or_else(|| me.dest.clone())
        };
        // Its files under a former name (`.mdm.part`, Dannex up to 0.6.5) take today's first.
        if !self.fresh {
            mdm_engine::state::migrate_legacy(&dest);
        }
        let state = state_path(&dest);
        if !self.fresh && state.exists() {
            return Begin::Resume(state);
        }
        // The folder the engine is about to make, DDM's to remove again once empty.
        note_made(me.id, dest.parent());
        // A name nobody gave (the address's guess) is the server's to give: into the folder,
        // under its Content-Disposition name (#28); the row follows the file made. A name given
        // before its folder was looked at is numbered on there if it is taken (#743). A missing
        // folder is created by the engine; a removed drive is reported without touching the
        // progress shown for the data on it.
        Begin::Fresh(match dest.parent() {
            Some(dir) if self.guessed => Destination::Dir(dir.to_owned()),
            _ if self.fresh => Destination::Free(dest),
            _ => Destination::File(dest),
        })
    }

    /// A video's place as it starts: its own, or a stem of its own if another row there has
    /// its stem (#442, `place_to_take_by`; yt-dlp takes any `<stem>.*` as its own). On the run's
    /// own thread (`VideoJob::begin`, which makes its folder there, #743); the row follows
    /// (`Progress::place`).
    pub(super) fn video(self) -> PathBuf {
        for gone in self.clearing {
            let _ = gone.recv();
        }
        let running = &self.running;
        let id = self.me.id;
        let place = place_to_take_by(&self.me, &self.others, |o| {
            running.contains(&o.id) || o.holds_place()
        })
        .unwrap_or(self.me.dest);
        note_made(id, place.parent());
        place
    }
}

/// `Authorization: Basic …` for "Log in to the server".
/// What a download's requests carry: its headers (a login, the browser's Referer and Cookie, a
/// form's Content-Type) and the options its requests go with (User-Agent, proxy, a form's body).
/// Download File Info's look-up asks with the same as the download, so both get one answer.
#[derive(Clone)]
pub struct RequestParts {
    pub headers: HeaderMap,
    pub opts: Options,
}

impl App {
    // ---- actions ---------------------------------------------------------------------------

    pub(super) fn add(&mut self, fi: &mut FileInfo, start: Start) {
        // Only a folder other than the category's becomes its new one (an unchanged folder
        // stays "the default", following Downloads).
        if let Some((cat, dir)) = fi.remembered() {
            self.settings
                .keep_category_dir(cat, std::path::Path::new(&dir), &self.downloads);
        }
        let id = self.push_item(
            fi.url.clone(),
            fi.dest(),
            fi.category,
            fi.description.clone(),
            fi.size(),
            fi.login.clone(),
            fi.context.clone(),
            if fi.context.referer.is_some() || fi.context.cookies.is_some() {
                crate::i18n::mark("from the browser")
            } else {
                crate::i18n::mark("with Add URL")
            },
        );
        self.name_guessed(id, fi.name_guessed());
        self.set_direct(id, fi.direct);
        self.metalink_parts(id, fi);
        self.number_at_start(id);
        self.selection.set(id);
        if matches!(start, Start::Now) {
            // Later: the answer is closed (its download starts some other time).
            if let Some(k) = fi.take_kept() {
                self.kept_answers.insert(id, k);
            }
            self.start(id, true);
        } else {
            // "Download Later": the main download queue (IDM's default), or the one picked.
            self.queue_later(vec![id], self.settings.queue_panel_later);
        }
        self.dirty = true;
    }

    /// A Metalink's file (Download File Info's, `FileInfo::apply_metalink`): download `id` gets
    /// its other sources and its hashes. Nothing for any other address.
    pub(super) fn metalink_parts(&mut self, id: u64, fi: &FileInfo) {
        if let Some(item) = self.item(id) {
            item.mirrors = fi.mirrors();
            item.checksum = fi.checksum();
            item.pieces = fi.pieces();
        }
    }

    /// Options → Downloads → "Start downloading immediately while displaying Download File
    /// Info": once the address answered (name and size known) and its folder exists, the
    /// download starts behind the dialog; the dialog's answer then keeps, moves or drops it.
    pub(super) fn start_early(&mut self, fi: &mut FileInfo) {
        if !self.settings.start_immediately || fi.early.is_some() || !fi.probed() {
            return;
        }
        // Once Save As has been looked at off the window's thread (#743): its name numbered in
        // its folder, and the folder there. A folder still to be created waits for the dialog's
        // question; a look still under way, for its answer.
        if fi.folder_found() != Some(true) {
            return;
        }
        let dest = fi.dest();
        let id = self.push_item(
            fi.url.clone(),
            dest,
            fi.category,
            fi.description.clone(),
            fi.size(),
            fi.login.clone(),
            fi.context.clone(),
            if fi.context.referer.is_some() || fi.context.cookies.is_some() {
                crate::i18n::mark("from the browser")
            } else {
                crate::i18n::mark("with Add URL")
            },
        );
        self.set_direct(id, fi.direct);
        // A Metalink's file started early goes with its mirrors and hashes as one added later
        // does (1.1.5 started it from its first address alone, its hash unchecked).
        self.metalink_parts(id, fi);
        self.number_at_start(id);
        // Not in the list until the dialog's Start or Later (the owner's report, 0.6.5: the row
        // showed as the dialog opened, and stayed after Cancel's removal had to catch up).
        if let Some(item) = self.item(id) {
            item.provisional = true;
        }
        fi.early = Some(id);
        if let Some(k) = fi.take_kept() {
            self.kept_answers.insert(id, k);
        }
        tracing::info!(
            "start time: download {id} started behind Download File Info {} ms after it opened",
            crate::starttime::ms(fi.opened)
        );
        self.start(id, false);
    }

    /// Download File Info closed with Start / Later on a download that started early.
    pub(super) fn adopt_early(&mut self, fi: &FileInfo, id: u64, start: Start) {
        if let Some((cat, dir)) = fi.remembered() {
            self.settings
                .keep_category_dir(cat, std::path::Path::new(&dir), &self.downloads);
        }
        let dest = fi.dest();
        let now = matches!(start, Start::Now);
        let Some(item) = self.item(id) else { return };
        // In the list from now on, as a download added with this answer is.
        item.provisional = false;
        item.category = fi.category;
        item.description = fi.description.clone();
        let moved = item.dest != dest;
        self.selection.set(id);
        // Finished behind the dialog: its end goes on now (the sound, the statistics, its box,
        // its archive), as if it had just come; not a file about to be started over elsewhere.
        let held = self.held_finished.iter().position(|h| *h == id);
        if let Some(k) = held {
            self.held_finished.remove(k);
            if !moved {
                self.released_finished.push(id);
            }
        }
        if moved {
            // Another name or folder than the one it started at: the data so far goes and it
            // starts over there.
            self.stop(id);
            self.after_stop.push((id, Some((dest, now))));
        } else if now {
            let (live, complete) = self
                .items
                .iter()
                .find(|i| i.id == id)
                .map_or((false, false), |i| {
                    (i.live.is_some(), i.status == Status::Complete)
                });
            // Finished behind the dialog: its box comes with its end (`released_finished`).
            if complete {
                self.dirty = true;
                return;
            }
            if !live {
                self.start(id, true);
            } else {
                self.open_started_progress(id);
            }
        } else {
            // Later: paused, its data kept for the queue to resume.
            self.stop(id);
        }
        if !now {
            self.queue_later(vec![id], self.settings.queue_panel_later);
        }
        self.dirty = true;
    }

    /// Carry out `after_stop` for downloads that have stopped.
    pub(super) fn finish_stopped(&mut self) {
        if self.after_stop.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut self.after_stop);
        for (id, then) in pending {
            if self.item(id).is_some_and(|i| i.live.is_some()) {
                self.after_stop.push((id, then));
                continue;
            }
            self.take_made();
            let Some(item) = self.item(id) else { continue };
            let old = item.dest.clone();
            match then {
                None => {
                    // Cancelled: a file that finished meanwhile goes too, with its windows, and
                    // the folder DDM made for it once empty. On another thread: a slow or stuck
                    // disk took the window with it (#423).
                    let complete = item.status == Status::Complete;
                    let files = files_of(&old, complete);
                    let prune = self
                        .items
                        .iter()
                        .find(|i| i.id == id)
                        .and_then(|i| self.prune_for(i));
                    self.remove_beside(Removing::of(&files, None), move || {
                        remove_files(&files);
                        if let Some(prune) = &prune {
                            prune.run();
                        }
                    });
                    self.complete.retain(|c| c.id != id);
                    self.delete(id);
                }
                Some((dest, start)) => {
                    // A restart: the old files go off the window thread (#423, #743). Under the
                    // same name they are gone before the new start, which waits for them
                    // (`Removing`, `start_look`).
                    let complete = item.status == Status::Complete;
                    let files = files_of(&old, complete);
                    self.remove_beside(Removing::of(&files, None), move || remove_files(&files));
                    let item = self.item(id).expect("the download just found");
                    if let Some(name) = dest.file_name() {
                        item.file_name = name.to_string_lossy().into_owned();
                    }
                    item.dest = dest;
                    item.downloaded = 0;
                    item.status = Status::Stopped;
                    // Download File Info's place, given without waiting for the disk (#743).
                    item.number_at_start = true;
                    if start {
                        self.start(id, true);
                    }
                }
            }
            self.dirty = true;
        }
    }

    /// Downloads added with Later (or by a batch): into the main queue, or the queue chosen in
    /// the selection panel when `ask` (Options → Downloads).
    pub(super) fn queue_later(&mut self, ids: Vec<u64>, ask: bool) {
        if ids.is_empty() {
            return;
        }
        if ask && self.queue_pick.is_none() {
            let names = self.queues.iter().map(|q| q.name.clone()).collect();
            self.queue_pick = Some((ids, dialogs::QueuePick::new(names)));
        } else {
            self.queues[0].items.extend(ids);
        }
        self.dirty = true;
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn push_item(
        &mut self,
        url: url::Url,
        dest: PathBuf,
        category: Category,
        description: String,
        size: Option<u64>,
        login: Option<(String, String)>,
        context: ipc::Command,
        source: &str,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let mut item = Item::new(id, url, dest, category);
        item.note(crate::i18n::fmt(
            crate::i18n::t("Added {source}"),
            &[("source", crate::i18n::t(source))],
        ));
        item.size = size;
        item.description = description;
        item.login = login;
        // The browser had started it: the host is already sending the file (#44).
        item.direct = crate::debrid::browser_started(&context);
        // The page it came from (the browser's, a site grabber project's), as File Properties
        // shows it: kept with the list, as the Referer is.
        item.page = context.referer.clone().filter(|r| model::web_address(r));
        item.context = context;
        self.items.push(item);
        self.dirty = true;
        id
    }

    /// Download `id` skips the debrid accounts, or goes through them again (#44).
    pub(super) fn set_direct(&mut self, id: u64, direct: bool) {
        if let Some(item) = self.item(id) {
            item.direct = direct;
        }
    }

    /// Download `id`'s name is only its address's guess (`guessed`): the server names it when
    /// it starts (`Item::name_from_url`).
    pub(super) fn name_guessed(&mut self, id: u64, guessed: bool) {
        if guessed && let Some(item) = self.item(id) {
            item.name_from_url = true;
        }
    }

    /// The headers and options `url`'s requests go with: the download's own `login`, else the
    /// one Options → Sites Logins has for its address, else an FTP address's own (its resume
    /// state keeps the address without the password); the browser's Referer, Cookie,
    /// User-Agent and form (`context`); Options' connections, proxy and User-Agent.
    ///
    /// Through the first debrid account whose service takes its host (#44), unless turned off
    /// or `direct` (`Item::direct`).
    pub fn request_parts(
        &self,
        url: &url::Url,
        login: Option<&(String, String)>,
        context: &ipc::Command,
        direct: bool,
    ) -> RequestParts {
        let site_login = self.settings.site_login(url);
        let url_login = mdm_engine::ftp::is_ftp(url)
            .then(|| url_login(url))
            .flatten();
        let mut headers = auth_headers(login.or(site_login.as_ref()).or(url_login.as_ref()));
        let post_type = context.post.as_ref().and(context.post_type.as_ref());
        let referer = context
            .referer
            .as_deref()
            .and_then(|r| sent_referrer(url, r));
        for (name, v) in [
            (reqwest::header::REFERER, &referer),
            (reqwest::header::COOKIE, &context.cookies),
            (reqwest::header::CONTENT_TYPE, &post_type.cloned()),
        ] {
            if let Some(v) = v.as_deref().and_then(|v| HeaderValue::from_str(v).ok()) {
                headers.insert(name, v);
            }
        }
        // The browser's own User-Agent when it handed the download over, else the one set in
        // Options, else the automatic one (the default browser's, #668).
        let own = self.settings.own_user_agent();
        let given = context.user_agent.as_deref().or(own);
        let opts = Options {
            max_connections: self.settings.connections_for(url),
            proxy: self.settings.proxy_for(context),
            dns: self.settings.dns(),
            route: self.route.clone(),
            // Through a debrid account when one takes this host (#44).
            unlocker: self.debrid_route().unlocker(url, direct),
            global: Some(self.global.clone()),
            host_limits: Some(self.host_limits.clone()),
            ignore_mtime: self.settings.ignore_mtime,
            server_time: self.settings.set_creation_date,
            ftp_passive: self.settings.ftp_passive,
            user_agent: given.map_or_else(mdm_engine::agent::automatic, str::to_owned),
            automatic_agent: given.is_none(),
            // A browser's User-Agent (handed over, or the automatic one) may be refused on this
            // connection (#372): the one set in Options, else DDM's former one (Internet
            // Explorer 11's, which such bot checks let through), is tried once then. One the
            // user set is the user's choice: no other.
            agent_fallback: match (context.user_agent.is_some(), own) {
                (true, Some(own)) => Some(own.to_owned()),
                (_, None) => Some(mdm_engine::agent::IE11_USER_AGENT.to_owned()),
                (false, Some(_)) => None,
            },
            // Where the browser's own download of it ended: asked when the link itself is
            // refused (#630, a one-time or session-bound token the browser already used).
            browser_final: context
                .final_url
                .as_deref()
                .and_then(|u| url::Url::parse(u).ok())
                .filter(|f| f != url && mdm_engine::supported(f)),
            post: context.post.clone().map(|body| body.into_bytes().into()),
            on_file_created: Some(mark_hook(url, context.referer.as_deref())),
            // The browser's own download's type: a web page instead is an error (#533).
            expected_type: context.mime.clone(),
            ..Options::default()
        };
        RequestParts { headers, opts }
    }

    /// What file download `id`'s start asks the disk, beside the window (`StartLook`, #743):
    /// copies of its row and of the others in its folder, and the files Redownload is removing
    /// for it, or that it would write among and are being removed (`Removing`: Delete with its
    /// files, Overwrite...), to wait for.
    pub(super) fn start_look(&mut self, id: u64) -> Option<StartLook> {
        let mut clearing: Vec<_> = self.clearing.remove(&id).into_iter().collect();
        self.removing.retain(Removing::under_way);
        let me = self.items.iter().find(|i| i.id == id)?;
        let dir = me.dest.parent();
        clearing.extend(waits_for(&self.removing, dir, &Writes::of(me)));
        let others = || {
            self.items
                .iter()
                .filter(move |o| o.id != id && o.dest.parent() == dir)
        };
        Some(StartLook {
            me: me.place_copy(),
            guessed: me.name_from_url,
            fresh: me.number_at_start,
            others: others().map(Item::place_copy).collect(),
            running: others()
                .filter(|o| o.live.is_some())
                .map(|o| o.id)
                .collect(),
            clearing,
        })
    }

    /// Start or resume a download; `show` opens its progress dialog (IDM's default).
    pub(super) fn start(&mut self, id: u64, show: bool) {
        self.stopped_by_hand.remove(&id);
        // Started by the person (its window shows): no longer a queue's quiet download.
        if show {
            self.queue_started.remove(&id);
        }
        let rt = self.rt.clone();
        let Some(item) = self.item(id) else { return };
        // A finished gallery may start again: it fetches the files not there yet (#58).
        if item.live.is_some() || (item.status == Status::Complete && item.gallery.is_none()) {
            return;
        }
        // The download limit is reached: it starts when the window has room again (a video or a
        // gallery too, #708: their bytes count toward it).
        if self.settings.limits_on && self.quota.blocked {
            if !self.quota.held.contains(&id) {
                self.quota.held.push(id);
            }
            if let Some(item) = self.item(id) {
                item.status = Status::Queued;
                item.note(crate::i18n::t(
                    "Waiting: the download limit is reached (Scheduler)",
                ));
            }
            self.dirty = true;
            return;
        }
        // A video from a page: yt-dlp, not the engine.
        if self.item(id).is_some_and(|i| i.video.is_some()) {
            self.start_video(id, show);
            self.share_with_new_helper(id);
            return;
        }
        // An image gallery: gallery-dl (Redownload's removal, of files it doesn't write, isn't
        // waited for).
        if self.item(id).is_some_and(|i| i.gallery.is_some()) {
            self.clearing.remove(&id);
            // Its share of the helpers' limit is given as it starts (`gallery_found`).
            self.start_gallery(id, show);
            return;
        }
        if self.item(id).is_some_and(|i| i.torrent.is_some()) {
            self.start_torrent(id, show);
            return;
        }
        // A torrent through a debrid account (#44).
        if self.item(id).is_some_and(|i| i.remote.is_some()) {
            self.start_remote(id, show);
            return;
        }
        let Some((url, login, context, own_limit, checksum, mirrors, direct)) =
            self.item(id).map(|i| {
                (
                    i.url.clone(),
                    i.login.clone(),
                    i.context.clone(),
                    i.own_limit,
                    i.checksum.clone(),
                    i.mirrors.clone(),
                    i.direct,
                )
            })
        else {
            return;
        };
        // The browser's proxy needs its VPN extension's own login (#42): said, never bypassed.
        if let Some(why) = self.settings.browser_proxy_refused(&context) {
            if let Some(item) = self.item(id) {
                item.status = Status::Error;
                item.error = Some(why);
                item.bot_check = None;
                item.last_try = Some(model::now_stamp());
            }
            self.dirty = true;
            return;
        }
        let RequestParts { headers, mut opts } =
            self.request_parts(&url, login.as_ref(), &context, direct);
        // Download File Info's open answer for it, if it has one (#28); used once.
        let kept = self.kept_answers.remove(&id);
        // The file's own remembered limit, else the Speed Limiter's.
        opts.speed_limit = own_limit.or(self.settings.speed_limit());
        opts.checksum = checksum;
        opts.mirrors = mirrors;
        opts.pieces = self
            .item(id)
            .and_then(|i| i.pieces.clone())
            .map(std::sync::Arc::new);
        // What the disk has to say (a resume state there, a place another row holds, its folder
        // there or not) is asked beside the window as the download starts (`StartLook`, #743): a
        // sleeping hard disk, a USB drive or a network folder holds up no frame, and the row
        // runs (counted by its queue, stopped by Stop) from now. Files Redownload is removing go
        // first.
        let Some(look) = self.start_look(id) else {
            return;
        };
        let Some(item) = self.item(id) else { return };
        // A new download from Download File Info (or added without it, #743) has nothing of its
        // own on the disk yet: it starts from nothing.
        if look.fresh {
            item.downloaded = 0;
        }
        let _guard = rt.enter();
        let dl = Download::begin(
            item.url.clone(),
            headers,
            opts,
            kept,
            item.downloaded,
            move || look.decide(),
        );
        item.note(if item.downloaded > 0 {
            // i18n: event log entry, past tense: the download resumed (just now)
            crate::i18n::t("Resumed")
        } else {
            // i18n: event log entry, past tense: the download started (just now)
            crate::i18n::t("Started")
        });
        item.live = Some(Live::new(model::Job::File(dl), item.downloaded));
        item.status = Status::Downloading;
        item.error = None;
        item.last_try = Some(model::now_stamp());
        self.dirty = true;
        if show {
            self.open_started_progress(id);
        }
    }

    pub(super) fn stop(&mut self, id: u64) {
        if let Some(l) = self.item(id).and_then(|i| i.live.as_ref()) {
            l.dl.cancel();
        }
    }

    /// The user's Stop (or Pause): it stays stopped, even in a queue that is running (#341).
    pub(super) fn stop_by_hand(&mut self, id: u64) {
        self.stop(id);
        self.stopped_by_hand.insert(id);
    }

    pub(super) fn stop_all(&mut self) {
        // Running queues stop with them: else a queue would start its next downloads at once,
        // and Stop All (Pause All) would leave downloads running. Started again, a queue goes on
        // where it stopped.
        for q in 0..self.queues.len() {
            if self.queues[q].running {
                self.stop_queue(q);
            }
        }
        // The list's downloads: not one started early behind Download File Info (its answer
        // decides).
        let running: Vec<u64> = self
            .items
            .iter()
            .filter(|i| i.live.is_some() && i.listed())
            .map(|i| i.id)
            .collect();
        for id in running {
            self.stop_by_hand(id);
        }
    }

    /// Whether a double-click or Enter on a download opens it (a finished one's file, an
    /// unfinished one's progress window) rather than showing its File Properties: On Double
    /// click → Open. Properties, IDM's default, for every row, finished, stopped or running.
    pub(super) fn enter_opens(&self) -> bool {
        self.settings.double_click_open
    }

    pub(super) fn open_item(&mut self, id: u64) {
        let Some(i) = self.items.iter().find(|i| i.id == id) else {
            return;
        };
        if i.status == Status::Complete && i.live.is_none() {
            // Known to be gone: asked at once, nothing handed to the system (#5). Otherwise the
            // file is looked for beside the window, and reported if gone (`files_gone`).
            if i.missing {
                let (name, dest) = (i.file_name.clone(), i.dest.clone());
                self.ask_missing(id, &name, &dest);
            } else {
                crate::dialogs::open_file(&i.dest);
            }
        } else {
            self.show_progress(id);
        }
    }

    /// The download already there: its "Download complete" box when finished, else resumed.
    pub(super) fn open_or_resume(&mut self, id: u64) {
        let Some(i) = self.items.iter().find(|i| i.id == id) else {
            return;
        };
        self.selection.set(id);
        if i.status == Status::Complete {
            let c = Complete::new(
                id,
                i.url.to_string(),
                i.dest.clone(),
                i.size.unwrap_or(i.downloaded),
                i.notices.clone(),
            );
            // Its box once: after the one shown, if one is (they show one at a time).
            if !self.complete.iter().any(|w| w.id == id) {
                self.complete.push(c);
            }
        } else if i.live.is_none() {
            self.start(id, true);
        } else {
            self.open_started_progress(id);
        }
    }

    /// Can this download be fetched again from the start? A file download that isn't running
    /// (torrents re-check their data instead; a video's file is yt-dlp's to replace).
    pub(super) fn can_redownload(&self, id: u64) -> bool {
        self.items
            .iter()
            .find(|i| i.id == id)
            .is_some_and(|i| i.live.is_none() && i.torrent.is_none() && i.video.is_none())
    }

    /// Download again from the start: the partial state goes; a finished file is replaced when
    /// the new download completes.
    pub(super) fn redownload(&mut self, id: u64) {
        if !self.can_redownload(id) {
            return;
        }
        let Some(item) = self.item(id) else { return };
        // Its partial state and data go beside the window (#743), its start waiting for them
        // (`StartLook`). A name given without a look at its folder has nothing of its own there.
        if !item.number_at_start {
            let mut files = vec![
                state_path(&item.dest),
                mdm_engine::naming::part_path(&item.dest),
            ];
            files.extend(mdm_engine::state::legacy_files(&item.dest));
            let (gone, clearing) = std::sync::mpsc::channel();
            self.rt.spawn_blocking(move || {
                for f in files {
                    let _ = std::fs::remove_file(f);
                }
                let _ = gone.send(());
            });
            self.clearing.insert(id, clearing);
        }
        let Some(item) = self.item(id) else { return };
        item.downloaded = 0;
        item.status = Status::Stopped;
        item.error = None;
        item.note(crate::i18n::t("Downloading again from the start"));
        self.dirty = true;
        self.start(id, true);
    }
}

/// The place a download about to start takes instead of its own, when its own belongs to
/// another row (#442): two can wait under one name (nothing was on the disk when each was added,
/// and they can start in the same moment, before the engine has reserved anything). None: it
/// stays. A row with data of its own never moves, so a resume finds its pieces. Whether another
/// row has its place on the disk is said by `holds` (`Item::holds_place` and the rows that run:
/// rows copied for a thread other than the window's know nothing of running, `StartLook`).
/// Looks at the disk: on a thread other than the window's (#743).
pub(super) fn place_to_take_by(
    item: &Item,
    items: &[Item],
    holds: impl Fn(&Item) -> bool,
) -> Option<PathBuf> {
    if item.downloaded > 0 || item.owns_resume_state() {
        return None;
    }
    let dir = item.dest.parent()?;
    let others = || {
        items
            .iter()
            .filter(|o| o.id != item.id && o.dest.parent() == Some(dir))
    };
    let lower = |p: &std::path::Path| p.to_string_lossy().to_lowercase();
    let stem = |o: &Item| {
        o.dest
            .file_stem()
            .map(|s| s.to_string_lossy().to_lowercase())
    };
    if item.video.is_some() {
        // yt-dlp takes any `<stem>.*` there as its own (#32): the stem has to be free.
        others()
            .any(|o| stem(o) == stem(item) && holds(o))
            .then(|| {
                mdm_engine::naming::stem_destination(dir, &item.file_name, |s| {
                    let s = s.to_lowercase();
                    others().any(|o| stem(o).as_ref() == Some(&s) && holds(o))
                })
            })
    } else {
        others()
            .any(|o| lower(&o.dest) == lower(&item.dest) && holds(o))
            .then(|| {
                let mut taken: std::collections::HashSet<PathBuf> = others()
                    .filter(|o| holds(o))
                    .map(|o| o.dest.clone())
                    .collect();
                taken.insert(item.dest.clone());
                place_in(dir, &item.file_name, &mut taken)
            })
    }
}

/// The login written in an address (`ftp://user:pass@host/…`), decoded.
pub(super) fn url_login(url: &url::Url) -> Option<(String, String)> {
    let password = url.password()?;
    let decode = |s: &str| {
        percent_encoding::percent_decode_str(s)
            .decode_utf8_lossy()
            .into_owned()
    };
    Some((decode(url.username()), decode(password)))
}

/// The Referer a browser would send for `url` from page `referrer` (its default policy,
/// strict-origin-when-cross-origin): the whole address on the same site, only the site's origin
/// to another, nothing from https to http.
pub(super) fn sent_referrer(url: &url::Url, referrer: &str) -> Option<String> {
    let from = url::Url::parse(referrer).ok()?;
    if !matches!(from.scheme(), "http" | "https") {
        return None;
    }
    if from.scheme() == "https" && url.scheme() == "http" {
        return None;
    }
    if from.origin() == url.origin() {
        let mut full = from.clone();
        full.set_fragment(None);
        let _ = full.set_username("");
        let _ = full.set_password(None);
        return Some(full.to_string());
    }
    Some(format!("{}/", from.origin().ascii_serialization()))
}

/// What a link found on a page takes along from that page.
pub(super) fn page_data_for(link: &str, page: Option<&str>, data: Option<&str>) -> Option<String> {
    let (link, page) = (url::Url::parse(link).ok()?, url::Url::parse(page?).ok()?);
    let same_host = link.host_str().is_some()
        && link
            .host_str()
            .map(str::to_ascii_lowercase)
            .eq(&page.host_str().map(str::to_ascii_lowercase));
    let downgrade = page.scheme() == "https" && link.scheme() != "https";
    (same_host && !downgrade)
        .then(|| data.map(str::to_owned))
        .flatten()
}

/// Marks finished files (a video's and its subtitles, a gallery's pictures) as downloaded from the
/// web, as `mark_hook` does for the engine's, on a thread of their own: writing the mark has the
/// antivirus read the whole file again, and on a big video that kept the window waiting (#510). A
/// file moved or removed meanwhile is left alone: marking its old path would create an empty
/// file there.
pub(crate) fn mark_finished(
    files: Vec<std::path::PathBuf>,
    url: String,
    referrer: Option<String>,
) -> Option<std::thread::JoinHandle<()>> {
    std::thread::Builder::new()
        .name("mark-downloaded".into())
        .spawn(move || {
            for f in files.iter().filter(|f| f.is_file()) {
                #[cfg(windows)]
                crate::win32::mark_downloaded(f, &url, referrer.as_deref());
                #[cfg(unix)]
                crate::unix::mark_downloaded(f, &url, referrer.as_deref());
            }
        })
        .ok()
}

/// Marks each file the engine starts writing as downloaded from the web (Mark-of-the-Web, macOS'
/// quarantine: win32 / unix `mark_downloaded`), as browsers mark theirs. Only http(s) addresses
/// (an FTP address may carry its login).
pub(crate) fn mark_hook(url: &url::Url, referrer: Option<&str>) -> mdm_engine::FileHook {
    let web = matches!(url.scheme(), "http" | "https");
    let (url, referrer) = (url.to_string(), referrer.map(str::to_owned));
    mdm_engine::FileHook::new(move |path: &std::path::Path| {
        if !web {
            return;
        }
        #[cfg(windows)]
        crate::win32::mark_downloaded(path, &url, referrer.as_deref());
        #[cfg(unix)]
        crate::unix::mark_downloaded(path, &url, referrer.as_deref());
    })
}

pub fn auth_headers(login: Option<&(String, String)>) -> HeaderMap {
    let mut h = HeaderMap::new();
    if let Some((user, pass)) = login {
        let token = base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}"));
        if let Ok(v) = HeaderValue::from_str(&format!("Basic {token}")) {
            h.insert(AUTHORIZATION, v);
        }
    }
    h
}
