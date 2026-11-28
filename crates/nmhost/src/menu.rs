//! The video button's menu: the page's video looked up by the app, its qualities polled until they
//! settle, and the one picked handed over.

use super::*;

/// Which of the quality menu's messages: what an answer that isn't the app's means depends on
/// it ([`menu_answer`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MenuAsk {
    Lookup,
    Poll,
    Pick,
    Cancel,
}

/// The longest page title passed on (the extension cuts it there too).
pub(super) const TITLE_MAX: usize = 300;

/// The longest look-up token taken (the app's are 64 hex digits).
pub(super) const TOKEN_MAX: usize = 128;

/// How long a question about a look-up waits here while it runs, asking the app again every
/// [`POLL_EVERY`]: the browser then asks about once every 1.5 s while yt-dlp works (a host
/// started for each question), and hears of the answer within 25 ms of it (#3: a tenth of a
/// second before; each question is a millisecond on this computer, and the app answers it on a
/// pass of its own logic, not its window's).
pub(super) const POLL_WAIT: Duration = Duration::from_millis(1500);

pub(super) const POLL_EVERY: Duration = Duration::from_millis(25);

/// The Download video window's command for a video message: the page, or the video's own file
/// or its stream's manifest (`src`) fetched as from its page; or why it is refused (the reply's
/// `why` and `error`).
pub(super) fn video_command(
    m: &Message,
    rules: &mdm_ipc::capture::CaptureRules,
    browser: &str,
) -> Result<mdm_ipc::Command, (&'static str, &'static str)> {
    if !(m.url.starts_with("http://") || m.url.starts_with("https://")) {
        return Err(("refused", "not an http(s) page"));
    }
    // The toolbar button's list of the tab's media (#38) asks for one explicitly: whether the
    // button over videos shows on this site doesn't matter then.
    if !m.explicit && !rules.panel_on(&m.url, m.referrer.as_deref(), browser) {
        return Err(("options", "the video button is off here"));
    }
    // The video clicked, not the page's first one (a feed plays many): its own file when there
    // is one, fetched as from its page.
    let src = m
        .src
        .clone()
        .filter(|s| s.starts_with("http://") || s.starts_with("https://"));
    let referer = match &src {
        Some(_) => Some(m.url.clone()),
        None => m.referrer.clone().filter(|r| !r.is_empty()),
    };
    Ok(mdm_ipc::Command {
        video_page: Some(src.unwrap_or_else(|| m.url.clone())),
        video_title: m.title.clone().filter(|t| !t.trim().is_empty()),
        referer,
        cookies: m.cookies.clone().filter(|c| !c.is_empty()),
        cookie_jar: m.cookie_jar.clone(),
        user_agent: m.user_agent.clone(),
        private: m.private,
        ..Default::default()
    })
}

/// The quality menu's message `m` is, if it is one.
pub(super) fn menu_ask(m: &Message) -> Option<MenuAsk> {
    if m.video_lookup.is_some() {
        Some(MenuAsk::Lookup)
    } else if m.video_poll.is_some() {
        Some(MenuAsk::Poll)
    } else if m.video_pick.is_some() {
        Some(MenuAsk::Pick)
    } else if m.video_cancel.is_some() {
        Some(MenuAsk::Cancel)
    } else {
        None
    }
}

