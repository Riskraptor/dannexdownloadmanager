//! A download as the app holds it (`Download`): started, paused, its snapshot and events, and the
//! state the running connections share (`Shared`).

use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub path: PathBuf,
    pub size: u64,
}

/// A running download.
pub struct Download {
    pub(super) shared: Arc<Shared>,
    pub(super) cancel: CancellationToken,
    pub(super) task: tokio::task::JoinHandle<Result<Outcome, Error>>,
}

impl Download {
    /// Start downloading in the background. Must be called inside a Tokio runtime.
    pub fn start(req: Request, opts: Options) -> Download {
        Download::start_with(req, opts, None)
    }

    /// `start`, connection 1 reading `kept` (a look's open answer, `probe_kept`) instead of
    /// asking again, when it is the same address's and recent (`KEEP_ANSWER_FOR`).
    pub fn start_with(req: Request, opts: Options, kept: Option<KeptAnswer>) -> Download {
        Download::spawn(req.url.clone(), move |shared, cancel| {
            let sh = shared.clone();
            let body = async move {
                if crate::ftp::is_ftp(&req.url) {
                    run_fresh_ftp(req, opts, sh, cancel).await
                } else {
                    run_fresh(req, opts, sh, cancel, kept).await
                }
            };
            run(body, shared)
        })
    }

    /// Continue a paused, cancelled or crashed download from its state file
    /// (`state::state_path(dest)`). `headers` are sent on every request (cookies etc. are not
    /// stored in the state file).
    pub fn resume(
        state_file: PathBuf,
        headers: HeaderMap,
        opts: Options,
    ) -> Result<Download, Error> {
        let saved = load_resume(&state_file)?;
        // Its figure is what is on the disk from the start (#4): it showed 0 until the resume
        // had read its part file, a dip a video's bar and the command line showed at each resume.
        let on_disk = saved.downloaded();
        Ok(Download::spawn_from(
            saved.url.clone(),
            on_disk,
            move |shared, cancel| {
                run(
                    run_resume(saved, state_file, headers, opts, shared.clone(), cancel),
                    shared,
                )
            },
        ))
    }

    /// Start from scratch or continue, as `look` says, `look` run on a blocking thread once the
    /// download is under way (#743): whoever starts it (a window) never waits for the disk, and
    /// has the running download at once to show, count and stop. A state file it names is read
    /// there too (`resume`'s checks). `shown`: the bytes the caller shows for it so far, its
    /// figure until it knows better; a start from scratch begins at 0 once its folder is found,
    /// and one whose folder is missing (a removed drive) keeps it while it says why it can't
    /// start. `kept` serves a start from scratch (`start_with`). Inside a Tokio runtime.
    pub fn begin(
        url: Url,
        headers: HeaderMap,
        opts: Options,
        kept: Option<KeptAnswer>,
        shown: u64,
        look: impl FnOnce() -> Begin + Send + 'static,
    ) -> Download {
        /// What the look found, read beside the download.
        enum Found {
            Resume(Box<ResumeState>, PathBuf),
            /// Where to, and whether its folder is there.
            Fresh(Destination, bool),
        }
        Download::spawn_from(url.clone(), shown, move |shared, cancel| {
            let sh = shared.clone();
            let noted = shared.clone();
            let body = async move {
                let found = tokio::task::spawn_blocking(move || {
                    Ok(match look() {
                        Begin::Resume(state_file) => {
                            let saved = load_resume(&state_file)?;
                            if part_vanished(&saved) {
                                // Its part file was moved or deleted (#5): nothing is left to
                                // continue, and its state alone would fail every resume. Its
                                // state goes and it starts again, saying so.
                                let _ = std::fs::remove_file(&state_file);
                                noted.event(
                                    "Its part file is no longer there (moved or deleted): starting again from the beginning",
                                );
                                let dest = Destination::File(saved.dest.clone());
                                let there = dest.folder().is_dir();
                                Found::Fresh(dest, there)
                            } else {
                                Found::Resume(Box::new(saved), state_file)
                            }
                        }
                        Begin::Fresh(dest) => {
                            let there = dest.folder().is_dir();
                            Found::Fresh(dest, there)
                        }
                    })
                });
                let found = cancellable(&cancel, async {
                    found
                        .await
                        .map_err(|e| Error::Io(Arc::new(io::Error::other(e.to_string()))))?
                })
                .await?;
                match found {
                    Found::Resume(saved, state_file) => {
                        run_resume(*saved, state_file, headers, opts, sh, cancel).await
                    }
                    Found::Fresh(dest, there) => {
                        if there {
                            sh.view.lock().unwrap().downloaded = 0;
                        }
                        let req = Request { url, headers, dest };
                        if crate::ftp::is_ftp(&req.url) {
                            run_fresh_ftp(req, opts, sh, cancel).await
                        } else {
                            run_fresh(req, opts, sh, cancel, kept).await
                        }
                    }
                }
            };
            run(body, shared)
        })
    }

