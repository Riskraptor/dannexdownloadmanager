//! A message answered: Options' rules asked, a download checked against them and turned into the
//! app's command (with the browser's proxy), and the downloads left to the browser logged.

use super::*;

/// Why the browser keeps a download DDM handed back to it (#376), for DDM's log.
pub(super) const HANDED_BACK_REASON: &str =
    "handed back by DDM after the site's bot check (Download with the browser)";

/// The largest proxy auto-config script taken from the browser (#42).
pub(super) const PAC_MAX: usize = 256 * 1024;

/// The longest link text handed on, in characters (the rest ends in "…").
pub(super) const LINK_TEXT_MAX: usize = 200;

/// The answer to one message, under `rules` (the app's saved Options).
pub(super) fn answer(m: Message, rules: &mdm_ipc::capture::CaptureRules) -> Reply {
    let browser = browser(&m);
    if m.rules || m.hello {
        let (prevent, force) = rules.click_keys();
        return Reply {
            ok: true,
            version: Some(env!("CARGO_PKG_VERSION")),
            types: Some(rules.auto_types.clone()),
            capture: Some(rules.captures(browser)),
            excluded_sites: Some(rules.excluded_sites.clone()),
            excluded_addresses: Some(rules.excluded_addresses.clone()),
            opened: Some(rules.capture_opened),
            min_kb: Some(rules.capture_min_kb),
            capture_off: rules.capture_off(browser).map(|d| d.code()),
            video_lookup: Some(true),
            keys: Some(ClickKeys { prevent, force }),
            menu: Some(rules.menu_shown()),
            store: mdm_ipc::extension::store_page(),
            ..Default::default()
        };
    }
    if let Some(n) = &m.declined {
        log_left(browser, &n.site, &n.name, notice_reason(&n.why), m.private);
        return Reply::done();
    }
    // The video button's quality menu: asked of a running app only, answered in `lookup`.
    if let Some(ask) = menu_ask(&m) {
        return quality_menu(ask, &m, rules, browser);
    }
    // The video button: asked about, clicked, its menu. What it sends goes to the app (started
    // if need be), which opens its window or keeps the setting.
    if m.panel {
        return Reply {
            ok: rules.panel_on(&m.url, m.referrer.as_deref(), browser),
            panel: Some(PanelLook {
                video: rules.video_panel_video,
                audio: rules.video_panel_audio,
                min_seconds: rules.video_panel_min_s,
                corner: rules.panel_corner(),
            }),
            ..Default::default()
        };
    }
    let hide = m
        .hide_site
        .as_deref()
        .map(str::trim)
        .filter(|h| !h.is_empty());
    let panel_cmd = if let Some(host) = hide {
        Some(mdm_ipc::Command {
            panel_hide_site: Some(host.to_owned()),
            ..Default::default()
        })
    } else if m.open_options {
        let page = match m.page.as_deref() {
            Some("general") => "options:general",
            _ => "options:panels",
        };
        Some(mdm_ipc::Command {
            show: Some(page.into()),
            ..Default::default()
        })
    } else if m.open {
        // An empty command: the running app comes to the front (a second launch's).
        Some(mdm_ipc::Command::default())
    } else if let Some(host) = m
        .exclude_site
        .as_deref()
        .map(str::trim)
        .filter(|h| !h.is_empty())
    {
        Some(mdm_ipc::Command {
            exclude_site: Some(host.to_owned()),
            ..Default::default()
        })
    } else if !m.links.is_empty() {
        // Each link with its text, so the two stay in step when some are left out.
        let texts = m
            .texts
            .iter()
            .map(String::as_str)
            .chain(std::iter::repeat(""));
        let (links, link_texts): (Vec<String>, Vec<String>) = m
            .links
            .iter()
            .zip(texts)
            .filter(|(l, _)| downloadable(l))
            .map(|(l, t)| (l.clone(), link_text(t)))
            .unzip();
        if links.is_empty() {
            return Reply::failed("refused", "no link DDM downloads");
        }
        let link_texts = if link_texts.iter().all(String::is_empty) {
            Vec::new()
        } else {
            link_texts
        };
        Some(mdm_ipc::Command {
            links,
            link_texts,
            referer: m.referrer.clone().filter(|r| !r.is_empty()),
            cookies: m.cookies.clone().filter(|c| !c.is_empty()),
            user_agent: m.user_agent.clone(),
            private: m.private,
            ..Default::default()
        })
    } else if m.video {
        match video_command(&m, rules, browser) {
            Ok(cmd) => Some(cmd),
            Err((why, error)) => return Reply::failed(why, error),
        }
    } else {
        None
    };
    if let Some(cmd) = panel_cmd {
        return delivered(&cmd);
    }
    // A magnet link from the right-click menu (Download with DDM): the app's Add torrent window
    // shows what it holds. Only when asked for: Options' file types don't apply to it.
    if m.explicit && is_magnet(&m.url) {
        return delivered(&mdm_ipc::Command {
            url: Some(m.url.clone()),
            private: m.private,
            ..Default::default()
        });
    }
    if !downloadable(&m.url) {
        return Reply::failed("refused", "not an http(s) or ftp(s) URL");
    }
    let site = mdm_ipc::capture::host_of(&m.url);
    if let Err(why) = verdict(&m, rules) {
        log_left(browser, &site, &file_name_of(&m), why.reason(), m.private);
        return Reply {
            declined: Some(why.code()),
            ..Reply::failed(
                "options",
                format!("left to the browser by Options ({why:?})"),
            )
        };
    }
    let (name, private) = (file_name_of(&m), m.private);
    let reply = delivered(&to_command(m));
    if reply.declined == Some(mdm_ipc::HANDED_BACK) {
        log_left(browser, &site, &name, HANDED_BACK_REASON, private);
    }
    reply
}

