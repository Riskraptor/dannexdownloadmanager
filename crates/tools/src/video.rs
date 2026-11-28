//! A video download run by yt-dlp under supervision: our file name, progress read from a fixed
//! `--progress-template`, the final path from `--print after_move`. Stopping kills it; starting
//! again continues (`--continue`). Each run takes the page as looked up (`-J`), points links at
//! servers that answer ([`crate::servers`]) and downloads from that list (`--load-info-json`),
//! so a run reads the page once. A run looks the page up afresh, except the first run of a
//! download chosen in the browser's quality menu, which takes the look-up the menu was made from
//! ([`Preloaded`]): the page is then read once for the menu and the download together.
//!
//! The bytes come down through DDM's engine (#769, [`crate::streams`]): yt-dlp chooses the
//! formats, the engine downloads each stream into the file yt-dlp looks for, then yt-dlp merges
//! them, saves the subtitles and the metadata as after its own download. What the engine can't
//! download (a live stream, RTMP…) yt-dlp downloads itself, as before.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::proc::{self, ProcError};
use crate::servers;

mod by_engine;
mod files;
mod lookup;
mod meter;
mod subtitles;
mod supervise;

pub use self::files::*;
pub use self::lookup::*;
pub use self::subtitles::*;
pub use self::supervise::*;

// What only the parts share (each takes it, and this module's imports, with `use super::*`).
use self::by_engine::*;
use self::meter::*;

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Running,
    Done(PathBuf),
    Failed(String),
    Stopped,
}

#[derive(Clone, Debug)]
pub struct Progress {
    pub state: State,
    pub downloaded: u64,
    pub total: Option<u64>,
    /// Whether `total` is the real size (#2): `Some(true)` once every stream's size is the site's
    /// own count (or the saved file's, at the end), `Some(false)` while any is an estimate,
    /// `None` until the run knows (the size the choice said stands until then).
    pub exact: Option<bool>,
    /// Bytes/s.
    pub speed: f64,
    /// Notes for the download's log, oldest first (a server swapped for another).
    pub events: Vec<(std::time::SystemTime, String)>,
    /// Saved beside the video once it is done: its subtitle files (#68).
    pub side_files: Vec<PathBuf>,
    /// Subtitles that didn't come with the video, once it is done (#68): the video is saved all
    /// the same.
    pub sub_problems: Vec<SubProblem>,
    /// While DDM's engine downloads the streams (#769): its segments, connections and server.
    pub engine: Option<crate::streams::View>,
    /// Why yt-dlp downloads this one itself (#769), once a run has found out.
    pub helper: Option<crate::streams::Helper>,
    /// The file the run goes to (`dir/<stem>.<ext>`), once [`VideoJob::begin`]'s look on the
    /// job's own thread has said it (#743); None from `start_with`, whose place was given.
    pub place: Option<PathBuf>,
}

pub struct VideoJob {
    /// yt-dlp once the page is looked up (`None` before).
    child: Arc<Mutex<Option<proc::Spawned>>>,
    progress: Arc<Mutex<Progress>>,
    /// The engine's download of a stream, while one is under way (#769).
    current: Arc<crate::streams::Current>,
}