    pub(super) fn spawn<F, Fut>(url: Url, f: F) -> Download
    where
        F: FnOnce(Arc<Shared>, CancellationToken) -> Fut,
        Fut: Future<Output = Result<Outcome, Error>> + Send + 'static,
    {
        Download::spawn_from(url, 0, f)
    }

    /// `spawn`, its figure `downloaded` until the download says otherwise.
    pub(super) fn spawn_from<F, Fut>(url: Url, downloaded: u64, f: F) -> Download
    where
        F: FnOnce(Arc<Shared>, CancellationToken) -> Fut,
        Fut: Future<Output = Result<Outcome, Error>> + Send + 'static,
    {
        let cancel = CancellationToken::new();
        let shared = Arc::new(Shared {
            view: Mutex::new(Snapshot {
                state: State::Probing,
                url,
                file: None,
                size: None,
                downloaded,
                resumable: None,
                segments: Vec::new(),
                connections: Vec::new(),
                error: None,
                failure: None,
                events: Vec::new(),
                server: None,
                at_service: None,
            }),
            seen: Default::default(),
            planner: Mutex::new(None),
            limiter: Limiter::new(None),
            best_speed: Default::default(),
            flushing: Default::default(),
            held: Default::default(),
            got: Default::default(),
            range_ttfb: std::sync::atomic::AtomicU64::new(u64::MAX),
            own_limits: Default::default(),
            partial_seen: Default::default(),
        });
        let task = tokio::spawn(f(shared.clone(), cancel.clone()));
        Download {
            shared,
            cancel,
            task,
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        self.shared.snapshot()
    }

    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// Change this download's speed limit (bytes/s, None = unlimited) while it runs.
    pub fn set_speed_limit(&self, rate: Option<u64>) {
        self.shared.limiter.set(rate);
    }

    pub fn speed_limit(&self) -> Option<u64> {
        self.shared.limiter.get()
    }

    pub async fn wait(self) -> Result<Outcome, Error> {
        match self.task.await {
            Ok(r) => r,
            Err(e) if e.is_cancelled() => Err(Error::Cancelled),
            Err(e) => std::panic::resume_unwind(e.into_panic()),
        }
    }
}

pub(super) struct Shared {
    pub(super) view: Mutex<Snapshot>,
    /// What the download's client learnt of its server (#140); the look's when it went on
    /// from the look's answer.
    pub(super) seen: std::sync::OnceLock<Arc<crate::server::ServerSeen>>,
    pub(super) planner: Mutex<Option<Planner>>,
    pub(super) limiter: Limiter,
    /// The best speed one connection of this download sustained (≥ 1 s), bytes/s: what a good
    /// connection to this server does, the yardstick for shedding.
    pub(super) best_speed: std::sync::atomic::AtomicU64,
    /// The resume save is flushing the part file to the disk right now (`save_state`): the
    /// connections hold what they receive (`hold_cap`).
    pub(super) flushing: std::sync::atomic::AtomicBool,
    /// Bytes received and claimed but not yet written, in the connections' buffers: shown as
    /// downloaded (they are), but never saved as such.
    pub(super) held: std::sync::atomic::AtomicU64,
    /// Bytes every connection received (claimed) so far: a silent connection is stalled only
    /// if the others got enough after it went quiet (`stalled`).
    pub(super) got: std::sync::atomic::AtomicU64,
    /// The quickest answer a range request got (ms from sending it to its headers), u64::MAX
    /// before one: what a split costs before its first byte (`split_floor`).
    pub(super) range_ttfb: std::sync::atomic::AtomicU64,
    /// A range request of this run got its part (206), on any connection: a resume whose server
    /// sent none ([`run_resume`]) can't be continued.
    pub(super) partial_seen: std::sync::atomic::AtomicBool,
    /// The connection caps this download learns when its options share no table
    /// (`Options::host_limits` None: the command line, DDM's own fetches): a server's refusals
    /// still retire the connections it won't take, instead of each retrying them.
    pub(super) own_limits: crate::hosts::HostLimits,
}

impl Shared {
    pub(super) fn snapshot(&self) -> Snapshot {
        let mut s = self.view.lock().unwrap().clone();
        s.server = self.seen.get().and_then(|seen| seen.get());
        if let Some(p) = self.planner.lock().unwrap().as_ref() {
            s.downloaded = p.downloaded() + self.held.load(Ordering::Relaxed);
            s.segments = p
                .segments()
                .iter()
                .map(|g| SegmentView {
                    start: g.start,
                    cursor: g.cursor,
                    end: g.end,
                })
                .collect();
        }
        s
    }

