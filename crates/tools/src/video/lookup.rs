//! Looking a page up (`-J`): the answer kept for a moment (`Kept`), shared with the window that
//! waits for it (`InfoSlot`), a quality menu's look-up handed to its download (`Preloaded`), and
//! links moved off servers that don't answer.

use super::*;

/// How long the browser's quality menu's look-up may take: a stream's playlists
/// (`crate::manifest`, 5 s), yt-dlp's look-up and its second one without the cookies
/// (`proc::look_up`, 90 s each), and a little more. The download chosen there waits for it that
/// long at most, and a second click shares a look-up still running that long (the app's
/// videolookup.rs).
pub const LOOKUP_WAIT: Duration = Duration::from_secs(
    crate::manifest::BUDGET.as_secs() + 2 * crate::resolve::LOOK_UP_TIMEOUT.as_secs() + 5,
);

/// The largest look-up kept for the download (a page's is a few hundred KB, YouTube's a few MB):
/// a bigger one isn't kept, and the download looks the page up itself.
pub const INFO_MAX: usize = 8 * 1024 * 1024;

/// All the look-ups kept at once, together: past it, the oldest are let go (their downloads look
/// the page up themselves). Twenty look-ups are kept for ten minutes (videolookup.rs), and
/// [`INFO_MAX`] each would be 160 MB.
pub const INFO_TOTAL: usize = 32 * 1024 * 1024;

/// The look-ups' answers kept, oldest first, with their sizes: what [`INFO_TOTAL`] counts. A
/// look-up no longer kept anywhere (its slot dropped) counts no more.
pub(super) struct Kept {
    pub(super) limit: usize,
    pub(super) answers: Mutex<std::collections::VecDeque<(std::sync::Weak<SlotInner>, usize)>>,
}

impl Kept {
    pub(super) fn new(limit: usize) -> Arc<Kept> {
        Arc::new(Kept {
            limit,
            answers: Mutex::default(),
        })
    }

    /// Every look-up's: the process's own pool.
    pub(super) fn shared() -> Arc<Kept> {
        static POOL: std::sync::LazyLock<Arc<Kept>> =
            std::sync::LazyLock::new(|| Kept::new(INFO_TOTAL));
        POOL.clone()
    }

    /// `slot` keeps an answer of `len` bytes now: the oldest others are let go while all of
    /// them together are more than the limit.
    pub(super) fn add(&self, slot: &Arc<SlotInner>, len: usize) {
        let mut answers = self.answers.lock().unwrap_or_else(|e| e.into_inner());
        answers.retain(|(s, _)| s.strong_count() > 0);
        answers.push_back((Arc::downgrade(slot), len));
        let mut total: usize = answers.iter().map(|(_, n)| n).sum();
        while total > self.limit && answers.len() > 1 {
            let Some((oldest, n)) = answers.pop_front() else {
                break;
            };
            total -= n;
            if let Some(s) = oldest.upgrade() {
                s.let_go();
            }
        }
    }
}

/// yt-dlp's look-up (`-J`) of a page for the browser's quality menu, kept for the download
/// chosen from it: being looked up, done (the answer as yt-dlp gave it, and whether it took
/// looking the page up without cookies), or failed. In memory only, never written or logged
/// (its links are signed): its `Debug` shows nothing of it.
#[derive(Clone)]
pub struct InfoSlot(pub(super) Arc<SlotInner>);

pub(super) struct SlotInner {
    pub(super) info: Mutex<Info>,
    pub(super) changed: Condvar,
    pub(super) born: Instant,
    /// Where its answer counts ([`INFO_TOTAL`]).
    pub(super) pool: Arc<Kept>,
}

impl SlotInner {
    /// Its answer dropped to make room: as if the look-up had failed.
    pub(super) fn let_go(&self) {
        let mut info = self.info.lock().unwrap_or_else(|e| e.into_inner());
        if matches!(*info, Info::Ready(..)) {
            *info = Info::Failed;
            self.changed.notify_all();
        }
    }
}

pub(super) enum Info {
    Pending,
    Ready(Arc<Vec<u8>>, bool),
    Failed,
}