/// The app's command for one of the quality menu's messages, or, without asking the app, its
/// answer's state: `use-window` for a page not on the web (or a choice without a token), `off`
/// where the button is off (Options' rules, as `panel` asks), `gone` for a question without a
/// token. Each command says `startup` too: an older app, which knows none of them, takes it for
/// its startup entry's and opens nothing (a bare command would bring its window up).
pub(super) fn menu_command(
    ask: MenuAsk,
    m: &Message,
    rules: &mdm_ipc::capture::CaptureRules,
    browser: &str,
) -> Result<mdm_ipc::Command, &'static str> {
    use mdm_ipc::api::lookup;
    let token =
        |t: &str| Some(t.trim().to_owned()).filter(|t| !t.is_empty() && t.len() <= TOKEN_MAX);
    let store = |s: &Option<String>| s.clone().filter(|s| !s.trim().is_empty());
    let cmd = match ask {
        MenuAsk::Lookup => {
            let l = m.video_lookup.as_ref().ok_or(lookup::USE_WINDOW)?;
            let web = |s: &str| s.starts_with("http://") || s.starts_with("https://");
            if !web(&l.page) {
                return Err(lookup::USE_WINDOW);
            }
            if !rules.panel_on(&l.page, l.referrer.as_deref(), browser) {
                return Err(lookup::OFF);
            }
            // As the button's click (`video_command`): the video clicked, its own file when
            // there is one, fetched as from its page, with the page's title (a video yt-dlp
            // knows only by its address is named by it, #345).
            let src = l.src.clone().filter(|s| web(s));
            let referer = match &src {
                Some(_) => Some(l.page.clone()),
                None => l.referrer.clone().filter(|r| !r.is_empty()),
            };
            let title = l
                .title
                .as_deref()
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(|t| t.chars().take(TITLE_MAX).collect());
            mdm_ipc::Command {
                video_lookup: Some(mdm_ipc::VideoLookup {
                    address: src.unwrap_or_else(|| l.page.clone()),
                    title,
                    store_id: store(&l.store_id),
                    prefetch: l.prefetch,
                }),
                referer,
                cookies: m.cookies.clone().filter(|c| !c.is_empty()),
                cookie_jar: m.cookie_jar.clone(),
                user_agent: m.user_agent.clone(),
                private: m.private,
                ..Default::default()
            }
        }
        MenuAsk::Poll => {
            let p = m.video_poll.as_ref().ok_or(lookup::GONE)?;
            mdm_ipc::Command {
                video_poll: Some(token(&p.token).ok_or(lookup::GONE)?),
                ..Default::default()
            }
        }
        MenuAsk::Pick => {
            let p = m.video_pick.as_ref().ok_or(lookup::USE_WINDOW)?;
            mdm_ipc::Command {
                video_pick: Some(mdm_ipc::VideoPick {
                    token: token(&p.token).ok_or(lookup::USE_WINDOW)?,
                    choice: p.choice,
                    store_id: store(&p.store_id),
                }),
                private: m.private,
                ..Default::default()
            }
        }
        MenuAsk::Cancel => {
            let c = m.video_cancel.as_ref().ok_or(lookup::GONE)?;
            mdm_ipc::Command {
                video_cancel: Some(token(&c.token).ok_or(lookup::GONE)?),
                ..Default::default()
            }
        }
    };
    Ok(mdm_ipc::Command {
        startup: true,
        ..cmd
    })
}

/// One of the quality menu's messages, asked of the running app (never started for it: a menu
/// over a video isn't worth starting DDM unseen; the button's click does that).
pub(super) fn quality_menu(
    ask: MenuAsk,
    m: &Message,
    rules: &mdm_ipc::capture::CaptureRules,
    browser: &str,
) -> Reply {
    let cmd = match menu_command(ask, m, rules, browser) {
        Ok(cmd) => cmd,
        Err(state) => return Reply::lookup(mdm_ipc::api::VideoLookupAnswer::of(state)),
    };
    // The choice brings its download's progress window up: the app may bring it to the front
    // (Windows only, as in `deliver`: Linux and macOS have no foreground lock for a host to
    // lift). A look-up and the questions about it open nothing.
    #[cfg(windows)]
    if ask == MenuAsk::Pick
        && let Some(pid) = mdm_ipc::app_pid()
    {
        win32::allow_foreground(pid);
    }
    let token = cmd.video_poll.clone().unwrap_or_default();
    let ask_app = || menu_answer(ask, mdm_ipc::request_checked(&cmd), &token);
    let answer = match ask {
        MenuAsk::Poll => until_settled(ask_app, POLL_WAIT, POLL_EVERY),
        MenuAsk::Lookup | MenuAsk::Pick | MenuAsk::Cancel => ask_app(),
    };
    // A look-up asked ahead that didn't start (another one runs) isn't worth a line.
    if let Some(l) = &cmd.video_lookup
        && !(l.prefetch && answer.state == mdm_ipc::api::lookup::BUSY)
    {
        log_line(&lookup_line(
            browser,
            &l.address,
            m.private,
            l.prefetch,
            &answer.state,
        ));
    }
    Reply::lookup(answer)
}