impl VideoJob {
    /// Download `url` in format `selector` to `dir/<stem>.<ext>`; when yt-dlp downloads it by
    /// itself, at most `rate` bytes/s ([`limit_args`]). `ffmpeg` (its exe) merges separate video and audio into an MP4. `expected`: the size the
    /// choice was shown with (every stream), so the progress starts from the whole. `browser`:
    /// what the browser's request for the page carried (proc::FromBrowser). `subs`: the
    /// subtitles to save with it (#68; none for a sound alone, which has no picture to put them
    /// on). `engine`: DDM's engine, which downloads the streams yt-dlp finds (#769; its options'
    /// speed limit is the download's); without it, or for what it can't download, yt-dlp does.
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        exe: &Path,
        url: &str,
        selector: &str,
        dir: &Path,
        stem: &str,
        rate: Option<u64>,
        ffmpeg: Option<&Path>,
        expected: Option<u64>,
        browser: &proc::FromBrowser,
        subs: &Subs,
        engine: Option<crate::streams::Engine>,
    ) -> Result<VideoJob, ProcError> {
        Self::start_with(
            exe, url, selector, dir, stem, rate, ffmpeg, expected, browser, subs, None, engine,
        )
    }

    /// [`VideoJob::start`], the first run taking the quality menu's look-up (`preloaded`) when
    /// it is for this very choice.
    #[allow(clippy::too_many_arguments)]
    pub fn start_with(
        exe: &Path,
        url: &str,
        selector: &str,
        dir: &Path,
        stem: &str,
        rate: Option<u64>,
        ffmpeg: Option<&Path>,
        expected: Option<u64>,
        browser: &proc::FromBrowser,
        subs: &Subs,
        preloaded: Option<Preloaded>,
        engine: Option<crate::streams::Engine>,
    ) -> Result<VideoJob, ProcError> {
        proc::safe(url)?;
        let preloaded = preloaded.filter(|p| p.selector == selector);
        let (theirs, cookies) = browser.args(url)?;
        let plan = Plan::new(
            selector,
            dir,
            stem,
            rate,
            ffmpeg,
            expected,
            subs,
            &theirs,
            engine.is_some(),
        )?;
        let job = VideoJob::running(cookies.is_none(), engine.as_ref());
        let (exe, url) = (exe.to_owned(), url.to_owned());
        let by_engine = engine.map(|engine| ByEngine {
            engine,
            choose: plan.choose.clone(),
            current: job.current.clone(),
        });
        let (progress, child) = (job.progress.clone(), job.child.clone());
        std::thread::spawn(move || {
            // The cookie file lives as long as the run.
            let _cookies = cookies;
            run_job(
                &exe, &url, &plan, &theirs, &progress, &child, preloaded, by_engine,
            );
        });
        Ok(job)
    }

    /// [`VideoJob::start_with`] where the file it goes to (`place`: `dir/<stem>.<ext>`) is the
    /// caller's look at the disk, run on the job's own thread as it starts, and its folder made
    /// there (#743: a sleeping hard disk, a USB drive or a network folder can take seconds to
    /// answer). The job runs from now (shown, stopped by [`VideoJob::stop`]); where it went is
    /// [`Progress::place`] once known. What is made from the place, and found wrong, ends the
    /// run with the reason ([`State::Failed`]), as `start_with` would have refused it.
    #[allow(clippy::too_many_arguments)]
    pub fn begin(
        exe: &Path,
        url: &str,
        selector: &str,
        place: impl FnOnce() -> PathBuf + Send + 'static,
        rate: Option<u64>,
        ffmpeg: Option<&Path>,
        expected: Option<u64>,
        browser: &proc::FromBrowser,
        subs: &Subs,
        preloaded: Option<Preloaded>,
        engine: Option<crate::streams::Engine>,
    ) -> Result<VideoJob, ProcError> {
        proc::safe(url)?;
        let preloaded = preloaded.filter(|p| p.selector == selector);
        // The browser's cookie file is written on the job's own thread too (#743).
        let prepared = browser.prepare(url)?;
        let job = VideoJob::running(!prepared.has_cookies(), engine.as_ref());
        let (exe, url, selector) = (exe.to_owned(), url.to_owned(), selector.to_owned());
        let (ffmpeg, subs) = (ffmpeg.map(Path::to_owned), subs.clone());
        let (progress, child, current) =
            (job.progress.clone(), job.child.clone(), job.current.clone());
        std::thread::spawn(move || {
            // Kept as long as the run.
            let (theirs, _cookies) = match prepared.write() {
                Ok(written) => written,
                Err(e) => {
                    progress.lock().unwrap_or_else(|p| p.into_inner()).state =
                        State::Failed(e.to_string());
                    return;
                }
            };
            // A panic in the look must not leave the download "running" forever either (#79).
            let Ok(dest) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(place)) else {
                progress.lock().unwrap_or_else(|p| p.into_inner()).state =
                    State::Failed("the video download stopped on an internal error".into());
                return;
            };
            if progress.lock().unwrap_or_else(|p| p.into_inner()).state != State::Running {
                // Stopped while its place was looked for: nothing started, nothing written.
                return;
            }
            let dir = dest.parent().map(Path::to_owned).unwrap_or_default();
            let stem = dest
                .file_stem()
                .map_or_else(|| "video".to_owned(), |s| s.to_string_lossy().into_owned());
            progress.lock().unwrap_or_else(|p| p.into_inner()).place = Some(dest);
            let _ = std::fs::create_dir_all(&dir);
            let plan = match Plan::new(
                &selector,
                &dir,
                &stem,
                rate,
                ffmpeg.as_deref(),
                expected,
                &subs,
                &theirs,
                engine.is_some(),
            ) {
                Ok(plan) => plan,
                Err(e) => {
                    progress.lock().unwrap_or_else(|p| p.into_inner()).state =
                        State::Failed(e.to_string());
                    return;
                }
            };
            let by_engine = engine.map(|engine| ByEngine {
                engine,
                choose: plan.choose.clone(),
                current,
            });
            run_job(
                &exe, &url, &plan, &theirs, &progress, &child, preloaded, by_engine,
            );
        });
        Ok(job)
    }

    /// A job just started: running, nothing come yet; the log says whose cookies yt-dlp reads
    /// when the browser gave it none (`given_none`, #229).
    fn running(given_none: bool, engine: Option<&crate::streams::Engine>) -> VideoJob {
        // Whose cookies yt-dlp has, for the log (#229): the browser's name only, never a
        // cookie or the profile (a folder may name the person).
        let events = match proc::browser_cookies() {
            Some(b) if given_none => vec![(
                std::time::SystemTime::now(),
                format!(
                    "Cookies read from {} on this computer (Options → Sites Logins)",
                    b.browser
                ),
            )],
            _ => Vec::new(),
        };
        VideoJob {
            child: Arc::new(Mutex::new(None)),
            progress: Arc::new(Mutex::new(Progress {
                state: State::Running,
                downloaded: 0,
                total: None,
                exact: None,
                speed: 0.0,
                events,
                side_files: Vec::new(),
                sub_problems: Vec::new(),
                engine: None,
                helper: None,
                place: None,
            })),
            current: Arc::new(crate::streams::Current::new(
                engine.and_then(|e| e.opts.speed_limit),
            )),
        }
    }

    /// Its progress. Stopped only once the engine's download has saved where it stopped: a
    /// start again before then would find the stream still held.
    pub fn progress(&self) -> Progress {
        let mut p = self.progress.lock().unwrap().clone();
        if p.state == State::Stopped && self.current.busy() {
            p.state = State::Running;
            p.speed = 0.0;
        }
        p
    }

    /// Whether its total is the real size ([`Progress::exact`]), read without copying its
    /// progress.
    pub fn size_exact(&self) -> Option<bool> {
        self.progress.lock().unwrap().exact
    }

    /// Stop (the partial file stays; starting again continues it).
    pub fn stop(&self) {
        self.progress.lock().unwrap().state = State::Stopped;
        self.current.cancel();
        if let Some(s) = self.child.lock().unwrap().as_mut() {
            s.kill();
        }
    }

    /// A new speed limit (bytes/s): the engine's download takes it at once (#769). yt-dlp
    /// downloading by itself took its limit as it started.
    pub fn set_speed_limit(&self, rate: Option<u64>) {
        self.current.set_limit(rate);
    }

    pub fn speed_limit(&self) -> Option<u64> {
        self.current.limit()
    }

    /// Why yt-dlp downloads this one itself (#769: a live stream, RTMP…), as far as known: a new
    /// speed limit then takes starting it again.
    pub fn helper(&self) -> Option<crate::streams::Helper> {
        self.progress.lock().unwrap().helper.clone()
    }
}

#[cfg(test)]
mod tests;
