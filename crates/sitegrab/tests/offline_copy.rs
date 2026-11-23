//! A site saved to read offline, end to end: the lab server's `/web/` site
//! (crates/labserver/src/web.rs) grabbed with the "Whole site for offline reading" template, each
//! file found saved where `offline::local_path` puts it (as the app's downloads save them), its
//! pages and style sheets changed to lead to each other, then every link of the copy followed on
//! the disk. And a site of thousands of pages, stopped and continued, each page read once.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use mdm_labserver::{Config, LabServer};
use mdm_sitegrab::offline::{self, Copy, Text};
use mdm_sitegrab::{
    Crawl, End, Found, FoundKind, Frontier, Group, Net, Phase, Spec, Status, Template,
};
use url::Url;

async fn lab() -> LabServer {
    LabServer::start(Config::default()).await.unwrap()
}

/// The engine's options with `localhost` (the lab site's "other site") on this computer.
fn net() -> Net {
    let local: IpAddr = "127.0.0.1".parse().unwrap();
    Net {
        opts: mdm_engine::Options {
            hosts: vec![("localhost".into(), vec![local])],
            // slow.html answers after 3 s: a page that times out.
            read_timeout: Duration::from_secs(1),
            ..Default::default()
        },
        ..Default::default()
    }
}

async fn run(crawl: Crawl) -> (End, Vec<Found>, Status, Option<Frontier>) {
    let (end, found, status, frontier, _) = run_with_cookies(crawl).await;
    (end, found, status, frontier)
}

async fn run_with_cookies(
    crawl: Crawl,
) -> (End, Vec<Found>, Status, Option<Frontier>, Option<String>) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
    let end = loop {
        if let Phase::Ended(end) = crawl.status().phase {
            break end;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the grab never ended"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    (
        end,
        crawl.files(),
        crawl.status(),
        crawl.frontier(),
        crawl.site_cookies(),
    )
}

fn offline_spec(lab: &LabServer, start: &str) -> Spec {
    let mut s = Spec {
        start: lab.url(start),
        delay_ms: 0,
        connections: 4,
        ..Spec::default()
    };
    Template::Offline.apply(&mut s);
    s
}

/// `found`'s address as the tests name it: its path and query, another site's after `other:`.
fn key(f: &str) -> String {
    let u = Url::parse(f).unwrap();
    let mut k = u.path().to_owned();
    if let Some(q) = u.query() {
        k = format!("{k}?{q}");
    }
    if u.host_str() == Some("localhost") {
        k = format!("other:{k}");
    }
    k
}

/// Save every file found under `dir` as the app's downloads would, each at its offline path,
/// the start site's files with its `cookies`; returns each one's file.
async fn save(
    found: &[Found],
    dir: &Path,
    port: u16,
    cookies: Option<&str>,
) -> Vec<(Found, PathBuf)> {
    let local: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let client = reqwest::Client::builder()
        .resolve("localhost", local)
        .build()
        .unwrap();
    let mut out = Vec::new();
    for f in found {
        let file = dir.join(offline::local_path(f));
        let mut req = client.get(&f.url);
        if let Some(c) = cookies
            && f.url.starts_with("http://127.0.0.1")
        {
            req = req.header(reqwest::header::COOKIE, c);
        }
        let body = req.send().await.unwrap();
        assert!(body.status().is_success(), "{} {}", f.url, body.status());
        let bytes = body.bytes().await.unwrap();
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, &bytes).unwrap();
        out.push((f.clone(), file));
    }
    out
}

/// Change the copy's pages and style sheets; how many changed.
fn make_local(saved: &[(Found, PathBuf)], copy: &Copy) -> usize {
    let mut changed = 0;
    for (f, file) in saved {
        let Some(text) = offline::text_of(file) else {
            continue;
        };
        let url = Url::parse(&f.url).unwrap();
        let bytes = std::fs::read(file).unwrap();
        let new = match text {
            Text::Html => offline::rewrite_html(&bytes, &url, file, copy),
            Text::Css => offline::rewrite_css(&bytes, &url, file, copy),
        };
        if let Some(new) = new {
            std::fs::write(file, new).unwrap();
            changed += 1;
        }
    }
    changed
}

/// Every link written in `file` (a page or a style sheet).
fn links_of(file: &Path) -> Vec<String> {
    let text = String::from_utf8_lossy(&std::fs::read(file).unwrap()).into_owned();
    match offline::text_of(file) {
        Some(Text::Html) => mdm_sitegrab::extract::html_links(&text)
            .links
            .into_iter()
            .map(|l| l.href)
            .collect(),
        Some(Text::Css) => mdm_sitegrab::extract::css_links(&text)
            .into_iter()
            .map(|(h, _)| h)
            .collect(),
        None => Vec::new(),
    }
}

