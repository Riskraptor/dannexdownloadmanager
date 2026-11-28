//! Grabs from the lab server's small web site (crates/labserver/src/site.rs), end to end: the
//! engine's client, robots.txt, depth, scope, file types, patterns, limits, sizes, the login.

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::time::Duration;

use mdm_labserver::{Config, LabServer};
use mdm_sitegrab::{Crawl, End, Fault, Group, Net, Phase, Scope, Spec, Template};

async fn lab() -> LabServer {
    LabServer::start(Config::default()).await.unwrap()
}

/// The engine's options with `other.test` (the lab site's "other site") on this computer.
fn net() -> Net {
    let local: IpAddr = "127.0.0.1".parse().unwrap();
    Net {
        opts: mdm_engine::Options {
            hosts: vec![("other.test".into(), vec![local])],
            ..Default::default()
        },
        ..Default::default()
    }
}

fn spec(lab: &LabServer, template: Template) -> Spec {
    let mut s = Spec {
        start: lab.url("/site/"),
        delay_ms: 0,
        connections: 3,
        ..Spec::default()
    };
    template.apply(&mut s);
    s
}

/// Run a grab to its end: how it ended, and each file found by its path (`/gen/2K/photos/a.jpg`,
/// the other site's prefixed with `other:`) with its size.
async fn grab(spec: &Spec, net: Net) -> (End, BTreeMap<String, Option<u64>>, mdm_sitegrab::Status) {
    let crawl = Crawl::start(&tokio::runtime::Handle::current(), spec, net).unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
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
    let files = crawl
        .files()
        .into_iter()
        .map(|f| {
            let u = url::Url::parse(&f.url).unwrap();
            let mut key = u.path().to_owned();
            if let Some(q) = u.query() {
                key = format!("{key}?{q}");
            }
            if u.host_str() == Some("other.test") {
                key = format!("other:{key}");
            }
            (key, f.size)
        })
        .collect();
    (end, files, crawl.status())
}

fn names(files: &BTreeMap<String, Option<u64>>) -> Vec<&str> {
    files.keys().map(String::as_str).collect()
}

#[tokio::test]
async fn pictures_two_levels_deep_on_the_same_site() {
    let lab = lab().await;
    let (end, files, status) = grab(&spec(&lab, Template::Images), net()).await;
    assert_eq!(end, End::Finished);
    assert_eq!(
        names(&files),
        [
            "/gen/1K/favicon.ico",
            "/gen/1K/more.webp",
            "/gen/1K/poster.jpg",
            "/gen/2K/photos/a.jpg",
            "/gen/2K/photos/b.jpg",
            "/gen/2K/photos/level2.jpg",
            "/gen/3K/photos/a@2x.jpg",
            "/gen/5K/bg.gif",
            "/gen/6K/texture.png",
            "/gen/8K/photos/big.jpg",
            "other:/gen/4K/cdn/c.png",
        ],
        "level 3, the private folder, the other site's page, a comment's and a script's links are left"
    );
    // Sizes asked of the server (HEAD), the other site's too.
    assert_eq!(files["/gen/2K/photos/a.jpg"], Some(2048));
    assert_eq!(files["other:/gen/4K/cdn/c.png"], Some(4096));
    // index, gallery, docs and level2 (missing.html failed, the private page wasn't asked, the
    // download whose address names no file turned out a file).
    assert_eq!(status.pages, 4, "{status:?}");
    assert_eq!(status.robots_skipped, 1);
    assert_eq!(status.failed, 1);
    let log = lab.log();
    assert!(
        !log.iter().any(|e| e.path.contains("private")),
        "robots.txt kept it out"
    );
    assert!(
        !log.iter()
            .any(|e| e.path.contains("level3") || e.path.contains("elsewhere"))
    );
    // robots.txt asked once per site: this one, and the other site's for its picture's size.
    let robots: Vec<_> = log.iter().filter(|e| e.path == "/robots.txt").collect();
    assert_eq!(robots.len(), 2, "{robots:?}");
    // The files' look-ups carry no Authorization without a login.
    assert!(log.iter().all(|e| e.authorization.is_none()));
}