impl std::fmt::Debug for InfoSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("InfoSlot")
    }
}

impl InfoSlot {
    /// A look-up begun at `born`.
    pub fn new(born: Instant) -> Self {
        Self::counted_in(born, Kept::shared())
    }

    pub(super) fn counted_in(born: Instant, pool: Arc<Kept>) -> Self {
        InfoSlot(Arc::new(SlotInner {
            info: Mutex::new(Info::Pending),
            changed: Condvar::new(),
            born,
            pool,
        }))
    }

    pub fn born(&self) -> Instant {
        self.0.born
    }

    pub(super) fn info(&self) -> std::sync::MutexGuard<'_, Info> {
        self.0.info.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// yt-dlp answered with `json` (`without_cookies`: as `proc::LookedUp` says). Bigger than
    /// [`INFO_MAX`]: as if it had failed; the oldest others are let go past [`INFO_TOTAL`]. Only
    /// a look-up still pending takes it.
    pub fn fill(&self, json: Vec<u8>, without_cookies: bool) {
        let len = json.len();
        {
            let mut info = self.info();
            if !matches!(*info, Info::Pending) {
                return;
            }
            *info = if len > INFO_MAX {
                Info::Failed
            } else {
                Info::Ready(Arc::new(json), without_cookies)
            };
            self.0.changed.notify_all();
        }
        // Not under its own lock: letting another go locks that one.
        if len <= INFO_MAX {
            self.0.pool.add(&self.0, len);
        }
    }

    /// The look-up failed (or ended without an answer): the download looks the page up itself.
    pub fn fail(&self) {
        let mut info = self.info();
        if matches!(*info, Info::Pending) {
            *info = Info::Failed;
            self.0.changed.notify_all();
        }
    }

    pub fn pending(&self) -> bool {
        matches!(*self.info(), Info::Pending)
    }

    /// How many hold this look-up (the menu's, the download chosen from it): a check that a
    /// choice let go of it.
    pub fn holders(&self) -> usize {
        Arc::strong_count(&self.0)
    }

    /// The answer once there, waiting until `until` at most, or until `stopped` says so.
    pub(super) fn wait(
        &self,
        until: Instant,
        stopped: impl Fn() -> bool,
    ) -> Option<(Arc<Vec<u8>>, bool)> {
        let mut info = self.info();
        loop {
            match &*info {
                Info::Ready(json, without) => return Some((json.clone(), *without)),
                Info::Failed => return None,
                Info::Pending => {}
            }
            let now = Instant::now();
            if now >= until || stopped() {
                return None;
            }
            // A tenth of a second at a time: a Stop meanwhile is seen at once.
            let step = (until - now).min(Duration::from_millis(100));
            info = self
                .0
                .changed
                .wait_timeout(info, step)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }
}

/// The browser's quality menu's look-up for the download chosen there (its first run only):
/// the look-up, and the choice as the menu offered it (DDM's selector, its label, untranslated,
/// and whether the menu's list had merged qualities). The page yt-dlp read must offer that very
/// choice, or the run looks the page up itself.
pub struct Preloaded {
    pub slot: InfoSlot,
    pub selector: String,
    pub label: String,
    pub merge: bool,
}

/// How a run tells whether a media server can be reached and serves a stream, in place of
/// `servers::reachable` and `servers::Prober` (which ask the servers): set by tests only
/// (tests/fake_ytdlp.rs), whose servers must never be asked.
pub(super) type ServerChecks = (fn(&str) -> bool, fn(&str) -> bool);

pub(super) static SERVER_CHECKS: std::sync::RwLock<Option<ServerChecks>> =
    std::sync::RwLock::new(None);

/// Tests only: media servers `alive` says are reachable, serving every stream `serves` says.
#[doc(hidden)]
pub fn check_servers_with(alive: fn(&str) -> bool, serves: fn(&str) -> bool) {
    *SERVER_CHECKS.write().unwrap_or_else(|e| e.into_inner()) = Some((alive, serves));
}

/// yt-dlp's look-up `json` as a run's page, when it offers `selector` under `label` among its
/// choices (`resolve::choices` with `merge`): the menu offered that very choice from it.
pub(super) fn offered(
    json: &[u8],
    selector: &str,
    label: &str,
    merge: bool,
) -> Option<serde_json::Value> {
    let video = crate::resolve::parse(json)?;
    crate::resolve::choices(&video, merge)
        .iter()
        .any(|c| c.selector == selector && c.label == label)
        .then(|| serde_json::from_slice(json).ok())
        .flatten()
}

/// A private file for the looked-up page (its links are signed; removed when the run ends).
pub(super) fn info_file() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "ddm-video-{}-{}.json",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ))
}

