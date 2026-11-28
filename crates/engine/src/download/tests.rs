use super::*;

#[test]
fn pages_lead_on_as_browsers_read_them() {
    let base = Url::parse("https://site.test/download/abc").unwrap();
    let at = |v: &str| refresh_target(v, &base).map(|(d, u)| (d.as_secs(), u.to_string()));
    assert_eq!(
        at("0; url=https://cdn.test/f.zip"),
        Some((0, "https://cdn.test/f.zip".into()))
    );
    assert_eq!(
        at("5;URL='/get/f.zip'"),
        Some((5, "https://site.test/get/f.zip".into()))
    );
    assert_eq!(
        at("3, next?a=1&amp;b=2"),
        Some((3, "https://site.test/download/next?a=1&b=2".into()))
    );
    // The wait is capped; a page that reloads itself, or leads off the web, is the answer.
    assert_eq!(at("600; url=/x").map(|a| a.0), Some(PAGE_WAIT.as_secs()));
    assert_eq!(at("30"), None);
    assert_eq!(at("0; url=abc#top"), None);
    assert_eq!(at("0; url=javascript:alert(1)"), None);
    assert_eq!(at("soon; url=/x"), None);

    let page = |html: &str| meta_refresh(html.as_bytes(), &base).map(|(_, u)| u.to_string());
    assert_eq!(
        page(r#"<html><META HTTP-EQUIV="Refresh" CONTENT="0;URL=/file.zip"></html>"#),
        Some("https://site.test/file.zip".into())
    );
    assert_eq!(
        page("<meta content='2; url=https://cdn.test/a.bin' http-equiv=refresh>"),
        Some("https://cdn.test/a.bin".into())
    );
    // Other meta tags, and attribute names inside others, don't count.
    assert_eq!(
        page(
            r#"<meta name="description" content="0; url=/x"><meta data-http-equiv="refresh" content="0; url=/y">"#
        ),
        None
    );
    assert_eq!(page("<p>window.location = '/x'</p>"), None);
}

#[test]
fn a_pages_cookies_go_on_with_it() {
    let mut h = HeaderMap::new();
    h.insert(header::COOKIE, HeaderValue::from_static("a=1; session=old"));
    let mut set = HeaderMap::new();
    set.append(
        header::SET_COOKIE,
        HeaderValue::from_static("session=new; Path=/; HttpOnly"),
    );
    set.append(
        header::SET_COOKIE,
        HeaderValue::from_static("xsrf=t; Secure"),
    );
    add_cookies(&mut h, &set_cookies(&set));
    assert_eq!(
        header_str(&h, header::COOKIE),
        Some("a=1; session=new; xsrf=t")
    );
}

/// #361: a flush that took a while pushes the next one back, on a fast disk nothing changes.
#[test]
fn a_slow_flush_makes_the_next_one_wait() {
    let s = Duration::from_secs;
    let at = |took| Synced {
        at: std::time::Instant::now(),
        bytes: 0,
        took,
    };
    let gb = 1u64 << 30;
    // 3 s for the last flush: not again before 27 s, however many bytes came.
    assert!(!at(s(3)).due(gb, 45 * gb, s(20)));
    assert!(at(s(3)).due(gb, 45 * gb, s(27)));
    // At most SYNC_GAP_MAX, whatever the flush took.
    assert!(at(s(30)).due(gb, 45 * gb, s(61)));
    // A flush of a few milliseconds: the old rules (each tick once 64 MiB have arrived).
    assert!(at(Duration::from_millis(8)).due(128 << 20, 45 * gb, s(2)));
    assert!(!at(Duration::from_millis(8)).due(1 << 20, 45 * gb, s(2)));
    // Few bytes on a fast disk: still the long wait.
    assert!(at(Duration::from_millis(8)).due(1 << 20, 45 * gb, s(30)));
}

/// #518: when the forced save at the start (no connection writing yet) takes 0.5 s on a busy
/// disk, the first save of progress still comes at the first tick, not 4.5 s later.
#[test]
fn a_forced_save_does_not_push_the_next_back() {
    let at = |took| Synced {
        at: std::time::Instant::now(),
        bytes: 0,
        took,
    };
    let (slow, tick) = (Duration::from_millis(500), Duration::from_secs(2));
    assert!(at(forced_gap(true, slow)).due(15 << 20, 24 << 20, tick));
    // A save during the transfer still does.
    assert!(!at(forced_gap(false, slow)).due(15 << 20, 24 << 20, tick));
}

#[test]
fn flush_step_scales_with_the_file() {
    assert_eq!(sync_bytes(24 << 20), 3 << 19);
    assert_eq!(sync_bytes(100 << 10), 1 << 20);
    assert_eq!(sync_bytes(10 << 30), 64 << 20);
}

#[test]
fn a_silent_connection_is_detected_in_twelve_seconds() {
    assert_eq!(
        Options::default().read_timeout,
        std::time::Duration::from_secs(12)
    );
}

#[test]
fn connections_stay_in_range() {
    let with = |n| Options {
        max_connections: n,
        ..Options::default()
    };
    assert_eq!(with(0).connections(), 1);
    assert_eq!(with(100).connections(), 100);
    assert_eq!(with(5000).connections(), MAX_CONNECTIONS_UNLOCKED);
}

#[tokio::test]
async fn network_errors_read_plainly() {
    // Nothing listens on port 1.
    let e = reqwest::get("http://127.0.0.1:1/").await.unwrap_err();
    let msg = Error::from(e).to_string();
    assert!(msg.starts_with("cannot connect to the server"), "{msg}");
    assert!(
        !msg.contains("127.0.0.1") && !msg.contains("os error"),
        "{msg}"
    );
}

#[test]
fn retry_after_seconds_or_date() {
    let mut h = HeaderMap::new();
    h.insert(header::RETRY_AFTER, HeaderValue::from_static("7"));
    assert_eq!(retry_after(&h), Some(Duration::from_secs(7)));
    let later = std::time::SystemTime::now() + Duration::from_secs(90);
    h.insert(
        header::RETRY_AFTER,
        HeaderValue::from_str(&httpdate::fmt_http_date(later)).unwrap(),
    );
    let d = retry_after(&h).unwrap();
    assert!(
        d > Duration::from_secs(80) && d <= Duration::from_secs(90),
        "{d:?}"
    );
    h.insert(header::RETRY_AFTER, HeaderValue::from_static("soon"));
    assert_eq!(retry_after(&h), None);
}

#[test]
fn dead_links_are_not_retried() {
    for s in [401, 403, 404, 410, 416] {
        assert!(
            Error::Status(StatusCode::from_u16(s).unwrap()).link_dead(),
            "{s}"
        );
    }
    for s in [500, 502] {
        assert!(
            !Error::Status(StatusCode::from_u16(s).unwrap()).link_dead(),
            "{s}"
        );
    }
    assert!(!Error::Throttled(StatusCode::SERVICE_UNAVAILABLE, None).link_dead());
}

#[test]
fn credentials_stay_with_their_host() {
    let mut h = HeaderMap::new();
    h.insert(header::COOKIE, HeaderValue::from_static("sid=1"));
    h.insert(header::AUTHORIZATION, HeaderValue::from_static("Basic x"));
    h.insert(header::REFERER, HeaderValue::from_static("https://site/"));
    let u = |s: &str| Url::parse(s).unwrap();
    let same = scoped_headers(&h, &u("https://site/a"), &u("https://site/b"));
    assert!(same.contains_key(header::COOKIE) && same.contains_key(header::AUTHORIZATION));
    let cdn = scoped_headers(&h, &u("https://site/a"), &u("https://cdn.example/a"));
    assert!(!cdn.contains_key(header::COOKIE) && !cdn.contains_key(header::AUTHORIZATION));
    assert!(
        cdn.contains_key(header::REFERER),
        "non-credential headers stay"
    );
    let down = scoped_headers(&h, &u("https://site/a"), &u("http://site/a"));
    assert!(
        !down.contains_key(header::COOKIE),
        "no credentials over a https -> http downgrade"
    );
    let port = scoped_headers(&h, &u("http://site/a"), &u("http://site:8080/a"));
    assert!(
        !port.contains_key(header::COOKIE),
        "another port is another origin"
    );
}

#[test]
fn a_drive_links_cookies_go_to_drives_own_hosts_only() {
    let mut h = HeaderMap::new();
    h.insert(header::COOKIE, HeaderValue::from_static("SID=1"));
    let u = |s: &str| Url::parse(s).unwrap();
    let link = u("https://drive.google.com/file/d/x/view");
    for to in [
        "https://drive.usercontent.google.com/download?id=x",
        "https://docs.google.com/document/d/x/export",
    ] {
        assert!(
            scoped_headers(&h, &link, &u(to)).contains_key(header::COOKIE),
            "{to}"
        );
    }
    for to in [
        "https://accounts.google.com/ServiceLogin",
        "https://doc-0.docs.googleusercontent.com/export/x",
        "http://drive.usercontent.google.com/download?id=x",
    ] {
        assert!(
            !scoped_headers(&h, &link, &u(to)).contains_key(header::COOKIE),
            "{to}"
        );
    }
}

#[test]
fn drive_gets_a_browsers_agent_and_only_drive() {
    let u = |s: &str| Url::parse(s).unwrap();
    let browser = Some(sites::gdrive::USER_AGENT);
    // DDM's automatic agent (the default browser's, #668), by default.
    let auto = Options::default();
    assert!(auto.automatic_agent && auto.site_agent());
    for drive in [
        "https://drive.google.com/file/d/x/view",
        "https://drive.usercontent.google.com/download?id=x",
        "http://docs.google.com:8080/document/d/x/export",
    ] {
        assert_eq!(
            request_agent(auto.site_agent(), &u(drive)),
            browser,
            "{drive}"
        );
    }
    for other in [
        "https://example.com/a.zip",
        "https://accounts.google.com/ServiceLogin",
        "https://doc-0.docs.googleusercontent.com/export/x",
        "https://drive.google.com.evil.test/x",
    ] {
        assert_eq!(request_agent(auto.site_agent(), &u(other)), None, "{other}");
    }
    // The browser's own agent (handed over with the download) or the user's wins, even
    // when it is the same text as the automatic one.
    for own in [
        "Mozilla/5.0 Firefox/140.0".to_owned(),
        crate::agent::automatic(),
    ] {
        let mut opts = Options::default();
        opts.speak_as(own);
        assert!(!opts.site_agent());
        assert_eq!(
            request_agent(opts.site_agent(), &u("https://drive.google.com/x")),
            None
        );
    }
    // DDM's former agent, asked with when a browser's is refused (#372), still gets it.
    let mut fallback = Options::default();
    fallback.speak_as(crate::agent::IE11_USER_AGENT.to_owned());
    assert!(fallback.site_agent());
}

#[test]
fn content_range() {
    let mut h = HeaderMap::new();
    h.insert(
        header::CONTENT_RANGE,
        HeaderValue::from_static("bytes 0-99/1000"),
    );
    assert_eq!(parse_content_range(&h).unwrap(), (0, 99, Some(1000)));
    h.insert(
        header::CONTENT_RANGE,
        HeaderValue::from_static("bytes 5-9/*"),
    );
    assert_eq!(parse_content_range(&h).unwrap(), (5, 9, None));
    h.insert(
        header::CONTENT_RANGE,
        HeaderValue::from_static("items 0-1/2"),
    );
    assert!(parse_content_range(&h).is_err());
}
