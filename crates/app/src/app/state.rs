//! The main window's state (`App`): what it holds, how it starts (`App::new`) and what it does as
//! it closes (`Drop`: the list saved, downloads handed back to the browser).

use super::*;

pub struct App {
    pub(super) rt: tokio::runtime::Handle,
    pub(super) items: Vec<Item>,
    pub(super) next_id: u64,
    /// The list's selection (several downloads, Ctrl / Shift).
    pub(super) selection: crate::selection::Selection,
    pub(super) view: View,
    pub(super) show_categories: bool,
    /// The node the list was last drawn for (a new one starts at the top).
    pub(super) list_view: View,
    /// The download Move/Rename's dialog is for (winui::start_pick answers later).
    pub(super) move_rename_for: Option<u64>,
    /// The downloads Move/Rename's folder dialog is for, several selected (#333).
    pub(super) move_many_for: Vec<u64>,
    /// The Categories tree has the keys (else the list; Tab moves between them).
    pub(super) tree_keys: bool,
    /// The app's context, to reach its windows from anywhere (bring one to the front).
    pub(super) ui_ctx: egui::Context,
    /// The language screen readers were last told.
    pub(super) a11y_lang: &'static str,
    /// Maximized as last seen (the saved size is the restored window's, not the screen's).
    pub(super) maximized: Option<bool>,
    pub(super) all_open: bool,
    pub(super) unfinished_open: bool,
    pub(super) finished_open: bool,
    pub(super) queues_open: bool,
    pub(super) add_url: Option<AddUrl>,
    pub(super) file_info: Option<FileInfo>,
    /// Addresses (and their browser context, and the Metalink file they are, when one is)
    /// waiting for Download File Info to close.
    pub(super) file_info_waiting: std::collections::VecDeque<(
        dialogs::NewUrl,
        ipc::Command,
        Option<crate::metalinkdlg::Pick>,
    )>,
    /// Download File Info's open answers, for the download each was given to (by id) to start
    /// from (#28: a one-time link refuses a second request).
    pub(super) kept_answers: std::collections::HashMap<u64, mdm_engine::KeptAnswer>,
    /// Help menu windows.
    pub(super) about: Option<crate::help::About>,
    pub(super) updates: Option<crate::help::Updates>,
    pub(super) shortcuts: Option<crate::help::Shortcuts>,
    pub(super) tell_friend: Option<crate::help::TellFriend>,
    /// The language picker (first start, View → Language...).
    pub(super) lang_picker: Option<crate::langpick::Picker>,
    /// "Files already there", opened on its own by `/show existing-files` (screenshots and the
    /// translations' fit check); Add torrent asks it itself.
    pub(super) shown_existing: Option<crate::dialogs::ExistingFiles>,
    /// The browser extension's offer (extension.rs), and whether this run has weighed it.
    pub(super) extension: Option<crate::extension::Extension>,
    pub(super) extension_weighed: bool,
    /// What's new in this version (notes.rs), shown once after an update; whether this run has
    /// weighed it, and whether DDM had run before this start (it isn't shown on a first start).
    pub(super) whats_new: Option<crate::notes::WhatsNew>,
    pub(super) whats_new_weighed: bool,
    pub(super) ran_before: bool,
    /// The weekly update check, while it runs.
    pub(super) auto_update: Option<crate::help::Slot>,
    /// The automatic check found a new version: its window comes to the front once shown.
    pub(super) raise_updates: bool,
    /// Options was asked for from outside (the extension's Button settings..., `/show`): it comes
    /// to the front once shown, or it would open behind the browser (#77).
    pub(super) raise_options: bool,
    /// When DDM started (the update check waits a few seconds after it).
    pub(super) started: Instant,
    /// When this run last checked for updates by itself (seconds since the epoch).
    pub(super) update_checked_this_run: Option<i64>,
    pub(super) progress: Vec<Progress>,
    pub(super) complete: Vec<Complete>,
    /// Options → Downloads → Virus checking: the checks' answers, from beside the window
    /// (viruscheck.rs; `poll_virus_checks`).
    pub(super) virus: (
        std::sync::mpsc::Sender<crate::viruscheck::Done>,
        std::sync::mpsc::Receiver<crate::viruscheck::Done>,
    ),
    /// Downloads just finished whose progress window shows them whole for a moment
    /// ([`progress::SHOWN_WHOLE`]) before it gives way to Download complete: each one's id, and
    /// when it finished.
    pub(super) finishing: Vec<(u64, Instant)>,
    pub(super) settings: Settings,
    pub(super) speed_settings: Option<dialogs::SpeedSettings>,
    /// Beyond IDM: what came by day, site and kind (stats.json), each download's speed second by
    /// second and all of them together (the speed graphs), and Downloads → Statistics.
    pub(super) stats: crate::stats::Recorder,
    pub(super) speeds: crate::speedgraph::Speeds,
    pub(super) statistics: Option<crate::statsdlg::Statistics>,
    pub(super) scheduler: Option<crate::scheduler::Scheduler>,
    pub(super) options: Option<crate::options::OptionsDialog>,
    /// Downloads to finish off once stopped: Download File Info was cancelled after an early
    /// start (`None`: the download and its data go), or its file was given another place
    /// (`Some((dest, start))`: the data so far goes, it starts over there when `start`).
    pub(super) after_stop: Vec<(u64, Option<(PathBuf, bool)>)>,
    /// Downloads started early behind Download File Info that finished before its answer
    /// (`Item::provisional`): what their end sets off (the sound, the statistics, the Download
    /// complete box, an archive's extraction) waits for Start or Later, and Cancel drops it.
    pub(super) held_finished: Vec<u64>,
    /// Those the answer took into the list: ended at the next pass, as if they had just finished.
    pub(super) released_finished: Vec<u64>,
    /// Tasks → Show drop target: the floating box links are dragged onto.
    pub(super) drop_target: Option<crate::droptarget::DropTarget>,
    /// Links and files dropped on the main window.
    pub(super) drops: crate::droptarget::Drops,
    /// The queue selection panel after Download Later / a batch: the downloads waiting for it.
    pub(super) queue_pick: Option<(Vec<u64>, dialogs::QueuePick)>,
    /// Link grabber (#37): links from the browser, Tasks → Import, drops and pastes, in
    /// packages to tick before adding.
    pub(super) grabber: Option<crate::grabber::Grabber>,
    /// View → Theme → Choose a theme...: the theme picker, while open.
    pub(super) theme_picker: Option<crate::themepick::Picker>,
    /// Site grabber: its projects and its window (app/sitegrab.rs).
    pub(super) grabs: sitegrab::Grabs,
    /// When `follow_assistive_technology` last asked (#131).
    pub(super) assistive_checked: Option<Instant>,
    /// Archives extracted when their download completes (#37).
    pub(super) extracts: crate::extractq::Extracts,
    /// Add category / Properties from the Categories tree.
    pub(super) category_dlg: Option<crate::categories::CategoryDialog>,
    /// The category dialog was opened by Download File Info's "+" (its result is picked there).
    pub(super) category_for_file_info: bool,
    /// Delete a category of the user's, asking first.
    pub(super) confirm_category_delete: Option<dialogs::DeleteCategory>,
    /// File Properties windows, one per download.
    pub(super) properties: Vec<crate::properties::Properties>,
    /// Verify checksum windows, one per download.
    pub(super) checksums: Vec<crate::checksum::ChecksumDialog>,
    pub(super) batch: Option<crate::batch::Batch>,
    pub(super) video_dlg: Option<crate::videodlg::VideoDialog>,
    /// Video pages handed over by the browser's button while Download video was open, oldest
    /// first: each opens in it when the one before it closes (one window at a time, as Download
    /// File Info and Add torrent: never one replacing another).
    /// A quality chosen in the browser's menu waits with its look-up (it opens as that choice,
    /// nothing looked up again).
    pub(super) video_waiting: std::collections::VecDeque<(
        String,
        ipc::Command,
        Option<Box<crate::videolookup::Picked>>,
    )>,
    /// Download video closed this frame: the next waiting page opens after a frame without it.
    pub(super) video_closed: bool,
    /// Downloads a running queue started (until the person starts one again): no progress window
    /// opens for them, and their end opens no Download complete, as IDM's queue downloads don't,
    /// so a queue of thirty files never ends with thirty windows (atch_quiet).
    pub(super) queue_started: std::collections::HashSet<u64>,
    /// The browser's quality menu's look-ups (videolookup.rs).
    pub(super) lookups: crate::videolookup::Lookups,
    /// Download a gallery (#58).
    pub(super) gallery_dlg: Option<crate::gallerydlg::GalleryDialog>,
    pub(super) torrent_dlg: Option<crate::torrentdlg::TorrentDialog>,
    /// Magnet links and .torrents handed over while Add torrent was open (#507), oldest first:
    /// each opens in Add torrent when the one before it closes.
    pub(super) torrent_waiting:
        std::collections::VecDeque<(String, Option<crate::torrents::Remote>)>,
    /// Add torrent closed this frame: the next waiting one opens after a frame without it.
    pub(super) torrent_closed: bool,
    /// Add Metalink (a Metalink of several files, `metalinkdlg`), the Metalinks handed over
    /// while it was open (each opens when the one before it closes, a frame later), and
    /// whether it closed this frame.
    pub(super) metalink_dlg: Option<crate::metalinkdlg::MetalinkDialog>,
    pub(super) metalink_waiting: std::collections::VecDeque<crate::metalinkdlg::Opened>,
    pub(super) metalink_closed: bool,
    pub(super) torrent_settings: Option<crate::torrentdlg::TorrentSettings>,
    /// Right-click → Torrent → Add trackers… (#39).
    pub(super) add_trackers: Option<crate::torrentdlg::AddTrackers>,
    pub(super) torrent_files: Option<crate::torrentfiles::TorrentFiles>,
    /// RSS feeds and their download rules (#39, rss.rs), kept in rss.json; the window; the
    /// feeds being fetched; what the fetches and look-ups sent back (`rss_tick`).
    pub(super) rss: crate::rss::Store,
    pub(super) rss_dlg: Option<crate::rssdlg::RssWindow>,
    pub(super) rss_busy: std::collections::HashSet<u64>,
    pub(super) rss_events: std::sync::Arc<std::sync::Mutex<Vec<RssEvent>>>,
    /// Torrents whose list numbers wait for the session after an action (Recheck, files
    /// chosen; `torrent_resync`), by info hash: true when what the check found goes in the
    /// event log.
    pub(super) torrent_resync: HashMap<String, bool>,
    /// Since when no torrent has needed the torrent engine (`torrent_tick`).
    pub(super) torrent_idle_since: Option<Instant>,
    /// What each torrent sent, run after run (`torrent_accounts`), when last counted (whole
    /// seconds), and when its totals last made the list be saved.
    pub(super) accountant: crate::torrents::Accountant,
    pub(super) account_at: Instant,
    pub(super) account_saved: Instant,
    /// Each running torrent's bytes from peers as last seen (by item id), and the torrents'
    /// smoothed speed with when it was measured (`torrent_speed`, for the global cap).
    pub(super) torrent_fetched: HashMap<u64, u64>,
    pub(super) torrent_speed: (f64, Instant),
    /// The torrents' share of the global cap as last set, and when (`apply_global_limit`).
    pub(super) torrent_cap: (Option<u64>, Instant),
    /// File downloads' smoothed speed, when measured, and the global limiter's byte count
    /// then (`file_speed`).
    pub(super) file_speed: (f64, Instant, u64),
    /// A blocklist is being fetched (torrents::refresh_blocklist).
    pub(super) blocklist_busy: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Startup work for torrents done (associations refreshed, sharing resumed).
    pub(super) torrents_started: bool,
    /// The torrent session, started on first use (D-014).
    pub(super) torrent_host: crate::torrents::Host,
    /// The remote control (#141): paired phones and computers on the local network.
    pub remote: Option<mdm_remote::Remote>,
    /// Torrents a paired device sent, their files being looked up (`remote::poll_torrents`).
    pub remote_torrents: Vec<crate::remote::Lookup>,
    /// Free space where downloads go, and when it was last looked up (`free_space`).
    pub(super) free_space: std::sync::Arc<std::sync::Mutex<(Option<Instant>, Option<u64>)>>,
    /// Options → Proxy / Socks → Network adapter (#35): the watch, the route seen last frame, and
    /// the downloads stopped because it changed, to start again when it allows.
    pub(super) netwatch: crate::netwatch::NetWatch,
    /// Options → Accounts (#44): the debrid accounts downloads go through, and what they were
    /// set up from (so they are set up again only when that changes).
    pub(super) debrid: mdm_debrid::Debrid,
    pub(super) debrid_from: u64,
    pub(super) route: mdm_engine::Route,
    pub(super) adapter_paused: Vec<u64>,
    /// Videos and galleries (#708): yt-dlp and gallery-dl take a speed limit only as they start
    /// (`--limit-rate`). The limits each running one was started under (`helper_limits`), the limit
    /// set in its progress window without Remember (it lasts until it is stopped), and the ones
    /// stopped to start again at a new limit, started once their jobs have ended
    /// (`limit_restart_tick`). #769 moves their downloads to the engine, whose limits follow a
    /// change by themselves.
    pub(super) helper_rates: HashMap<u64, (Option<u64>, Option<u64>)>,
    /// The rate each running video and gallery was started with (`helper_rate`).
    pub(super) helper_started: HashMap<u64, Option<u64>>,
    pub(super) helper_own: HashMap<u64, Option<u64>>,
    pub(super) limit_restarts: Vec<u64>,
    pub(super) find: Option<crate::find::Find>,
    /// View → Choose columns, while open.
    pub(super) columns_dlg: Option<crate::columnsdlg::CustomizeColumns>,
    /// View → Toolbar → Customize..., while open.
    pub(super) toolbar_dlg: Option<crate::toolbar::Customize>,
    /// The last search, for Find Next.
    pub(super) last_find: Option<crate::find::Query>,
    /// A message with OK: its window title and message: "Cannot find …", an export that failed…
    pub(super) notice: Option<(&'static str, String)>,
    /// Why the last save of the download list failed (told once per reason).
    pub(super) save_failed: Option<String>,
    /// Finished files being moved (Move/Rename, Properties → Move): where to, and the answer.
    pub(super) moving: Vec<(u64, PathBuf, MoveDone)>,
    /// Finished torrents whose deferred rename is running asynchronously through `torrent_host`:
    /// where from, whether a folder, and the answer to come.
    #[allow(clippy::type_complexity)]
    pub(super) renaming_torrents: Vec<(
        u64,
        PathBuf,
        bool,
        std::sync::mpsc::Receiver<(Result<PathBuf, String>, Option<String>)>,
    )>,
    /// Finished downloads whose deferred rename is persisted on `Item::rename_on_complete` and
    /// waiting for `torrent_host` (or startup recovery) to run it.
    pub(super) deferred_torrent_queue: Vec<u64>,
    /// Finished downloads' deferred renames, done beside the window (#743): where from, and
    /// the answer to come.
    pub(super) renaming: Vec<(u64, PathBuf, std::sync::mpsc::Receiver<Renamed>)>,
    /// Downloads whose files Redownload is removing beside the window (#743): their next start
    /// waits for the answer (`StartLook`).
    pub(super) clearing: HashMap<u64, std::sync::mpsc::Receiver<()>>,
    /// Files being removed beside the window (#743: Delete with its files, a cancelled or
    /// restarted download's, Overwrite's): a start, or a new download's name, among them waits
    /// for them to go (`remove_beside`).
    pub(super) removing: Vec<Removing>,
    /// Folders of downloads added without Download File Info, looked at beside the window for
    /// the names taken there (`App::number_beside`, #743).
    pub(super) numbering: Vec<std::sync::mpsc::Receiver<Vec<Numbered>>>,
    /// Finished downloads' files looked for beside the window ("File missing", `poll_missing`):
    /// the look under way (one at a time), and when the next one is due.
    pub(super) missing_look: Option<std::sync::mpsc::Receiver<Vec<(u64, PathBuf, bool)>>>,
    pub(super) missing_due: Instant,
    /// File missing (#5): what to do with a finished download whose file Open or Open folder
    /// didn't find (`crate::missing`).
    pub(super) missing_ask: Option<crate::missing::Missing>,
    /// Downloads whose Locate... file chooser is open.
    pub(super) locating: Vec<u64>,
    /// Leftover temporary files looked for beside the window (`poll_leftovers`): at start, and
    /// for Downloads → Clean Up Temporary Files (true: its answer is said).
    pub(super) leftovers: Option<(std::sync::mpsc::Receiver<leftovers::Swept>, bool)>,
    /// The start's sweep is yet to run: only when the list was read whole (`store::Loaded`),
    /// so a list lost or restored from an older copy never takes its downloads' files with it.
    pub(super) sweep_at_start: bool,
    /// Torrents' folders looked at beside the window (#743): a paired device's before it is
    /// added, one through the debrid account for files already there (`poll_torrent_checks`).
    pub(super) torrent_checks: Vec<std::sync::mpsc::Receiver<TorrentChecked>>,
    /// A finished queue asked to turn the computer off: the countdown, cancellable.
    pub(super) turn_off: Option<dialogs::TurnOffCountdown>,
    /// A finished queue asked to exit.
    pub(super) exit_requested: bool,
    /// The global speed cap shared by every download (Speed Limiter settings).
    pub(super) global: std::sync::Arc<mdm_engine::limiter::Limiter>,
    /// Connection caps learned per host, shared by every download.
    pub(super) host_limits: std::sync::Arc<mdm_engine::hosts::HostLimits>,
    /// Scheduler → Download limits: traffic in the window, and what the limit holds back.
    pub(super) quota: crate::quota::Quota,
    /// "Show warning before stopping downloads": the notice, while open.
    pub(super) limit_notice: Option<dialogs::LimitNotice>,
    /// The notification-area icon (created on the first frame), the window hidden in it, and
    /// a real exit under way (closing the window otherwise only hides it, as IDM does).
    pub(super) tray: Option<crate::tray::Tray>,
    pub(super) tray_limiter: Option<bool>,
    /// The tray menu's Pause all (what it paused, for Resume all) and the notifications of
    /// completed downloads waiting to go.
    pub(super) tray_kept: crate::tray::Kept,
    /// When the main window was last seen focused: a tray click steals the focus first
    /// (the taskbar becomes foreground), so a click on a frontmost window reads as
    /// unfocused by the time Toggle runs. Within [`TRAY_TOGGLE_GRACE`] it still counts
    /// as in front.
    pub(super) tray_last_focused: Option<Instant>,
    pub(super) hidden: bool,
    /// Hidden to the tray: when to hand the window's memory back to the system next (#72), and
    /// how many times it was since the window hid.
    pub(super) trim_when_hidden: Option<Instant>,
    pub(super) trims_since_hidden: u32,
    /// Started with the session, the window hidden until the notification area is known
    /// (#226, [`at_login`]): when, when DDM's tray icon was first seen, and when it was last
    /// looked at.
    pub(super) at_login: Option<Instant>,
    pub(super) at_login_icon: Option<Instant>,
    pub(super) at_login_checked: Option<Instant>,
    pub(super) exiting: bool,
    /// Run without a window (`--headless`, #157): nothing is shown or asked; what arrives is
    /// added as the remote control adds it.
    pub(super) headless: bool,
    /// Clipboard watching (Options → General): the text last seen (outer None: not looked
    /// yet).
    pub(super) clip_seen: Option<Option<String>>,
    /// The monitor's first look, asked for when it is turned on, its read off the window
    /// thread (#418).
    pub(super) clip_asked: Option<crate::clipboard::Ask>,
    pub(super) confirm_delete: Option<dialogs::ConfirmDelete>,
    /// Queue right-click → Delete, asking first: the queue and the question.
    pub(super) confirm_queue_delete: Option<(usize, dialogs::Confirm)>,
    /// Redownload, asking first: the download and the question.
    pub(super) confirm_redownload: Option<(u64, dialogs::Confirm)>,
    /// A duplicate address asking what to do: the download already there, what was being added.
    pub(super) duplicate: Option<(u64, Pending, dialogs::Duplicate)>,
    /// Duplicates that came while one was being asked about, next in turn.
    pub(super) duplicates_waiting: std::collections::VecDeque<(u64, Pending, dialogs::Duplicate)>,
    /// The next duplicate check is passed (the user chose to add it anyway).
    pub(super) duplicate_cleared: bool,
    /// New downloads held after the duplicate check for an answer from beside the window
    /// (#743): a finished duplicate's file looked for, or the files of the one replaced being
    /// removed (`poll_duplicates`).
    pub(super) dup_waits: Vec<DupWait>,
    /// Videos being added, their folders read beside the window (#743, `poll_video_adds`).
    pub(super) video_adds: Vec<VideoAdding>,
    /// Playlists being added, their folders read beside the window (#743,
    /// `poll_playlist_adds`): what was chosen, and the names there to come.
    pub(super) playlist_adds: Vec<(
        crate::playlistdlg::PlaylistAdd,
        std::sync::mpsc::Receiver<Vec<String>>,
    )>,
    /// Galleries being added, their folders looked at beside the window (#743,
    /// `poll_gallery_adds`).
    pub(super) gallery_adds: Vec<GalleryAdding>,
    /// Galleries whose start waits for gallery-dl to be looked for beside the window (#743,
    /// `start_gallery`).
    pub(super) gallery_looks: std::collections::HashSet<u64>,
    pub(super) downloads: PathBuf,
    pub(super) dirty: bool,
    /// `secrets::Vault::confirmed` when last seen.
    pub(super) secrets_confirmed: u64,
    pub(super) last_save: Instant,
    /// Writes the list off the window's thread.
    pub(super) saver: store::Saver,
    /// Writes `rss.json` off the window's thread: the feeds' timer saves it by itself (#513).
    pub(super) rss_saver: store::Saver,
    /// When the light/dark mode was last checked against Windows' app mode.
    pub(super) theme_checked: Option<Instant>,
    /// The zoom (View → Interface size, #262) the windows were last drawn at: a change is saved,
    /// and the windows that don't follow by themselves are made to (`follow_zoom`).
    pub(super) zoom: f32,
    /// The main window was brought inside the screen after the start (scale::fit_window).
    pub(super) fitted: bool,
    /// View → Theme's choices (#45), and when the themes folder last changed (read again then).
    pub(super) theme_list: Vec<crate::themefile::Entry>,
    pub(super) themes_changed: Option<std::time::SystemTime>,
    /// pywal's colours were there at the last check (View → Theme → pywal listed then).
    pub(super) pywal_there: bool,
    /// The themes folder and pywal's colours are being looked at beside the window (#743,
    /// `themes_seen`).
    pub(super) themes_looking: bool,
    /// Answers from beside the window for the window to act on (#743, `poll_beside`).
    pub(super) beside: Vec<std::sync::mpsc::Receiver<Beside>>,
    /// Videos whose start waits for DDM's tools folder to be looked at a first time
    /// (`videodlg::found_tools`, #743): their ids, and whether their progress window opens.
    pub(super) tools_wait: Vec<(u64, bool)>,
    /// When `logic`'s every-2-seconds work last ran (crash state, torrents, #174).
    pub(super) ticked: Option<Instant>,
    /// The main window's caption matches the mode.
    pub(super) caption_set: bool,
    /// Windows: whether the main window's title bar is ours (a style theme's, #45), as applied
    /// to its frame; None until the window exists. And the window, once it is.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(super) drawn_frame: Option<bool>,
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(super) main_hwnd: Option<isize>,
    pub(super) list: list::ListState,
    pub(super) icons: ShellIcons,
    /// Open top-level menu (index into `chrome::menus()`).
    pub(super) open_menu: Option<usize>,
    /// Alt is down, and was pressed alone so far (releasing it opens the menu bar).
    pub(super) alt_down: bool,
    pub(super) alt_alone: bool,
    /// The open context / drop-down menu, and its top-left.
    pub(super) popup: Option<(Popup, Pos2, u64)>,
    /// Counts menu opens: each open is a new menu window, even at the same place.
    pub(super) popup_serial: u64,
    /// The column layout last seen, to notice changes that need saving.
    pub(super) last_layout: Option<list::Layout>,
    /// Main download queue (0), synchronization queue (1).
    pub(super) queues: Vec<Queue>,
    /// Failure time and count per download, for queue retries.
    pub(super) failures: HashMap<u64, (Instant, u32)>,
    /// Downloads that failed on the network, to start again by themselves.
    pub(super) auto_resume: crate::autoresume::AutoResume,
    /// Stopped by hand while they could have run: a running queue leaves them alone until
    /// they are started again or the queue is started anew (#341).
    pub(super) stopped_by_hand: std::collections::HashSet<u64>,
    /// Seconds since local midnight at the last scheduler tick (`None` until the clock's first
    /// answer, #536).
    pub(super) clock: Option<u32>,
    /// Scheduler → "Wake the computer to run this queue": the wake set with the system (#142).
    pub(super) wake: crate::wake::Waker,
    /// Commands from ipc.rs (command line, second launch, browser).
    pub(super) commands: std::sync::mpsc::Receiver<ipc::Incoming>,
}

/// A download no Download File Info answered: its address, login and browser context.
pub(super) type Unseen = (url::Url, Option<(String, String)>, ipc::Command);

impl Drop for App {
    fn drop(&mut self) {
        // Let any in-flight deferred rename (a file's beside the window, #743, a torrent folder's)
        // finish and update `item.dest` before saving.
        for (id, from, rx) in std::mem::take(&mut self.renaming) {
            if let Ok(r) = rx.recv_timeout(Duration::from_secs(2)) {
                self.apply_deferred_renamed(id, &from, r, false);
            }
        }
        for (id, from, is_dir, rx) in std::mem::take(&mut self.renaming_torrents) {
            if let Ok((r, sharing_error)) = rx.recv_timeout(Duration::from_secs(2)) {
                self.apply_finished_torrent_rename(id, &from, is_dir, r, sharing_error, false);
            }
        }
        // An archive being extracted stops; its staging folder goes with it.
        self.extracts.stop();
        // Torrents stop with their pieces saved, so the next start goes on from them.
        self.torrent_host.shutdown(&self.rt);
        // No wake for a queue DDM won't be there to start (Linux's alarm would outlive it).
        self.wake.clear();
        self.keep_unseen();
        // The feeds as last handed to their saver; the statistics as they are; the site
        // grabber's projects (a grab under way stopped, to continue at the next start).
        self.rss_saver.flush();
        self.stats.flush();
        self.grab_exit();
        // What the saver has in hand first, then the last state, written here and now.
        self.saver.flush();
        let _ = store::save(
            self.next_id,
            &self.items,
            &self.list.layout(),
            &self.settings,
            &self.queues,
        );
    }
}

impl App {
    /// For a run without a window (#157): downloads go to `downloads` when given (a server's
    /// share), and nothing waits for an answer from someone at the computer.
    pub fn headless(mut self, downloads: Option<PathBuf>) -> Self {
        self.headless = true;
        if let Some(d) = downloads {
            self.downloads = d;
        }
        self
    }

