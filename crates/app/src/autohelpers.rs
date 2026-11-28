//! The video helpers come with DDM, never asked for (the owner's decision, 2026-11-27): yt-dlp,
//! and ffmpeg where DDM installs it (Windows), as `mdm_tools::needed` names them.
//!
//! - **The Windows setup** gets them as it installs: it runs the installed program with
//!   `--install-helpers` ([`setup_main`]), which downloads and checks them with the same code as
//!   everywhere else (`mdm_tools::needed::install`: yt-dlp's checksum list signed by its
//!   developers' key, ffmpeg's published checksums) and reports how far it is on its standard
//!   output, line by line, for the setup's progress bar ([`Line`]). Offline, or a download that
//!   fails or stalls: the setup finishes anyway, and DDM gets them itself.
//! - **DDM** gets whatever is still missing whenever it runs ([`tick`]): a portable copy's or a
//!   Linux or macOS package's first start, a setup that was offline, a helper deleted. Quietly,
//!   in the background: at once (once the network is there), again after a failure with a
//!   growing wait, and at once when the network comes back (the adapter watch, `netwatch`). A
//!   video asked for meanwhile waits and starts by itself once yt-dlp is there (app.rs
//!   `start_video`, [`video_waits`]).
//!
//! What a person sees of it is generic (the owner's wish, 2026-11-27: a modern app neither names
//! nor explains its parts): "Getting ready..." in the status strip while one downloads
//! ([`note`]), nothing once it has come; the helpers' names only in Helpers..., opened on purpose,
//! and in the log. The person's own programs (Helpers..., `mdm_tools::chosen`) and, on Linux and
//! macOS, the system's count as there: nothing is downloaded over them.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use mdm_tools::chosen::Helper;
use mdm_tools::update::Progress;

/// The first wait after a failure; each one after doubles it, up to [`WAIT_AT_MOST`].
const FIRST_WAIT: Duration = Duration::from_secs(15);
const WAIT_AT_MOST: Duration = Duration::from_secs(10 * 60);
/// How often the background looks at the network while it waits.
const LOOK_EVERY: Duration = Duration::from_secs(1);

/// The background's state, for the windows that show it.
#[derive(Default)]
struct Shared {
    /// The background task is running (something was missing when it started).
    running: bool,
    /// The helper downloading now, and how far it is.
    current: Option<(Helper, Arc<Progress>)>,
    /// The last failure (none since a helper came): which helper, and why, in words.
    failed: Option<(Helper, String)>,
    /// The main window, repainted when this changes.
    ctx: Option<egui::Context>,
}

static SHARED: LazyLock<Mutex<Shared>> = LazyLock::new(Default::default);
/// "Try now": the video window's Install button, while the background waits.
static NUDGE: LazyLock<tokio::sync::Notify> = LazyLock::new(tokio::sync::Notify::new);