/// Why the extension left a download alone, from its notice's code.
pub(super) fn notice_reason(code: &str) -> String {
    match mdm_ipc::capture::Declined::from_code(code) {
        Some(d) => d.reason().to_owned(),
        None if code == "site-access" => {
            "the browser doesn't let the extension read this site (its site access is limited)"
                .to_owned()
        }
        None => format!("{:?}", clean(code, 32)),
    }
}

/// `s` for one line of the log: no line breaks or other control characters, at most `max`
/// characters.
pub(super) fn clean(s: &str, max: usize) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(max)
        .collect()
}

/// The log line for a download left to the browser. A private window's site and file name are
/// left out: the log outlives the window.
pub(super) fn left_line(
    browser: &str,
    site: &str,
    name: &str,
    reason: &str,
    private: bool,
) -> String {
    let browser = if browser.is_empty() {
        "a browser"
    } else {
        browser
    };
    let what = if private {
        "a download from a private window".to_owned()
    } else {
        match (clean(name, 200), clean(site, 200)) {
            (n, s) if n.trim().is_empty() && s.trim().is_empty() => "a download".to_owned(),
            (n, s) if s.trim().is_empty() => n,
            (n, s) if n.trim().is_empty() => format!("a download from {s}"),
            (n, s) => format!("{n} from {s}"),
        }
    };
    format!("browser integration: {browser} keeps {what}: {reason}")
}

pub(super) fn log_left(
    browser: &str,
    site: &str,
    name: &str,
    reason: impl AsRef<str>,
    private: bool,
) {
    log_line(&left_line(browser, site, name, reason.as_ref(), private));
}