#[tokio::test]
async fn deeper_without_robots_and_on_every_site() {
    let lab = lab().await;
    let mut s = spec(&lab, Template::Images);
    s.depth = 3;
    s.robots = false;
    s.scope = Scope::Any;
    s.sizes = false;
    let (end, files, _) = grab(&s, net()).await;
    assert_eq!(end, End::Finished);
    for found in [
        "/gen/2K/photos/level3.jpg",
        "/gen/2K/photos/secret.jpg",
        "other:/gen/2K/photos/elsewhere.jpg",
    ] {
        assert!(files.contains_key(found), "{found} in {:?}", names(&files));
    }
    assert!(files.values().all(Option::is_none), "no sizes asked");
    assert!(!lab.log().iter().any(|e| e.path == "/robots.txt"));
}

#[tokio::test]
async fn other_types_patterns_and_a_download_without_a_name() {
    let lab = lab().await;
    // Documents behind a <base href>, and an archive whose address names no file.
    let mut s = spec(&lab, Template::Documents);
    s.groups.push(Group::Archives);
    let (_, files, _) = grab(&s, net()).await;
    assert_eq!(
        names(&files),
        [
            "/gen/10K/manual.pdf",
            "/gen/12K/notes.txt",
            "/gen/13K/table.xlsx",
            "/gen/7K?cd=bundle.zip",
        ]
    );
    // Its size from the answer that showed it was a file.
    assert_eq!(files["/gen/7K?cd=bundle.zip"], Some(7 * 1024));
    // Include and exclude patterns; custom extensions.
    let mut s = spec(&lab, Template::Images);
    s.include = "*/photos/*".into();
    s.exclude = "big".into();
    s.custom = "mp4".into();
    s.sizes = false;
    let (_, files, _) = grab(&s, net()).await;
    assert_eq!(
        names(&files),
        [
            "/gen/2K/photos/a.jpg",
            "/gen/2K/photos/b.jpg",
            "/gen/2K/photos/level2.jpg",
            "/gen/3K/photos/a@2x.jpg",
        ]
    );
    s.include.clear();
    s.groups.clear();
    let (_, files, _) = grab(&s, net()).await;
    assert_eq!(names(&files), ["/gen/20K/clip.mp4"]);
}

#[tokio::test]
async fn files_stay_on_the_site_when_asked() {
    let lab = lab().await;
    let mut s = spec(&lab, Template::Images);
    s.files_anywhere = false;
    s.sizes = false;
    let (_, files, _) = grab(&s, net()).await;
    assert!(!files.keys().any(|k| k.starts_with("other:")));
    assert!(files.contains_key("/gen/2K/photos/a.jpg"));
}

#[tokio::test]
async fn limits_stop_it_and_say_so() {
    let lab = lab().await;
    let mut s = spec(&lab, Template::Images);
    s.max_files = 3;
    s.sizes = false;
    let (end, files, _) = grab(&s, net()).await;
    assert_eq!(end, End::MaxFiles);
    assert_eq!(files.len(), 3);
    let mut s = spec(&lab, Template::Images);
    s.max_pages = 1;
    s.sizes = false;
    let (end, files, status) = grab(&s, net()).await;
    assert_eq!(end, End::MaxPages);
    assert_eq!(status.pages, 1);
    assert!(
        !files.contains_key("/gen/2K/photos/b.jpg"),
        "the gallery wasn't explored"
    );
    assert!(files.contains_key("/gen/2K/photos/a.jpg"));
}

#[tokio::test]
async fn a_site_to_read_offline_keeps_its_pages_and_styles() {
    let lab = lab().await;
    let mut s = spec(&lab, Template::Offline);
    s.sizes = false;
    let (_, files, _) = grab(&s, net()).await;
    for page in [
        "/site/",
        "/site/gallery.html",
        "/site/docs/",
        "/site/deep/level2.html",
        "/site/deep/level3.html",
        "/site/style.css",
        "/site/more.css",
        "/gen/20K/clip.mp4",
        "/gen/11K/song.mp3",
    ] {
        assert!(files.contains_key(page), "{page} in {:?}", names(&files));
    }
    assert!(
        !files.contains_key("/site/missing.html"),
        "a broken page isn't one"
    );
}