    /// Asked to quit (File → Exit, `/exit`, the Scheduler's "Exit when done").
    pub fn wants_exit(&self) -> bool {
        self.exiting || self.exit_requested
    }

    pub fn new(
        ctx: &egui::Context,
        rt: tokio::runtime::Handle,
        commands: std::sync::mpsc::Receiver<ipc::Incoming>,
        sink: ipc::Sink,
        loaded: store::Loaded,
        start: crate::Start,
    ) -> Self {
        // The look (#45): the chosen theme, in the accent chosen for it, before anything is drawn.
        let first_theme = crate::theme::startup_id(&loaded.settings.theme);
        crate::theme::set_accent(crate::look::accent_of(
            &loaded.settings.theme_looks,
            &first_theme,
        ));
        crate::theme::select(&first_theme, ctx);
        crate::theme::apply(ctx);
        // The zoom main.rs started at (View → Interface size, #262).
        let zoom = crate::scale::startup(loaded.settings.interface_size);
        let sweep_at_start = loaded.whole && !loaded.items.is_empty();
        let mut settings = loaded.settings;
        // DDM ran here before (its first start's questions were answered): an update's start,
        // not a first one, before the installer's language below answers the first.
        let ran_before = settings.language_asked || settings.helpers_asked;
        if settings.limiter_on_startup {
            settings.speed_limit_on = true;
        }
        // The language (D-019): the installer's wizard says which on its first start (it always
        // leaves the value; take it so an upgrade's can't override a later choice); a portable
        // copy writes nothing to the registry. With neither, the first start asks.
        #[cfg(windows)]
        let installer = mdm_ipc::portable_dir()
            .is_none()
            .then(crate::win32::take_install_language)
            .flatten();
        #[cfg(not(windows))]
        let installer: Option<String> = None;
        if !settings.language_asked
            && let Some(code) = installer.filter(|c| crate::i18n::i18n().from_code(c).is_some())
        {
            // Windows' own language is kept as "follow Windows".
            settings.language = (code != crate::i18n::system_code()).then_some(code);
            settings.language_asked = true;
        }
        let ask_language = !settings.language_asked;
        crate::i18n::set_lang(settings.language.as_deref());
        crate::theme::language_fonts(ctx);
        crate::regional::set(&settings.regional);
        if settings.proxy_mode == 2 {
            crate::pacfetch::refresh(&settings.proxy_script, &rt);
        }
        // "Launch on startup" is whatever the Run key says (the installer may have set it); on
        // Linux and macOS, the autostart entry / LaunchAgent.
        // An entry written before #226 (by versions before 0.5.0, or by their installer) starts DDM
        // with its window in front: written again, with the argument that starts it in the
        // notification area. Only an entry for this very program (not one a copy elsewhere
        // wrote), and never by a portable copy, which writes nothing outside its folder.
        #[cfg(windows)]
        {
            settings.launch_on_startup = crate::win32::run_at_startup();
            if settings.launch_on_startup && mdm_ipc::portable_dir().is_none() {
                crate::win32::update_run_at_startup();
            }
        }
        #[cfg(unix)]
        {
            settings.launch_on_startup = crate::unix::run_at_startup();
            if settings.launch_on_startup && mdm_ipc::portable_dir().is_none() {
                crate::unix::update_run_at_startup();
            }
            if mdm_ipc::portable_dir().is_none() && !crate::offscreen() {
                crate::unix::register_browsers();
            }
        }
        let mut queues = loaded.queues;
        if queues.len() < 2 {
            // The built-in queues' names are saved in English and translated where shown
            // (`t(&queue.name)`; a name the user typed is not in the catalog and stays).
            queues = vec![
                Queue::new(crate::i18n::mark("Main download queue"), false),
                Queue::new(crate::i18n::mark("Synchronization queue"), true),
            ];
        }
        for q in &mut queues {
            q.running = q.schedule.start_on_startup;
        }
        // Options → Sounds → "Queue processing started", for a queue that starts with DDM.
        if queues.iter().any(|q| q.running) {
            event_sound(&settings, 2);
        }
        let (mut next_id, mut items) = (loaded.next_id, loaded.items);
        // Downloads started early behind a Download File Info that never got its answer (DDM
        // ended while it was open: a crash, a shutdown): never in the list, they leave no trace.
        // Their files go beside the window (a slow drive holds up no start).
        let unanswered = loaded.unanswered;
        // The list is written again without them at once.
        let cleaned = !unanswered.is_empty();
        if cleaned {
            tracing::info!(
                "{} file(s) of downloads started behind an unanswered Download File Info removed",
                unanswered.len()
            );
            rt.spawn_blocking(move || remove_files(&unanswered));
        }
        // MDM_SYNTHETIC_LIST (developers, a scratch data folder only): a big made-up list.
        let synthetic = crate::framestats::synthetic_wanted(&items);
        if synthetic > 0
            && let Some(data) = mdm_ipc::data_dir()
        {
            items.extend(crate::framestats::synthetic_items(
                next_id,
                synthetic,
                &data.join("synthetic"),
            ));
            next_id += synthetic as u64;
            tracing::info!("MDM_SYNTHETIC_LIST: {synthetic} made-up rows added");
        }
        settings.publish_categories();
        // D-021: torrents are a category of their own (lists from before had them by file
        // type); their files stay where they are.
        for i in &mut items {
            if i.torrent.is_some() {
                i.category = Category::Torrents;
            }
            // A user's category that is gone: General.
            if let Category::Custom(id) = i.category
                && crate::model::custom(id).is_none()
            {
                i.category = Category::General;
            }
        }
        let mut list = list::ListState::default();
        if let Some(cols) = loaded.columns {
            list.apply(cols);
        }
        let downloads = dirs::download_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join("Downloads")))
            .unwrap_or_else(|| PathBuf::from("."));
        // The Categories pane as it was left, and the node shown (if it still exists).
        let [all_open, unfinished_open, finished_open, queues_open] = settings.branches;
        let known = |c: Option<Category>| c.is_none_or(|c| Category::all().contains(&c));
        let grabs = sitegrab::Grabs::load();
        let view = match settings.view {
            Some(View::Queue(q)) if q < queues.len() => View::Queue(q),
            Some(v @ View::Cat(c)) if known(Some(c)) => v,
            Some(v @ (View::Unfinished(c) | View::Finished(c))) if known(c) => v,
            _ => View::All,
        };
        let show_categories = settings.show_categories;
        // The remote control (#141): off unless Options → Remote turned it on.
        let remote = crate::remote::start(&rt, sink, &settings);
        // Started with the session where a window can't be hidden (Wayland, #226): minimized.
        if start == crate::Start::Minimized {
            tracing::info!("started hidden: minimized (a window can't be hidden here)");
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
        let deferred_torrent_queue: Vec<u64> = items
            .iter()
            .filter(|i| i.status == Status::Complete && i.rename_on_complete.is_some())
            .map(|i| i.id)
            .collect();
        // The video helpers in DDM's tools folder, looked at beside the window from now (#743):
        // the browser's quality menu and a video's start find them known.
        let _ = crate::videodlg::found_tools(Some(ctx));
        App {
            rt,
            items,
            next_id,
            selection: Default::default(),
            view,
            show_categories,
            all_open,
            unfinished_open,
            finished_open,
            queues_open,
            maximized: None,
            a11y_lang: "",
            ui_ctx: ctx.clone(),
            tree_keys: false,
            move_rename_for: None,
            move_many_for: Vec::new(),
            list_view: view,
            add_url: None,
            about: None,
            updates: None,
            shortcuts: None,
            tell_friend: None,
            lang_picker: ask_language.then(|| crate::langpick::Picker::new(true, None)),
            shown_existing: None,
            extension: None,
            extension_weighed: false,
            whats_new: None,
            whats_new_weighed: false,
            ran_before,
            auto_update: None,
            raise_updates: false,
            raise_options: false,
            started: Instant::now(),
            update_checked_this_run: None,
            file_info: None,
            file_info_waiting: Default::default(),
            kept_answers: Default::default(),
            progress: Vec::new(),
            complete: Vec::new(),
            virus: std::sync::mpsc::channel(),
            finishing: Vec::new(),
            settings,
            speed_settings: None,
            stats: crate::stats::Recorder::load(),
            speeds: Default::default(),
            statistics: None,
            scheduler: None,
            options: None,
            category_dlg: None,
            category_for_file_info: false,
            grabber: None,
            theme_picker: None,
            grabs,
            assistive_checked: None,
            extracts: Default::default(),
            queue_pick: None,
            drop_target: None,
            drops: Default::default(),
            after_stop: Vec::new(),
            held_finished: Vec::new(),
            released_finished: Vec::new(),
            confirm_category_delete: None,
            properties: Vec::new(),
            checksums: Vec::new(),
            batch: None,
            video_dlg: None,
            video_waiting: Default::default(),
            video_closed: false,
            queue_started: Default::default(),
            lookups: Default::default(),
            gallery_dlg: None,
            torrent_dlg: None,
            torrent_waiting: Default::default(),
            torrent_closed: false,
            metalink_dlg: None,
            metalink_waiting: Default::default(),
            metalink_closed: false,
            torrent_settings: None,
            add_trackers: None,
            torrent_files: None,
            rss: crate::rss::Store::path()
                .map(|p| crate::rss::Store::load(&p))
                .unwrap_or_default(),
            rss_dlg: None,
            rss_busy: Default::default(),
            rss_events: Default::default(),
            torrent_resync: HashMap::new(),
            torrent_idle_since: None,
            accountant: Default::default(),
            account_at: Instant::now(),
            account_saved: Instant::now(),
            torrent_fetched: HashMap::new(),
            torrent_speed: (0.0, Instant::now()),
            torrent_cap: (None, Instant::now()),
            file_speed: (0.0, Instant::now(), 0),
            blocklist_busy: Default::default(),
            torrents_started: false,
            torrent_host: crate::torrents::Host::new(),
            remote,
            remote_torrents: Vec::new(),
            free_space: Default::default(),
            netwatch: Default::default(),
            debrid: Default::default(),
            debrid_from: 0,
            route: mdm_engine::Route::Any,
            adapter_paused: Vec::new(),
            helper_rates: HashMap::new(),
            helper_started: HashMap::new(),
            helper_own: HashMap::new(),
            limit_restarts: Vec::new(),
            find: None,
            columns_dlg: None,
            toolbar_dlg: None,
            last_find: None,
            notice: None,
            save_failed: None,
            moving: Vec::new(),
            renaming_torrents: Vec::new(),
            deferred_torrent_queue,
            renaming: Vec::new(),
            clearing: HashMap::new(),
            removing: Vec::new(),
            numbering: Vec::new(),
            missing_look: None,
            missing_due: Instant::now() + Duration::from_secs(2),
            missing_ask: None,
            locating: Vec::new(),
            leftovers: None,
            sweep_at_start,
            torrent_checks: Vec::new(),
            turn_off: None,
            exit_requested: false,
            global: std::sync::Arc::new(mdm_engine::limiter::Limiter::new(None)),
            host_limits: Default::default(),
            quota: Default::default(),
            limit_notice: None,
            clip_seen: None,
            clip_asked: None,
            tray: None,
            tray_limiter: None,
            tray_kept: Default::default(),
            tray_last_focused: None,
            // Started with the session (#226), or for a download handed over: hidden from the
            // first frame.
            hidden: start == crate::Start::Hidden,
            trim_when_hidden: (start == crate::Start::Hidden)
                .then(|| Instant::now() + next_trim(0)),
            trims_since_hidden: 0,
            at_login: (start == crate::Start::Hidden).then(Instant::now),
            at_login_icon: None,
            at_login_checked: None,
            exiting: false,
            headless: false,
            confirm_delete: None,
            confirm_queue_delete: None,
            confirm_redownload: None,
            duplicate: None,
            duplicates_waiting: Default::default(),
            duplicate_cleared: false,
            dup_waits: Vec::new(),
            video_adds: Vec::new(),
            playlist_adds: Vec::new(),
            gallery_adds: Vec::new(),
            gallery_looks: Default::default(),
            downloads,
            dirty: cleaned,
            secrets_confirmed: 0,
            last_save: Instant::now(),
            saver: store::Saver::new(),
            rss_saver: store::Saver::writing("rss saver", crate::rss::write),
            theme_checked: None,
            zoom,
            fitted: false,
            theme_list: crate::themefile::list(),
            themes_changed: crate::themefile::changed(),
            pywal_there: crate::themefile::pywal_file().is_some_and(|p| p.is_file()),
            themes_looking: false,
            beside: Vec::new(),
            tools_wait: Vec::new(),
            ticked: None,
            caption_set: false,
            // The system's title bar is the frame's own: nothing to apply for it.
            drawn_frame: Some(false),
            main_hwnd: None,
            list,
            icons: ShellIcons,
            open_menu: None,
            alt_down: false,
            alt_alone: false,
            popup: None,
            popup_serial: 0,
            last_layout: None,
            queues,
            failures: HashMap::new(),
            auto_resume: Default::default(),
            stopped_by_hand: Default::default(),
            clock: None,
            wake: Default::default(),
            commands,
        }
    }