/// `ask`'s answer once it is no longer `pending`, asked again every `every`, for `wait` at most
/// (then `pending` as it is).
pub(super) fn until_settled(
    ask: impl FnMut() -> mdm_ipc::api::VideoLookupAnswer,
    wait: Duration,
    every: Duration,
) -> mdm_ipc::api::VideoLookupAnswer {
    let start = std::time::Instant::now();
    until_settled_on(ask, wait, every, || start.elapsed(), std::thread::sleep)
}

/// [`until_settled`] on any clock: `elapsed` is the time since the first question, `sleep`
/// waits. The tests run it on a clock of their own, so how often it asks doesn't depend on how
/// busy the machine is (a loaded CI runner oversleeps and asks less often).
pub(super) fn until_settled_on(
    mut ask: impl FnMut() -> mdm_ipc::api::VideoLookupAnswer,
    wait: Duration,
    every: Duration,
    elapsed: impl Fn() -> Duration,
    mut sleep: impl FnMut(Duration),
) -> mdm_ipc::api::VideoLookupAnswer {
    loop {
        let answer = ask();
        if answer.state != mdm_ipc::api::lookup::PENDING || elapsed() + every > wait {
            return answer;
        }
        sleep(every);
    }
}

/// The app's answer to the quality menu's `ask` (`token`: the look-up a question is about), or
/// what its absence means. No app running, no answer, a refusal (`denied`, `gone`,
/// `too-large`), or `ok` (an older app, which knows no quality menu): a look-up or a choice is
/// left to the Download video window (`use-window`). `timeout` is different: the app's port
/// says it only after handing the command over (`mdm_ipc`'s server queues it, then waits for
/// the answer), so the app has it and will act on it. A choice is then `sent` (downloading
/// soon; the Download video window would download it twice), a look-up `busy` (its yt-dlp is
/// running; another window would start another), and a question `pending` again. Anything
/// else a question gets is `gone`.
pub(super) fn menu_answer(
    ask: MenuAsk,
    got: Result<String, mdm_ipc::SendError>,
    token: &str,
) -> mdm_ipc::api::VideoLookupAnswer {
    use mdm_ipc::api::{VideoLookupAnswer, lookup};
    if let Ok(line) = &got
        && let Ok(answer) = serde_json::from_str::<VideoLookupAnswer>(line)
        && !answer.state.is_empty()
    {
        return answer;
    }
    let timeout = matches!(&got, Ok(l) if l.trim() == "timeout");
    match ask {
        MenuAsk::Poll if timeout => VideoLookupAnswer::pending(token),
        MenuAsk::Poll => VideoLookupAnswer::of(lookup::GONE),
        MenuAsk::Pick if timeout => VideoLookupAnswer::of(lookup::SENT),
        MenuAsk::Lookup if timeout => VideoLookupAnswer::of(lookup::BUSY),
        MenuAsk::Lookup | MenuAsk::Pick => VideoLookupAnswer::of(lookup::USE_WINDOW),
        // Whatever became of it, the token is nothing to the extension any more.
        MenuAsk::Cancel => VideoLookupAnswer::of(lookup::GONE),
    }
}

/// The log line for a look-up the quality menu asked for (`ahead`: before a click): the video's
/// site (not its address, which may carry a signed link's keys) and how it went. A private
/// window's site is left out, as in [`left_line`]: the log outlives the window.
pub(super) fn lookup_line(
    browser: &str,
    address: &str,
    private: bool,
    ahead: bool,
    state: &str,
) -> String {
    let browser = if browser.is_empty() {
        "a browser"
    } else {
        browser
    };
    let site = clean(&mdm_ipc::capture::host_of(address), 200);
    let what = if private {
        "a video in a private window".to_owned()
    } else if site.trim().is_empty() {
        "a video".to_owned()
    } else {
        format!("a video on {site}")
    };
    let when = if ahead { " ahead of a click" } else { "" };
    format!(
        "browser integration: {browser} looked up the qualities of {what}{when}: {}",
        clean(state, 32)
    )
}
