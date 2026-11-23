//! "Download with the browser" (#376): a download the site's bot check refused, even when DDM
//! asked the way a browser connects (#668), opens in the browser it came from (else the
//! system's), where the user can answer the check. The browser's download that follows is left
//! to the browser: its extension offers it to DDM as usual (extension/background.js `offer`,
//! the native host, the command channel), and the channel answers it `handed-back`
//! (`mdm_ipc::HANDED_BACK`) on its own thread, before the window sees it ([`gate`]). Each
//! address handed back is answered so once, within [`KEPT`], by the address asked for or where
//! the redirects ended.
//!
//! The row stays in the list as it is (its partial data too): Resume tries again.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::model::{Item, Status};

/// How long an address handed back to the browser waits for the browser's download of it.
pub const KEPT: Duration = Duration::from_secs(10 * 60);

/// What "Download with the browser" needs of a download the site's bot check refused, kept with
/// the list (`Item::bot_check`): after a restart it is still offered, in the same browser.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BotCheck {
    /// The browser it came from, by the names Options lists ("Mozilla Firefox"); none when not
    /// known (added another way, or by an older native host): the system's browser opens it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub browser: Option<String>,
    /// Where the browser's own download of it had been redirected to (`Command::final_url`):
    /// the browser's next download may start there (a check answered on that page reloads it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_url: Option<String>,
}

/// The mark a run's end leaves on its row (`failure`: why it failed, if it did): the site's bot
/// check keeps the browser and the browser's redirect from the browser's request (`context`),
/// else from the `last` mark (the request isn't kept over a restart); any other end, none.
pub fn after_run(
    last: Option<BotCheck>,
    failure: Option<&mdm_engine::Error>,
    context: &mdm_ipc::Command,
) -> Option<BotCheck> {
    if !matches!(failure, Some(mdm_engine::Error::BotCheck)) {
        return None;
    }
    let last = last.unwrap_or_default();
    let given = |s: &Option<String>| s.clone().filter(|s| !s.trim().is_empty());
    Some(BotCheck {
        browser: given(&context.from_browser).or(last.browser),
        final_url: given(&context.final_url).or(last.final_url),
    })
}

/// Whether the progress window and the right-click menu offer "Download with the browser" for
/// `item`: a web file's download whose last run the site's bot check refused, not running.
pub fn offered(item: &Item) -> bool {
    item.status == Status::Error
        && item.live.is_none()
        && item.bot_check.is_some()
        && matches!(item.url.scheme(), "http" | "https")
        && item.video.is_none()
        && item.torrent.is_none()
        && item.gallery.is_none()
        && item.remote.is_none()
}

/// The addresses handed back to the browser, each waiting for the browser's download of it.
#[derive(Default)]
pub struct HandBack {
    waiting: Mutex<Vec<Waiting>>,
}

/// One download handed back: the addresses its browser download may have ([`key`]s), until when.
struct Waiting {
    addresses: Vec<String>,
    until: Instant,
}

impl HandBack {
    /// Hand one download back, by its `addresses` (the one asked for, where redirects ended).
    pub fn add(&self, addresses: impl IntoIterator<Item = String>) {
        self.add_at(addresses, Instant::now());
    }

    fn add_at(&self, addresses: impl IntoIterator<Item = String>, now: Instant) {
        let mut addresses: Vec<String> = addresses.into_iter().filter_map(|a| key(&a)).collect();
        addresses.dedup();
        if addresses.is_empty() {
            return;
        }
        let mut waiting = self.waiting.lock().unwrap_or_else(|e| e.into_inner());
        waiting.retain(|w| w.until > now);
        waiting.push(Waiting {
            addresses,
            until: now + KEPT,
        });
    }

    /// Whether `cmd` is the browser's download of an address handed back (its own address or
    /// where its redirects ended), which then waits no more: it stays with the browser. Only a
    /// download the browser goes on with when DDM leaves it (`browser_waits`) is.
    pub fn take(&self, cmd: &mdm_ipc::Command) -> bool {
        self.take_at(cmd, Instant::now())
    }

    fn take_at(&self, cmd: &mdm_ipc::Command, now: Instant) -> bool {
        if !cmd.browser_waits {
            return false;
        }
        let asked: Vec<String> = [&cmd.url, &cmd.final_url]
            .into_iter()
            .flatten()
            .filter_map(|a| key(a))
            .collect();
        if asked.is_empty() {
            return false;
        }
        let mut waiting = self.waiting.lock().unwrap_or_else(|e| e.into_inner());
        waiting.retain(|w| w.until > now);
        match waiting
            .iter()
            .position(|w| w.addresses.iter().any(|a| asked.contains(a)))
        {
            Some(i) => {
                waiting.remove(i);
                true
            }
            None => false,
        }
    }
}