    /// Quitting: downloads the browser handed over that never got their Download File Info
    /// (waiting their turn, or arriving at the last moment) go to the main queue as Download
    /// Later, not lost (the browser gave its own copy up). Commands after this get "gone".
    pub(super) fn keep_unseen(&mut self) {
        let mut unseen: Vec<Unseen> = self
            .file_info_waiting
            .drain(..)
            .map(|(new, context, _)| (new.url, new.login, context))
            .collect();
        // The one Download File Info shows, unanswered (an early download is in the list
        // already).
        if let Some(fi) = self.file_info.take()
            && fi.early.is_none()
        {
            unseen.insert(0, (fi.url.clone(), fi.login.clone(), fi.context.clone()));
        }
        while let Ok(inc) = self.commands.try_recv() {
            if inc.reply.is_none()
                && let Some(url) = inc
                    .cmd
                    .url
                    .as_deref()
                    .and_then(|u| url::Url::parse(u.trim()).ok())
                    .filter(mdm_engine::supported)
            {
                unseen.push((url, None, inc.cmd));
            }
        }
        for (url, login, context) in unseen {
            let guessed = context.name.is_none();
            let name = context
                .name
                .clone()
                .unwrap_or_else(|| mdm_engine::naming::file_name(None, &url));
            let name = mdm_engine::naming::sanitize(&name);
            let category = Category::of(&name);
            let dir = self.settings.category_dir(category, &self.downloads);
            let dest = mdm_engine::naming::destination(&dir, &name);
            let size = context.size;
            let id = self.push_item(
                url,
                dest,
                category,
                String::new(),
                size,
                login,
                context,
                crate::i18n::mark("from the browser"),
            );
            self.name_guessed(id, guessed);
            self.queues[0].items.push(id);
        }
        // The channel's receiver goes with it: later commands are answered "gone".
        self.commands = std::sync::mpsc::channel().1;
    }
}