#[tokio::test]
async fn a_start_page_that_fails_says_why() {
    let lab = lab().await;
    let mut s = spec(&lab, Template::Images);
    s.start = lab.url("/site/missing.html");
    let (end, files, _) = grab(&s, net()).await;
    assert_eq!(end, End::Failed(Fault::Status(404)));
    assert!(files.is_empty());
    s.start = lab.url("/site/private/secret.html");
    let (end, _, _) = grab(&s, net()).await;
    assert_eq!(end, End::Failed(Fault::Robots));
    s.start = lab.url("/gen/7K?cd=bundle.zip");
    let (end, _, _) = grab(&s, net()).await;
    assert_eq!(end, End::Failed(Fault::NotPage));
    assert!(matches!(
        Crawl::start(
            &tokio::runtime::Handle::current(),
            &Spec {
                start: "ftp://x/".into(),
                ..Spec::default()
            },
            net()
        ),
        Err(mdm_sitegrab::Invalid::Address)
    ));
}

#[tokio::test]
async fn the_login_goes_to_the_start_site_only() {
    let lab = lab().await;
    let mut s = spec(&lab, Template::Images);
    s.sizes = true;
    let mut n = net();
    n.login = Some(("user".into(), "secret".into()));
    let (end, _, _) = grab(&s, n).await;
    assert_eq!(end, End::Finished);
    let log = lab.log();
    let to_other: Vec<_> = log
        .iter()
        .filter(|e| {
            e.host
                .as_deref()
                .is_some_and(|h| h.starts_with("other.test"))
        })
        .collect();
    assert!(!to_other.is_empty());
    assert!(
        to_other.iter().all(|e| e.authorization.is_none()),
        "{to_other:?}"
    );
    assert!(
        log.iter()
            .filter(|e| e.path.starts_with("/site/"))
            .all(|e| e
                .authorization
                .as_deref()
                .is_some_and(|a| a.starts_with("Basic"))),
        "every page of the start site asked with it"
    );
}

#[tokio::test]
async fn stop_ends_it_at_once_and_keeps_what_was_found() {
    let lab = lab().await;
    let mut s = spec(&lab, Template::Images);
    // Slow enough that it can't finish before Stop.
    s.delay_ms = 2_000;
    s.connections = 1;
    let crawl = Crawl::start(&tokio::runtime::Handle::current(), &s, net()).unwrap();
    // The start page explored (robots.txt, then the page), then stopped while it waits.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    while crawl.status().pages < 1 {
        assert!(tokio::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    crawl.stop();
    let stopped_at = tokio::time::Instant::now();
    while !crawl.ended() {
        assert!(
            stopped_at.elapsed() < Duration::from_secs(1),
            "Stop took too long"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(crawl.status().phase, Phase::Ended(End::Stopped));
    assert!(!crawl.files().is_empty(), "the start page's files stay");
}

#[tokio::test]
async fn the_pause_between_requests_is_kept() {
    let lab = lab().await;
    let mut s = spec(&lab, Template::Images);
    s.delay_ms = 150;
    s.depth = 0;
    s.sizes = false;
    let (end, _, _) = grab(&s, net()).await;
    assert_eq!(end, End::Finished);
    // robots.txt, the start page, then its style sheets: one site, one request per pause.
    let mut starts: Vec<(u64, String, u64)> = lab
        .log()
        .iter()
        .filter(|e| {
            e.host
                .as_deref()
                .is_none_or(|h| !h.starts_with("other.test"))
        })
        .map(|e| (e.t_start_ms, format!("{} {}", e.method, e.path), e.conn_req))
        .collect();
    starts.sort();
    assert!(starts.len() >= 3, "{starts:?}");
    // The server notes when it starts each request, never before the crawler sent it but on a
    // busy machine late, which shortens the gap after it. So the requests are checked over their
    // whole span (each pause of 150 ms kept, 60 ms of slack in all) and no two may come together.
    // The first request also opened its connection (`conn_req` 1), and the server sees its
    // headers only once that is set up: on a machine running the whole test suite that took 65 ms
    // more (2026-11-22), so it gets that much slack again. A crawler without the pause comes
    // hundreds of milliseconds short either way.
    let span = starts.last().unwrap().0 - starts[0].0;
    let pauses = starts.len() as u64 - 1;
    let slack = if starts[0].2 == 1 { 160 } else { 60 };
    assert!(span + slack >= pauses * 150, "{starts:?}");
    for w in starts.windows(2) {
        assert!(w[1].0 - w[0].0 >= 20, "{starts:?}");
    }
}