/// A line in DDM's log (`<data>/logs/ddm.log`, crates/crash/src/logs.rs), dated as the app's
/// own lines: the host runs apart from the app, which may not be running at all. Appended
/// whole, so it never splits a line the app writes at the same time. The log under its former
/// name (`mdm.log`, Dannex up to 0.6.5) takes today's first, as the app's start does.
pub(super) fn log_line(message: &str) {
    // Unit tests run without a data folder of their own: never into the real DDM's log
    // (tests/host.rs runs the program with its own and reads what it wrote).
    if cfg!(test) {
        return;
    }
    let Some(dir) = mdm_ipc::data_dir().map(|d| d.join("logs")) else {
        return;
    };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let line = format!(
        "{}  INFO nmhost mdm_nmhost: {message}\n",
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.6fZ")
    );
    let (old, log) = (dir.join("mdm.log"), dir.join("ddm.log"));
    if old.exists() && !log.exists() {
        let _ = std::fs::rename(&old, &log);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
    {
        let _ = f.write_all(line.as_bytes());
    }
}

/// Whether Options let the browser hand `m` over; an explicit request always may. Both
/// addresses must pass when redirects led elsewhere (an excluded site sending to a CDN).
pub(super) fn verdict(
    m: &Message,
    rules: &mdm_ipc::capture::CaptureRules,
) -> Result<(), mdm_ipc::capture::Declined> {
    if m.explicit {
        return Ok(());
    }
    let (name, browser) = (file_name_of(m), browser(m));
    rules.accepts(&m.url, &name, browser)?;
    if rules.too_small(m.size) {
        return Err(mdm_ipc::capture::Declined::Small);
    }
    match m.final_url.as_deref().filter(|f| *f != m.url) {
        Some(f) => rules.accepts(f, &name, browser),
        None => Ok(()),
    }
}

pub(super) fn to_command(m: Message) -> mdm_ipc::Command {
    let some = |s: Option<String>| s.filter(|s| !s.is_empty());
    let from_browser = Some(browser(&m).to_owned()).filter(|b| !b.is_empty());
    // The browser's own download waits for the answer, and goes on as the browser's when DDM
    // leaves it; one asked for from its right-click menu (`explicit`) has none.
    let browser_waits = !m.explicit;
    // The browser's file name, without its folder.
    let name = some(m.filename).and_then(|f| {
        f.rsplit(['\\', '/'])
            .next()
            .filter(|n| !n.is_empty())
            .map(str::to_owned)
    });
    mdm_ipc::Command {
        final_url: some(m.final_url).filter(|f| *f != m.url),
        url: Some(m.url),
        name,
        referer: some(m.referrer),
        cookies: some(m.cookies),
        cookie_jar: m.cookie_jar,
        user_agent: m.user_agent,
        from_browser,
        browser_waits,
        size: m.size.filter(|s| *s > 0),
        mime: some(m.mime),
        post_type: m.post.as_ref().and(some(m.post_type)),
        post: m.post,
        private: m.private,
        proxy: m.proxy.as_deref().and_then(browser_proxy),
        proxy_pac: m
            .proxy_pac
            .filter(|s| s.len() <= PAC_MAX && s.contains("FindProxyForURL")),
        proxy_login: m.proxy_login,
        ..Default::default()
    }
}

/// The browser's proxy (#42) if it is one the app can use: `scheme://host:port`, the scheme
/// http, https, socks4 or socks5(h), the host a name or an IP address (IPv6 in brackets), nothing
/// after the port. Anything else is dropped (the app then uses its own proxy settings).
pub(super) fn browser_proxy(s: &str) -> Option<String> {
    let s = s.trim();
    let (scheme, rest) = s.split_once("://")?;
    if !matches!(scheme, "http" | "https" | "socks4" | "socks5" | "socks5h") {
        return None;
    }
    let (host, port) = rest.rsplit_once(':')?;
    let port: u16 = port.parse().ok().filter(|p| *p > 0)?;
    let ok_host = match host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
        Some(v6) => {
            !v6.is_empty()
                && v6
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.')
        }
        None => {
            !host.is_empty()
                && host.len() <= 253
                && host
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        }
    };
    ok_host.then(|| format!("{scheme}://{host}:{port}"))
}

/// A magnet link (`magnet:?xt=...`), on one line and of a sane length.
pub(super) fn is_magnet(url: &str) -> bool {
    url.get(..8)
        .is_some_and(|s| s.eq_ignore_ascii_case("magnet:?"))
        && url.len() <= 16 * 1024
        && !url.chars().any(char::is_control)
}

/// The addresses DDM downloads (mdm_engine::supported): http(s), and ftp links sent from a page
/// (browsers no longer download ftp themselves).
pub(super) fn downloadable(url: &str) -> bool {
    let scheme = url.split_once("://").map_or("", |(s, _)| s);
    ["http", "https", "ftp", "ftps", "ftpes"]
        .iter()
        .any(|k| scheme.eq_ignore_ascii_case(k))
}

/// A link's text as the page showed it, for Link grabber's column: its runs of spaces and line
/// breaks one space, no control characters, at most [`LINK_TEXT_MAX`] characters.
pub(super) fn link_text(t: &str) -> String {
    let words: Vec<&str> = t
        .split(|c: char| c.is_whitespace() || c.is_control())
        .filter(|w| !w.is_empty())
        .collect();
    let joined = words.join(" ");
    if joined.chars().count() <= LINK_TEXT_MAX {
        return joined;
    }
    let mut cut: String = joined.chars().take(LINK_TEXT_MAX).collect();
    cut.push('…');
    cut
}