#[tokio::test]
async fn a_whole_site_saved_to_read_offline() {
    let lab = lab().await;
    let port = Url::parse(&lab.url("/")).unwrap().port().unwrap();
    let spec = offline_spec(&lab, "/web/");
    assert!(!spec.sizes, "an offline copy asks no sizes first");
    let crawl = Crawl::start(&tokio::runtime::Handle::current(), &spec, net()).unwrap();
    let (end, found, status, frontier, cookies) = run_with_cookies(crawl).await;
    assert_eq!(end, End::Finished);
    assert!(
        frontier.is_none(),
        "a grab that finished leaves nothing to go on from"
    );
    // The cookie the site set while it was explored, for the downloads of its files.
    assert_eq!(cookies.as_deref(), Some("consent=yes"));
    let crawl_log = lab.log();

    let by_key: BTreeMap<String, &Found> = found.iter().map(|f| (key(&f.url), f)).collect();
    assert_eq!(by_key.len(), found.len(), "no address found twice");
    let kind = |k: &str| by_key.get(k).map(|f| f.kind);
    for page in [
        "/web/",
        "/web/about",
        "/web/docs/",
        "/web/docs/guide.html",
        "/web/docs/based.html",
        "/web/docs/deep/a/b.html",
        "/web/blog/post?id=1",
        "/web/blog/post?id=2",
        "/web/caf%C3%A9/men%C3%BC.html",
        "/web/moved/",
        "/web/proto.html",
        "/web/absolute.html",
        "/web/frame.html",
        "/web/refresh.html",
        "/web/consent",
        "/web/needs-consent.html",
    ] {
        assert_eq!(
            kind(page),
            Some(FoundKind::Page),
            "{page} in {:?}",
            by_key.keys()
        );
    }
    for style in ["/web/css/main.css", "/web/css/fonts.css"] {
        assert_eq!(kind(style), Some(FoundKind::Style), "{style}");
    }
    for file in [
        "/web/js/app.js",
        "/web/fonts/lab.woff2",
        "/web/files/report.pdf",
        "/web/img/favicon.png",
        "/web/img/styled.png",
        "/web/img/logo.png",
        "/web/img/logo@2x.png",
        "/web/img/lazy.png",
        "/web/img/hero.png",
        "/web/img/hero-small.png",
        "/web/img/inline.png",
        "/web/img/photo.png",
        "/web/img/deep.png",
        "/web/img/bg.png",
        "/web/img/post.png?v=1",
        "/web/img/post.png?v=2",
        "/web/img/consented.png",
        "/web/caf%C3%A9/%C3%B1and%C3%BA.png",
        "/web/moved/here.png",
        "other:/web/img/cdn.png",
    ] {
        assert_eq!(
            kind(file),
            Some(FoundKind::File),
            "{file} in {:?}",
            by_key.keys()
        );
    }
    for left in [
        "/web/slow.html",           // timed out
        "/web/members/",            // needs a cookie nobody gave
        "/web/private/hidden.html", // robots.txt
        "/web/redirect-me",         // found as where it leads
        "/web/img/commented.png",
        "other:/web/elsewhere.html",
    ] {
        assert!(!by_key.contains_key(left), "{left}");
    }
    // The redirect's first address is kept as another way to the page it led to.
    assert!(
        by_key["/web/moved/"]
            .also
            .iter()
            .any(|a| a.ends_with("/web/redirect-me")),
        "{:?}",
        by_key["/web/moved/"]
    );
    assert_eq!(status.robots_skipped, 1);
    assert!(status.failed >= 2, "{status:?}");
    // Each page and style sheet read once (`docs/` and `docs/#part` are one; `about` linked twice).
    let mut reads: HashMap<String, u32> = HashMap::new();
    for e in crawl_log
        .iter()
        .filter(|e| e.method == "GET" && e.path.starts_with("/web/"))
    {
        let k = format!("{}?{}", e.path, e.query.clone().unwrap_or_default());
        *reads.entry(k).or_default() += 1;
    }
    assert!(reads.values().all(|n| *n == 1), "read twice: {reads:?}");
    assert!(
        !crawl_log.iter().any(|e| e.path.contains("/img/")),
        "pictures aren't fetched while exploring"
    );

    // Saved as the downloads save them, then made to lead to each other. MDM_KEEP_OFFLINE_COPY
    // keeps the copy in that folder (to open it in a browser), else it goes with the test.
    let temp = tempfile::tempdir().unwrap();
    let kept = std::env::var_os("MDM_KEEP_OFFLINE_COPY").map(PathBuf::from);
    let dir = kept.as_deref().unwrap_or(temp.path());
    let saved = save(&found, dir, port, cookies.as_deref()).await;
    let copy = Copy::of(saved.iter().map(|(f, p)| (f, p.clone())));
    assert!(make_local(&saved, &copy) > 10);
    assert_eq!(
        make_local(&saved, &copy),
        0,
        "done twice, nothing more changes"
    );

    // The start page is there, where its address says.
    let site = dir.join(format!("127.0.0.1_{port}")).join("web");
    let index = site.join("index.html");
    assert!(index.is_file());
    let page = std::fs::read_to_string(&index).unwrap();
    let base = format!("http://127.0.0.1:{port}/web");
    for want in [
        r#"href="about.html""#.to_owned(),
        r#"href="docs/index.html""#.to_owned(),
        r##"href="docs/index.html#part""##.to_owned(),
        r#"href="blog/post@id=1.html""#.to_owned(),
        r#"href="blog/post@id=2.html""#.to_owned(),
        r#"href="caf%C3%A9/men%C3%BC.html""#.to_owned(),
        r#"href="moved/index.html""#.to_owned(),
        r#"href="proto.html""#.to_owned(),
        r#"href="absolute.html""#.to_owned(),
        format!(r#"href="{base}/slow.html""#),
        format!(r#"href="{base}/private/hidden.html""#),
        format!(r#"href="{base}/members/""#),
        format!(r#"href="http://localhost:{port}/web/elsewhere.html""#),
        r#"href="https://example.invalid/away""#.to_owned(),
        r#"href="mailto:lab@example.invalid""#.to_owned(),
        r##"href="#top""##.to_owned(),
        r#"href="img/favicon.png""#.to_owned(),
        r#"<img id="lazy" src="img/lazy.png""#.to_owned(),
        format!(r#"src="../../localhost_{port}/web/img/cdn.png""#),
    ] {
        assert!(page.contains(&want), "{want} not in\n{page}");
    }

    // Every link of every page and style sheet that leads to the disk finds its file there; none
    // that leads to the site is to a file the copy has.
    let mut followed = 0;
    for (f, file) in &saved {
        for href in links_of(file) {
            let h = href.trim();
            if h.starts_with('#') || h.is_empty() {
                continue;
            }
            if h.contains(':') {
                if let Ok(u) = Url::parse(h)
                    && matches!(u.scheme(), "http" | "https")
                {
                    let mut u = u;
                    u.set_fragment(None);
                    assert!(
                        copy.file(&u).is_none(),
                        "{} links to the site for {u}, which the copy has",
                        f.url
                    );
                }
                continue;
            }
            let path = h.split('#').next().unwrap();
            let path = percent_encoding::percent_decode_str(path).decode_utf8_lossy();
            let target = file
                .parent()
                .unwrap()
                .join(path.replace('/', std::path::MAIN_SEPARATOR_STR));
            assert!(
                target.is_file(),
                "{} links to {h}: {} isn't there",
                f.url,
                target.display()
            );
            followed += 1;
        }
    }
    assert!(followed > 50, "{followed} links followed");
    // Pictures are pictures on the disk too.
    let logo = std::fs::read(site.join("img").join("logo.png")).unwrap();
    assert!(logo.starts_with(b"\x89PNG"));
    assert!(site.join("caf\u{e9}").join("\u{f1}and\u{fa}.png").is_file());
    assert!(site.join("img").join("post@v=2.png").is_file());
}

/// A small part of a real site, politely (its robots.txt followed, one request at a time, a
/// second apart, a dozen pages): `MDM_REAL_SITE=https://www.sqlite.org/about.html` (SQLite's
/// documentation is in the public domain). Its copy is kept in MDM_KEEP_OFFLINE_COPY when set.
#[tokio::test]
#[ignore = "reads a real site; run on request with MDM_REAL_SITE"]
async fn a_real_site_saved_to_read_offline() {
    let Some(start) = std::env::var_os("MDM_REAL_SITE") else {
        return;
    };
    let mut spec = Spec {
        start: start.to_string_lossy().into_owned(),
        delay_ms: 1000,
        connections: 1,
        ..Spec::default()
    };
    Template::Offline.apply(&mut spec);
    spec.depth = 1;
    spec.max_pages = 12;
    spec.max_files = 60;
    spec.below = true;
    let (end, found, status, _) =
        run(Crawl::start(&tokio::runtime::Handle::current(), &spec, Net::default()).unwrap()).await;
    eprintln!("{end:?}: {} files, {status:?}", found.len());
    assert!(matches!(end, End::Finished | End::MaxPages | End::MaxFiles));
    let temp = tempfile::tempdir().unwrap();
    let kept = std::env::var_os("MDM_KEEP_OFFLINE_COPY").map(PathBuf::from);
    let dir = kept.as_deref().unwrap_or(temp.path());
    let client = reqwest::Client::new();
    let mut saved = Vec::new();
    for f in &found {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let Ok(resp) = client.get(&f.url).send().await else {
            continue;
        };
        if !resp.status().is_success() {
            continue;
        }
        let file = dir.join(offline::local_path(f));
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, resp.bytes().await.unwrap()).unwrap();
        saved.push((f.clone(), file));
    }
    let copy = Copy::of(saved.iter().map(|(f, p)| (f, p.clone())));
    make_local(&saved, &copy);
    assert_eq!(make_local(&saved, &copy), 0);
    let mut followed = 0;
    for (f, file) in &saved {
        for href in links_of(file) {
            let h = href.trim();
            if h.is_empty() || h.starts_with('#') || h.contains(':') {
                continue;
            }
            let path = percent_encoding::percent_decode_str(h.split('#').next().unwrap())
                .decode_utf8_lossy()
                .into_owned();
            let target = file
                .parent()
                .unwrap()
                .join(path.replace('/', std::path::MAIN_SEPARATOR_STR));
            assert!(target.is_file(), "{} links to {h}: not there", f.url);
            followed += 1;
        }
    }
    eprintln!(
        "{} files saved, {followed} links on the disk followed",
        saved.len()
    );
    assert!(followed > 0);
}

#[tokio::test]
async fn cookies_the_user_gives_open_the_pages_that_need_them() {
    let lab = lab().await;
    let mut spec = offline_spec(&lab, "/web/");
    spec.depth = 1;
    let mut n = net();
    n.cookies = Some("member=yes; theme=dark".into());
    let (end, found, _, _) =
        run(Crawl::start(&tokio::runtime::Handle::current(), &spec, n).unwrap()).await;
    assert_eq!(end, End::Finished);
    let keys: BTreeSet<String> = found.iter().map(|f| key(&f.url)).collect();
    assert!(keys.contains("/web/members/"), "{keys:?}");
    assert!(keys.contains("/web/img/member.png"));
    // Sent to the start address's site only, never to another.
    let log = lab.log();
    assert!(log.iter().filter(|e| e.path == "/web/members/").all(|e| {
        e.cookie
            .as_deref()
            .is_some_and(|c| c.contains("member=yes"))
    }));
}

#[tokio::test]
async fn a_big_site_cut_short_stopped_and_continued_reads_every_page_once() {
    let lab = lab().await;
    let mut spec = offline_spec(&lab, "/web/big/");
    spec.groups = vec![Group::Pages];
    spec.connections = 8;
    spec.max_pages = 1000;
    let rt = tokio::runtime::Handle::current();
    let started = std::time::Instant::now();
    // Most pages: it stops there, and says where it was.
    let (end, found, status, frontier) = run(Crawl::start(&rt, &spec, net()).unwrap()).await;
    assert_eq!(end, End::MaxPages);
    assert_eq!(status.pages, 1000);
    let frontier = frontier.expect("cut short: where it stood");
    assert_eq!(frontier.pages, 1000);
    assert!(!frontier.waiting.is_empty());
    // Continued with room for every page, then stopped on the way.
    spec.max_pages = 10_000;
    let crawl = Crawl::resume(&rt, &spec, net(), found, frontier).unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    while crawl.status().pages < 1500 {
        assert!(tokio::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    crawl.stop();
    let (end, found, status, frontier) = run(crawl).await;
    assert_eq!(end, End::Stopped);
    assert!(status.pages >= 1500);
    assert_eq!(
        found.len() as u32,
        status.pages,
        "every page read is a file found"
    );
    // Continued again: to the end.
    let crawl = Crawl::resume(&rt, &spec, net(), found, frontier.unwrap()).unwrap();
    let (end, found, status, frontier) = run(crawl).await;
    assert_eq!(end, End::Finished);
    assert!(frontier.is_none());
    let pages = mdm_labserver::web::BIG_PAGES + 1;
    assert_eq!(status.pages as usize, pages);
    assert_eq!(found.len(), pages);
    let unique: BTreeSet<&str> = found.iter().map(|f| f.url.as_str()).collect();
    assert_eq!(unique.len(), pages);
    // Each page asked once; only those under way when Stop came were asked again.
    let mut reads: HashMap<String, u32> = HashMap::new();
    for e in lab.log().iter().filter(|e| e.path.starts_with("/web/big/")) {
        *reads.entry(e.path.clone()).or_default() += 1;
    }
    assert_eq!(reads.len(), pages);
    let twice = reads.values().filter(|n| **n > 1).count();
    assert!(twice <= spec.connections as usize, "{twice} asked twice");
    assert!(reads.values().all(|n| *n <= 2));
    eprintln!("{pages} pages in {:?}", started.elapsed());
}