/// The page's video information from yt-dlp (one video, not a playlist), asked as the browser
/// asked for the page (`browser`: its Referer, User-Agent and cookies, `FromBrowser::args`).
/// Without them (#229) a page that wants a sign-in, YouTube's "confirm you're not a bot", was
/// refused here although the video window's look-up, which had them, had listed its qualities.
pub(super) fn look_up(
    exe: &Path,
    url: &str,
    browser: &[String],
) -> Result<(serde_json::Value, bool), String> {
    let mut args: Vec<String> = ["-J", "--no-playlist", "--no-warnings"]
        .map(String::from)
        .to_vec();
    args.extend(browser.iter().cloned());
    let out = proc::look_up(exe, &args, url, std::time::Duration::from_secs(90))
        .map_err(|e| e.to_string())?;
    let info = serde_json::from_slice(&out.json)
        .map_err(|_| "yt-dlp gave no video information".to_owned())?;
    Ok((info, out.without_cookies))
}

/// The page as the quality menu's look-up read it (`preloaded`), once yt-dlp has answered there
/// (waiting for it as long as that look-up may take, [`LOOKUP_WAIT`]) and if it offers the choice
/// the menu showed; else none, and the run looks the page up itself.
pub(super) fn preloaded_page(
    preloaded: &Preloaded,
    progress: &Mutex<Progress>,
) -> Option<(serde_json::Value, bool)> {
    let until = preloaded.slot.born() + LOOKUP_WAIT;
    let stopped = || progress.lock().unwrap().state == State::Stopped;
    let (json, without_cookies) = preloaded.slot.wait(until, stopped)?;
    let info = offered(
        &json,
        &preloaded.selector,
        &preloaded.label,
        preloaded.merge,
    )?;
    Some((info, without_cookies))
}

/// Move `info`'s links off servers that can't be reached (and off `dead`, the ones an earlier run
/// couldn't reach), when `reroute`.
pub(super) fn moved(
    info: &mut serde_json::Value,
    dead: &std::collections::BTreeSet<String>,
    reroute: bool,
) -> servers::Rerouted {
    let checks = *SERVER_CHECKS.read().unwrap_or_else(|e| e.into_inner());
    if let (true, Some((alive, serves))) = (reroute, checks) {
        servers::reroute(info, |h| !dead.contains(h) && alive(h), |l, _| serves(l))
    } else if reroute {
        let prober = servers::Prober::new();
        servers::reroute(
            info,
            |h| !dead.contains(h) && servers::reachable(h),
            |l, h| prober.serves(l, h),
        )
    } else {
        servers::Rerouted::default()
    }
}

/// Format `id` on the page as yt-dlp gives it today (its address, headers, pieces): looked up
/// again (as the browser asked, `browser`), its servers moved as the run moved them, and chosen
/// with `-f <id>`.
pub(super) fn relink(
    exe: &Path,
    url: &str,
    browser: &[String],
    choose_args: &[String],
    dead: &std::collections::BTreeSet<String>,
    reroute: bool,
    id: &str,
) -> Result<serde_json::Value, String> {
    let (mut info, _) = look_up(exe, url, browser)?;
    moved(&mut info, dead, reroute);
    let file = info_file();
    std::fs::write(&file, info.to_string())
        .map_err(|e| format!("Cannot write the video information: {e}"))?;
    let _cleanup = Remove(file.clone());
    let chosen = choose(exe, &with_selector(choose_args, id), &file)?;
    crate::streams::format_of(&chosen, id)
        .cloned()
        .ok_or_else(|| format!("the page no longer offers format {id}"))
}