    pub(super) fn set_state(&self, state: State) {
        self.view.lock().unwrap().state = state;
    }

    /// Add to the event log.
    pub(super) fn event(&self, text: impl Into<String>) {
        let mut v = self.view.lock().unwrap();
        let seq = v.events.last().map_or(1, |e| e.seq + 1);
        if v.events.len() >= EVENTS {
            v.events.remove(0);
        }
        v.events.push(Event {
            seq,
            at: std::time::SystemTime::now(),
            text: text.into(),
        });
    }

    pub(super) fn conn(&self, id: WorkerId, f: impl FnOnce(&mut ConnView)) {
        let mut v = self.view.lock().unwrap();
        let i = match v.connections.iter().position(|c| c.id == id) {
            Some(i) => i,
            None => {
                v.connections.push(ConnView {
                    id,
                    downloaded: 0,
                    status: ConnStatus::Connecting,
                    speed: 0,
                });
                v.connections.len() - 1
            }
        };
        f(&mut v.connections[i]);
    }

    pub(super) fn with_planner<T>(&self, f: impl FnOnce(&mut Planner) -> T) -> T {
        f(self
            .planner
            .lock()
            .unwrap()
            .as_mut()
            .expect("planner exists while workers run"))
    }
}

/// Download `url` to `path` and wait for it: DDM's own downloads that aren't in the list (its
/// helpers, its updates, a crash report's symbols) use the engine like every download. An
/// earlier attempt cut off at `path` resumes; `progress(done, total)` while it runs.
pub async fn fetch(
    url: Url,
    path: &Path,
    opts: Options,
    progress: impl Fn(u64, Option<u64>),
) -> Result<Outcome, Error> {
    // An earlier attempt's files under their former names (`.mdm.*`) resume as today's.
    crate::state::migrate_legacy(path);
    let state = crate::state::state_path(path);
    let fresh = || {
        Download::start(
            Request {
                url: url.clone(),
                headers: HeaderMap::new(),
                dest: Destination::File(path.to_owned()),
            },
            opts.clone(),
        )
    };
    // A saved state for another address (an earlier release's file) is not resumed.
    let dl = match crate::state::ResumeState::load(&state) {
        Ok(saved) if saved.url == url => {
            Download::resume(state, HeaderMap::new(), opts.clone()).unwrap_or_else(|_| fresh())
        }
        _ => fresh(),
    };
    loop {
        let s = dl.snapshot();
        progress(s.downloaded, s.size);
        if matches!(s.state, State::Done | State::Failed | State::Cancelled) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    dl.wait().await
}