fn shared() -> std::sync::MutexGuard<'static, Shared> {
    SHARED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Change the state and repaint the main window (asked once the state is let go: egui's own
/// lock is never taken while this one is held).
fn set(change: impl FnOnce(&mut Shared)) {
    let ctx = {
        let mut s = shared();
        change(&mut s);
        s.ctx.clone()
    };
    if let Some(ctx) = ctx {
        ctx.request_repaint();
    }
}

/// At every pass of the app's clock: when the helpers as last found (`videodlg::found_tools`,
/// read beside the window) miss one, the background starts getting it, unless it already is.
pub fn tick(ctx: &egui::Context, rt: &tokio::runtime::Handle) {
    let Some((Some(dir), ytdlp, ffmpeg)) = crate::videodlg::found_tools(None) else {
        return;
    };
    let missing = ytdlp.is_none() || (mdm_tools::needed::installs_ffmpeg() && ffmpeg.is_none());
    if !missing {
        return;
    }
    {
        let mut s = shared();
        if s.running {
            return;
        }
        s.running = true;
        s.ctx = Some(ctx.clone());
    }
    rt.spawn(run(dir));
}

/// Get what is missing in `dir`, one helper after the other, until nothing is.
async fn run(dir: PathBuf) {
    let mut failures = 0u32;
    loop {
        let look = dir.clone();
        let missing = tokio::task::spawn_blocking(move || mdm_tools::needed::missing(&look))
            .await
            .unwrap_or_default();
        if missing.is_empty() {
            break;
        }
        // No network at all (no adapter has an address, or the one chosen in Options is down):
        // nothing to try until it is there.
        if is_down(&crate::settings::own_route()) {
            tracing::info!("video helpers: waiting for the network");
            wait_for_network().await;
        }
        let mut failed = false;
        for h in missing {
            let progress = Arc::new(Progress::default());
            set(|s| s.current = Some((h, progress.clone())));
            tracing::info!("video helpers: getting {}", h.name());
            let res =
                mdm_tools::needed::install(h, &dir, &crate::settings::own_net(), Some(&progress))
                    .await;
            match res {
                Ok(got) => {
                    tracing::info!(
                        "video helpers: {} {} is ready ({})",
                        h.name(),
                        got.version,
                        got.exe.display()
                    );
                    set(|s| {
                        s.current = None;
                        s.failed = None;
                    });
                    // The quality menu, the video window and a waiting video see it.
                    crate::videodlg::tools_changed(None);
                }
                Err(e) => {
                    tracing::warn!("video helpers: {} could not be downloaded: {e}", h.name());
                    let why = crate::errors::tools(&e);
                    set(|s| {
                        s.current = None;
                        s.failed = Some((h, why));
                    });
                    failed = true;
                    break;
                }
            }
        }
        if !failed {
            failures = 0;
            continue;
        }
        failures += 1;
        let wait = wait_after(failures);
        tracing::info!(
            "video helpers: trying again in {} s, or when the network changes",
            wait.as_secs()
        );
        wait_or_wake(wait).await;
    }
    set(|s| {
        s.running = false;
        s.current = None;
    });
}

/// The wait after `failures` failures in a row: 15 s, 30 s, 1 min… up to 10 min.
fn wait_after(failures: u32) -> Duration {
    let doubled = FIRST_WAIT.saturating_mul(1 << failures.saturating_sub(1).min(16));
    doubled.min(WAIT_AT_MOST)
}

fn is_down(route: &mdm_engine::Route) -> bool {
    matches!(route, mdm_engine::Route::Down { .. })
}

/// Until the network is there (the adapter watch says so), or "Try now".
async fn wait_for_network() {
    while is_down(&crate::settings::own_route()) {
        tokio::select! {
            _ = tokio::time::sleep(LOOK_EVERY) => {}
            _ = NUDGE.notified() => return,
        }
    }
}

/// `wait`, cut short when the network changes (back after being down, another adapter, a VPN
/// connected) or "Try now" is asked.
async fn wait_or_wake(wait: Duration) {
    let until = Instant::now() + wait;
    let before = crate::settings::own_route();
    let mut went_down = false;
    while Instant::now() < until {
        tokio::select! {
            _ = tokio::time::sleep(LOOK_EVERY) => {}
            _ = NUDGE.notified() => return,
        }
        let now = crate::settings::own_route();
        if is_down(&now) {
            went_down = true;
        } else if went_down || now != before {
            tracing::info!("video helpers: the network changed, trying again");
            return;
        }
    }
}

/// "Try now" (the video window's Install button): the background tries again at once, if it
/// is waiting.
pub fn nudge() {
    NUDGE.notify_one();
}

/// The helper downloading now, and how far it is (None while unknown).
pub fn downloading() -> Option<(Helper, Option<f32>)> {
    shared()
        .current
        .as_ref()
        .map(|(h, p)| (*h, p.fraction().map(|f| f.min(1.0))))
}

/// Why `h` couldn't be downloaded last time, while the background waits to try again.
pub fn failed(h: Helper) -> Option<String> {
    let s = shared();
    s.failed
        .as_ref()
        .filter(|(f, _)| *f == h && s.current.is_none())
        .map(|(_, why)| why.clone())
}

/// Whether DDM gets yt-dlp by itself when it is missing: whenever it has a data folder to put
/// it in. A video asked for meanwhile waits for it ([`video_waits`]).
pub fn gets_ytdlp() -> bool {
    mdm_tools::tools_dir().is_some()
}

/// The status strip's note while a helper downloads in the background: "Getting ready...", and
/// how far it is; never its name. None otherwise: waiting for the network, and the end, are quiet.
pub fn note(ctx: &egui::Context) -> Option<String> {
    let (_, fraction) = downloading()?;
    ctx.request_repaint_after(Duration::from_millis(500));
    // i18n: Download video, while DDM gets what it needs for videos by itself (in the background, after an offline start); a percentage may follow
    let msg = crate::i18n::t("Getting ready...");
    Some(match fraction {
        Some(f) => format!("{msg} {}", crate::i18n::percent(f64::from(f), 0)),
        None => msg.to_owned(),
    })
}

/// A video's log line when it waits for yt-dlp (app.rs `start_video`): in general words.
pub fn video_waits() -> String {
    // i18n: a video download's log entry: DDM is still getting what it needs for videos (it was offline); the video starts by itself once that is done
    crate::i18n::t("Waiting until everything is ready: the video then starts by itself").to_owned()
}

// ---- The Windows setup's step: --install-helpers ----------------------------------------------

/// The command line switch the setup runs the installed program with.
pub const SWITCH: &str = "--install-helpers";

/// Given up when no byte comes for this long (the setup leaves the rest to DDM, which resumes).
const STALL: Duration = Duration::from_secs(90);
/// And when the whole step takes longer than this, on a very slow connection.
const AT_MOST: Duration = Duration::from_secs(20 * 60);

/// A line of [`setup_main`]'s output, for the setup (tools/installer/mdm.iss reads them): plain
/// words and numbers, never translated (the setup shows its own translated texts).
#[derive(Debug, PartialEq)]
enum Line<'a> {
    /// How many helpers are to be got (0: all there).
    Helpers(usize),
    /// Helper `k` of `n` starts.
    Get(&'a str, usize, usize),
    /// How far the whole step is, in thousandths.
    Progress(u32),
    Ready(&'a str, &'a str),
    Failed(&'a str, &'a str),
}

impl std::fmt::Display for Line<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Line::Helpers(n) => write!(f, "helpers {n}"),
            Line::Get(name, k, n) => write!(f, "get {name} {k} {n}"),
            Line::Progress(p) => write!(f, "progress {p}"),
            Line::Ready(name, version) => write!(f, "ready {name} {version}"),
            // One line: a reason never breaks it.
            Line::Failed(name, why) => {
                write!(f, "failed {name} {}", why.replace(['\r', '\n'], " "))
            }
        }
    }
}