/// An address as the browser's download and DDM's row are matched by: a web address, without
/// its `#fragment` and without the query parameters Cloudflare's challenge adds while it is
/// answered (`__cf_chl_tk`, `__cf_chl_rt_tk` …, which its page drops again after).
fn key(address: &str) -> Option<String> {
    let mut url = url::Url::parse(address.trim()).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    url.set_fragment(None);
    // The other parameters as written (not decoded and encoded again: `%20` stays `%20`).
    if let Some(query) = url.query().map(str::to_owned) {
        let kept: Vec<&str> = query
            .split('&')
            .filter(|p| !p.starts_with("__cf_chl_"))
            .collect();
        if kept.is_empty() {
            url.set_query(None);
        } else {
            url.set_query(Some(&kept.join("&")));
        }
    }
    Some(url.to_string())
}

/// The app's one list, which the command channel's listener asks ([`gate`]).
pub fn shared() -> &'static HandBack {
    static LIST: std::sync::OnceLock<HandBack> = std::sync::OnceLock::new();
    LIST.get_or_init(HandBack::default)
}

/// What the command channel asks of each command before the app sees it (main.rs): a browser's
/// download of an address handed back is answered `handed-back`, on the listener's thread.
pub fn gate() -> mdm_ipc::Gate {
    std::sync::Arc::new(|cmd: &mdm_ipc::Command| shared().take(cmd).then_some(mdm_ipc::HANDED_BACK))
}

/// "Download with the browser" for one row, as taken from it on the window's thread
/// ([`Ask::of`]); [`open`] does the rest beside the window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ask {
    /// The address to open.
    pub url: String,
    /// The browser to open it in, by the names Options lists; none: the system's.
    pub browser: Option<String>,
    /// Where the browser's own download of it had been redirected to, if known.
    pub also: Vec<String>,
    /// Its resume state, which says where DDM's own redirects ended on an earlier run.
    pub state: PathBuf,
}

impl Ask {
    pub fn of(item: &Item) -> Ask {
        let mark = item.bot_check.clone().unwrap_or_default();
        let given = |s: &Option<String>| s.clone().filter(|s| !s.trim().is_empty());
        let mut also: Vec<String> = [mark.final_url, given(&item.context.final_url)]
            .into_iter()
            .flatten()
            .collect();
        also.dedup();
        Ask {
            url: item.url.to_string(),
            browser: mark.browser.or_else(|| given(&item.context.from_browser)),
            also,
            state: mdm_engine::state::state_path(&item.dest),
        }
    }
}

/// Hand `ask`'s download back to the browser, on a thread of its own (its resume state is on a
/// disk that may be asleep, #743; finding the browsers reads the registry or the PATH): its
/// addresses go on the hand-back list ([`shared`]), then its address opens in its browser, or
/// the system's when that one isn't found.
pub fn open(ask: Ask) {
    crate::launch::spawn("handback", move || {
        let mut addresses = vec![ask.url.clone()];
        addresses.extend(ask.also.iter().cloned());
        // Where DDM's own redirects ended, when an earlier run got that far.
        if let Ok(state) = mdm_engine::state::ResumeState::load(&ask.state)
            && state.url.as_str() == ask.url
        {
            addresses.push(state.final_url.to_string());
        }
        shared().add(addresses);
        let program = ask.browser.as_deref().and_then(program_for);
        let host = url::Url::parse(&ask.url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_owned))
            .unwrap_or_default();
        tracing::info!(
            "Download with the browser: {host} handed back to {}",
            match (&program, &ask.browser) {
                (Some(_), Some(b)) => b.as_str(),
                _ => "the system's browser",
            }
        );
        match program {
            #[cfg(windows)]
            Some(program) => crate::win32::open_in(&program, &ask.url),
            #[cfg(unix)]
            Some(program) => crate::unix::open_in(&program, &ask.url),
            None => {
                if let Err(e) = open::that_detached(&ask.url) {
                    tracing::warn!("Download with the browser: the system's browser: {e}");
                }
            }
        }
    });
}

/// The row a browser has among the browsers installed (`mdm_ipc::browsers::BROWSERS`' names),
/// from its name in Options. Brave calls itself "Brave Browser" there.
fn installed_name(browser: &str) -> &str {
    match browser {
        "Brave Browser" => "Brave",
        other => other,
    }
}