/// Write `line` to the standard output at once (the setup reads it as it comes). Nobody reading
/// (no pipe) is no error.
fn say(line: Line<'_>) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

/// About how big each helper's download is, so the bar moves evenly over both (MB).
fn weight(h: Helper) -> f64 {
    match h {
        Helper::YtDlp => 24.0,
        Helper::Ffmpeg => 80.0,
        Helper::GalleryDl => 12.0,
    }
}

/// How far the whole step is, in thousandths: the helpers before `at` done, `at` itself
/// `fraction` of the way.
fn overall(list: &[Helper], at: usize, fraction: f64) -> u32 {
    let total: f64 = list.iter().map(|h| weight(*h)).sum();
    if total <= 0.0 {
        return 1000;
    }
    let done: f64 = list[..at.min(list.len())]
        .iter()
        .map(|h| weight(*h))
        .sum::<f64>()
        + list
            .get(at)
            .map_or(0.0, |h| weight(*h) * fraction.clamp(0.0, 1.0));
    ((done / total) * 1000.0).round().min(1000.0) as u32
}

/// `dannexdownloadmanager --install-helpers`: the setup's step (tools/installer/mdm.iss). Gets
/// the helpers missing from the data folder's tools folder, as DDM would, through the proxy,
/// DNS and network adapter its settings name (an update keeps them), and says how far it is
/// ([`Line`]). Exit code 0 once nothing is missing, 1 when something still is (offline, a
/// failure, too slow): DDM gets it itself when it runs. Nothing else of the app runs.
pub fn setup_main() -> std::process::ExitCode {
    let Some(data) = mdm_ipc::data_dir() else {
        say(Line::Failed("all", "no data folder"));
        return std::process::ExitCode::FAILURE;
    };
    let _ = mdm_crash::logs::init(&data, mdm_crash::logs::APP);
    tracing::info!(
        "Dannex Download Manager {}: the setup's video helpers",
        crate::help::VERSION
    );
    let settings = crate::store::settings_only().unwrap_or_default();
    let adapter = settings.net_adapter.trim();
    let net = mdm_engine::Options {
        dns: settings.dns(),
        route: if adapter.is_empty() {
            mdm_engine::Route::Any
        } else {
            mdm_engine::net::route_for(adapter, &crate::netwatch::adapters())
        },
        ..mdm_tools::net(settings.proxy())
    };
    let all_there = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt.block_on(get_for_setup(&data.join("tools"), &net)),
        Err(e) => {
            say(Line::Failed("all", &e.to_string()));
            false
        }
    };
    mdm_crash::logs::flush(Duration::from_secs(2));
    if all_there {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}

/// [`setup_main`]'s work: true when nothing is missing at the end.
async fn get_for_setup(dir: &Path, net: &mdm_engine::Options) -> bool {
    let look = dir.to_owned();
    let list = tokio::task::spawn_blocking(move || mdm_tools::needed::missing(&look))
        .await
        .unwrap_or_default();
    say(Line::Helpers(list.len()));
    let started = Instant::now();
    for (k, h) in list.iter().enumerate() {
        say(Line::Get(h.name(), k + 1, list.len()));
        say(Line::Progress(overall(&list, k, 0.0)));
        let progress = Progress::default();
        let install = mdm_tools::needed::install(*h, dir, net, Some(&progress));
        tokio::pin!(install);
        let (mut last, mut moved) = (0u64, Instant::now());
        let mut every = tokio::time::interval(Duration::from_millis(250));
        let res = loop {
            tokio::select! {
                res = &mut install => break res.map_err(|e| e.to_string()),
                _ = every.tick() => {
                    let got = progress.got.load(Relaxed);
                    if got != last {
                        (last, moved) = (got, Instant::now());
                    }
                    let f = progress.fraction().map_or(0.0, f64::from);
                    say(Line::Progress(overall(&list, k, f)));
                    if moved.elapsed() >= STALL {
                        break Err(format!("nothing came for {} s", STALL.as_secs()));
                    }
                    if started.elapsed() >= AT_MOST {
                        break Err(format!("slower than {} minutes", AT_MOST.as_secs() / 60));
                    }
                }
            }
        };
        match res {
            Ok(got) => {
                tracing::info!("setup: {} {} ready", h.name(), got.version);
                say(Line::Ready(h.name(), &got.version));
            }
            Err(why) => {
                // The rest waits for DDM too: the network is the likely reason. What came of
                // this one is kept, and DDM's own download resumes it.
                tracing::warn!("setup: {} could not be downloaded: {why}", h.name());
                say(Line::Failed(h.name(), &why));
                return false;
            }
        }
    }
    say(Line::Progress(1000));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wait_grows_after_each_failure_up_to_ten_minutes() {
        let waits: Vec<u64> = (1..=8).map(|n| wait_after(n).as_secs()).collect();
        assert_eq!(waits, [15, 30, 60, 120, 240, 480, 600, 600]);
        assert_eq!(wait_after(1000), WAIT_AT_MOST);
    }

    /// The setup reads these lines (mdm.iss `HelperLine`): their words and order are a contract.
    #[test]
    fn the_setup_reads_plain_lines() {
        assert_eq!(Line::Helpers(2).to_string(), "helpers 2");
        assert_eq!(Line::Get("yt-dlp", 1, 2).to_string(), "get yt-dlp 1 2");
        assert_eq!(Line::Progress(250).to_string(), "progress 250");
        assert_eq!(
            Line::Ready("ffmpeg", "9.0.20261126").to_string(),
            "ready ffmpeg 9.0.20261126"
        );
        assert_eq!(
            Line::Failed("yt-dlp", "network:\r\nno route").to_string(),
            "failed yt-dlp network:  no route"
        );
    }

    /// The bar moves by each helper's size: yt-dlp is the smaller part, ffmpeg the larger.
    #[test]
    fn the_bar_moves_by_size() {
        let both = [Helper::YtDlp, Helper::Ffmpeg];
        assert_eq!(overall(&both, 0, 0.0), 0);
        assert_eq!(overall(&both, 0, 1.0), 231);
        assert_eq!(overall(&both, 1, 0.0), 231);
        assert_eq!(overall(&both, 1, 0.5), 615);
        assert_eq!(overall(&both, 1, 1.0), 1000);
        assert_eq!(overall(&both, 2, 0.0), 1000);
        assert_eq!(overall(&[Helper::Ffmpeg], 0, 0.25), 250);
        assert_eq!(overall(&[], 0, 0.0), 1000);
    }

    /// Off-line (the adapter watch's `Down`), the background doesn't try; any other route is a
    /// network to try on.
    #[test]
    fn a_network_still_down_is_no_reason_to_try() {
        let down = mdm_engine::Route::Down {
            label: String::from("x"),
        };
        assert!(is_down(&down));
        assert!(!is_down(&mdm_engine::Route::Any));
    }
}