/// The program of the browser named `browser` (Options' names), if it is installed: Windows'
/// "default browser" list and App Paths, the app bundles on macOS, the commands on the PATH on
/// Linux (win32.rs / unix.rs `browsers`). None, and the system's browser opens the address,
/// for a browser not found: one that names itself Chrome without being it (Vivaldi, Chromium,
/// Helium, Thorium and Arc do, so "Google Chrome" may be any of them), or a Flatpak or Snap
/// browser on Linux, which `browsers` leaves out (its sandbox can't start the native host).
fn program_for(browser: &str) -> Option<PathBuf> {
    // Safari comes with macOS and isn't in the table of browsers the extension is offered in.
    if browser == "Apple Safari" {
        return [
            "/Applications/Safari.app",
            "/System/Cryptexes/App/System/Applications/Safari.app",
        ]
        .iter()
        .map(PathBuf::from)
        .find(|p| cfg!(target_os = "macos") && p.exists());
    }
    let name = installed_name(browser);
    #[cfg(windows)]
    let found = crate::win32::browsers();
    #[cfg(unix)]
    let found = crate::unix::browsers();
    found.into_iter().find(|(n, _)| n == name).map(|(_, p)| p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn download(url: &str, final_url: Option<&str>) -> mdm_ipc::Command {
        mdm_ipc::Command {
            url: Some(url.into()),
            final_url: final_url.map(Into::into),
            browser_waits: true,
            ..Default::default()
        }
    }

    #[test]
    fn the_browsers_download_is_matched_by_either_address_once() {
        let list = HandBack::default();
        let now = Instant::now();
        list.add_at(
            [
                "https://x.test/get?id=1".to_owned(),
                "https://cdn.test/f/a.zip".to_owned(),
            ],
            now,
        );
        // Another address: not this one.
        assert!(!list.take_at(&download("https://x.test/get?id=2", None), now));
        // Its address once: the browser keeps it; asked again, DDM takes it.
        assert!(list.take_at(&download("https://x.test/get?id=1", None), now));
        assert!(!list.take_at(&download("https://x.test/get?id=1", None), now));
        // Where the redirects ended, as the browser's download names it (its first address
        // another, a challenge's reload of the page it ended on).
        list.add_at(
            [
                "https://x.test/get?id=1".to_owned(),
                "https://cdn.test/f/a.zip".to_owned(),
            ],
            now,
        );
        assert!(list.take_at(
            &download("https://cdn.test/f/a.zip", Some("https://cdn2.test/a.zip")),
            now
        ));
        assert!(!list.take_at(&download("https://x.test/get?id=1", None), now));
        // The browser's download whose redirects ended at an address handed back.
        list.add_at(["https://cdn.test/f/b.zip".to_owned()], now);
        assert!(list.take_at(
            &download("https://x.test/get?id=9", Some("https://cdn.test/f/b.zip")),
            now
        ));
    }

    #[test]
    fn an_address_waits_ten_minutes() {
        let list = HandBack::default();
        let now = Instant::now();
        list.add_at(["https://x.test/a.zip".to_owned()], now);
        list.add_at(["https://x.test/b.zip".to_owned()], now);
        assert!(list.take_at(
            &download("https://x.test/a.zip", None),
            now + KEPT - Duration::from_secs(1)
        ));
        assert!(!list.take_at(&download("https://x.test/b.zip", None), now + KEPT));
        assert!(list.waiting.lock().unwrap().is_empty(), "the old ones go");
    }

    /// A link asked for from the browser's right-click menu has no download of the browser's
    /// behind it (left, nothing would happen): DDM takes it, and the address still waits.
    #[test]
    fn only_a_download_the_browser_goes_on_with_is_handed_back() {
        let list = HandBack::default();
        let now = Instant::now();
        list.add_at(["https://x.test/a.zip".to_owned()], now);
        let mut asked = download("https://x.test/a.zip", None);
        asked.browser_waits = false;
        assert!(!list.take_at(&asked, now));
        assert!(list.take_at(&download("https://x.test/a.zip", None), now));
        // Nor anything without a web address.
        list.add_at(
            ["ftp://x.test/a.zip".to_owned(), "not an address".into()],
            now,
        );
        assert!(list.waiting.lock().unwrap().is_empty());
        assert!(!list.take_at(&mdm_ipc::Command::default(), now));
    }

    #[test]
    fn addresses_match_without_fragment_or_the_challenges_parameters() {
        assert_eq!(
            key("https://x.test/a.zip?__cf_chl_rt_tk=abc#top").as_deref(),
            Some("https://x.test/a.zip")
        );
        assert_eq!(
            key("https://x.test/get?id=1&__cf_chl_tk=z").as_deref(),
            Some("https://x.test/get?id=1")
        );
        assert_eq!(
            key(" https://X.test/get?id=1 ").as_deref(),
            Some("https://x.test/get?id=1")
        );
        assert_eq!(
            key("https://x.test/a%20b.zip?q=a%20b").as_deref(),
            Some("https://x.test/a%20b.zip?q=a%20b"),
            "a query without the challenge's stays as written"
        );
        assert_eq!(key("magnet:?xt=urn:btih:abc"), None);
        let list = HandBack::default();
        let now = Instant::now();
        list.add_at(["https://x.test/a.zip".to_owned()], now);
        assert!(list.take_at(&download("https://x.test/a.zip?__cf_chl_tk=1#x", None), now));
    }

    /// The channel's gate answers for the app's own list.
    #[test]
    fn the_gate_answers_handed_back() {
        let url = "https://handback-gate.test/only-here.zip";
        assert_eq!(gate()(&download(url, None)), None);
        shared().add([url.to_owned()]);
        assert_eq!(gate()(&download(url, None)), Some(mdm_ipc::HANDED_BACK));
        assert_eq!(gate()(&download(url, None)), None, "once");
    }

    fn row(status: Status) -> Item {
        let url = url::Url::parse("https://x.test/a.zip").unwrap();
        let mut item = Item::new(
            1,
            url,
            PathBuf::from("a.zip"),
            crate::model::Category::of("a.zip"),
        );
        item.status = status;
        item
    }

    #[test]
    fn a_bot_check_leaves_its_mark_and_any_other_end_none() {
        let browser = mdm_ipc::Command {
            from_browser: Some("Mozilla Firefox".into()),
            final_url: Some("https://cdn.test/a.zip".into()),
            ..Default::default()
        };
        let bot = mdm_engine::Error::BotCheck;
        let mark = after_run(None, Some(&bot), &browser).unwrap();
        assert_eq!(mark.browser.as_deref(), Some("Mozilla Firefox"));
        assert_eq!(mark.final_url.as_deref(), Some("https://cdn.test/a.zip"));
        // After a restart the request is gone: the mark keeps what it had.
        let again = after_run(Some(mark.clone()), Some(&bot), &Default::default());
        assert_eq!(again, Some(mark.clone()));
        // Added by hand: a mark without a browser.
        assert_eq!(
            after_run(None, Some(&bot), &Default::default()),
            Some(BotCheck::default())
        );
        let other = mdm_engine::Error::DiskFull;
        assert_eq!(after_run(Some(mark.clone()), Some(&other), &browser), None);
        assert_eq!(after_run(Some(mark), None, &browser), None, "finished");
    }

    #[test]
    fn offered_for_a_failed_web_download_the_bot_check_refused() {
        let mut item = row(Status::Error);
        assert!(!offered(&item), "another failure");
        item.bot_check = Some(BotCheck::default());
        assert!(offered(&item));
        for status in [
            Status::Stopped,
            Status::Complete,
            Status::Queued,
            Status::Downloading,
        ] {
            item.status = status;
            assert!(!offered(&item), "{status:?}");
        }
        item.status = Status::Error;
        item.url = url::Url::parse("ftp://x.test/a.zip").unwrap();
        assert!(!offered(&item), "an FTP address: no browser downloads it");
    }

    #[test]
    fn the_ask_takes_the_rows_browser_and_addresses() {
        let mut item = row(Status::Error);
        item.bot_check = Some(BotCheck {
            browser: Some("Brave Browser".into()),
            final_url: Some("https://cdn.test/a.zip".into()),
        });
        let ask = Ask::of(&item);
        assert_eq!(ask.url, "https://x.test/a.zip");
        assert_eq!(ask.browser.as_deref(), Some("Brave Browser"));
        assert_eq!(ask.also, ["https://cdn.test/a.zip"]);
        assert!(ask.state.to_string_lossy().ends_with(".ddm.json"));
        // Without a mark's browser: the request's, else none (the system's browser).
        item.bot_check = Some(BotCheck::default());
        item.context.from_browser = Some("Microsoft Edge".into());
        assert_eq!(Ask::of(&item).browser.as_deref(), Some("Microsoft Edge"));
        item.context.from_browser = None;
        assert_eq!(Ask::of(&item).browser, None);
        assert_eq!(installed_name("Brave Browser"), "Brave");
        assert_eq!(installed_name("Mozilla Firefox"), "Mozilla Firefox");
    }
}
