//! #769: a gallery's files come down through DDM's engine, against a fake gallery-dl and the lab
//! server. gallery-dl lists the files (their addresses, the names it gives them, the headers its
//! extractor wants), DDM's engine downloads them into those very places, several at once, under
//! the gallery's speed limit (a change applying at once), paused and resumed like a file; what
//! the engine can't download (a video gallery-dl hands to yt-dlp, a text, a file the engine is
//! refused) gallery-dl downloads itself afterwards, skipping every file already there; a gallery
//! downloaded again skips them all. A gallery-dl that can't list them downloads them itself.
//!
//! One program, two roles (no test harness, `harness = false`):
//! - run as `fake-gallery-dl[.exe]` (a copy of itself): it is gallery-dl, as far as DDM drives
//!   it (`gallery.json` beside it says the gallery's files and how it behaves);
//! - run plainly (`cargo test`): it runs the checks.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use mdm_labserver::{Config, LabServer, content};
use mdm_tools::gallery::{Ask, GalleryJob, USER_AGENT};
use mdm_tools::video::{Progress, State};
use serde_json::{Value, json};

fn main() -> ExitCode {
    let me = std::env::current_exe().expect("own path");
    let name = me.file_stem().map(|s| s.to_string_lossy().into_owned());
    if name.as_deref() == Some("fake-gallery-dl") {
        return fake_gallery_dl();
    }
    checks(&me);
    println!("fake_gallery_dl: ok");
    ExitCode::SUCCESS
}

/// The gallery's files and the fake's ways, from `gallery.json` beside it:
/// `{"items": [{"url", "name", "ext", "headers"}], "no_print": bool, "no_meta": bool,
/// "slow": ms, "code_page": bool, "code_page_always": bool, "quiet_print": bool}` (`slow`: the
/// time it takes for each file it downloads itself; `code_page`: its path lines as Windows'
/// gallery-dl.exe writes them without `-o output.stdout=utf-8`, `?` for each letter outside
/// ASCII; `code_page_always`: so even with it; `quiet_print`: `--print` without the path lines,
/// as from 1.30.0).
fn gallery(home: &Path) -> Value {
    let text = std::fs::read_to_string(home.join("gallery.json")).expect("gallery.json");
    serde_json::from_str(&text).expect("gallery.json is JSON")
}

/// gallery-dl as DDM drives it: `--directory`, `--range 1-N`, the browser's `--user-agent`, and
/// either the listing (`--no-download`, `--print file:FORMAT` with `-o url-metadata=…`,
/// `-o path-metadata=…`) or its own download, skipping what is there (`# path`) and printing
/// each file it saves. Each run's arguments are added to `runs.txt` beside it.
fn fake_gallery_dl() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let home = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_owned))
        .expect("the fake's folder");
    {
        use std::io::Write;
        let mut runs = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(home.join("runs.txt"))
            .unwrap();
        writeln!(runs, "{}", json!(args)).unwrap();
    }
    let after = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let has = |flag: &str| args.iter().any(|a| a == flag);
    // Whether it reads an address (`gallery::handles`, before the command line's download).
    if has("--extractor-info") {
        return ExitCode::SUCCESS;
    }
    let options: Vec<&str> = args
        .iter()
        .enumerate()
        .filter(|(i, _)| *i > 0 && args[i - 1] == "-o")
        .map(|(_, a)| a.as_str())
        .collect();
    let g = gallery(&home);
    let flag = |k: &str| g[k].as_bool().unwrap_or(false);
    let code_page = flag("code_page")
        && (flag("code_page_always") || !options.contains(&"output.stdout=utf-8"));
    let shown = |p: &Path| -> String {
        let s = p.display().to_string();
        if code_page {
            s.chars()
                .map(|c| if c.is_ascii() { c } else { '?' })
                .collect()
        } else {
            s
        }
    };
    let print = after("--print");
    if print.is_some() && flag("no_print") {
        eprintln!("usage: gallery-dl [OPTION]... URL...");
        eprintln!("gallery-dl: error: unrecognized arguments: --print");
        return ExitCode::from(2);
    }
    let dir = PathBuf::from(after("--directory").expect("--directory"));
    let limit = after("--range")
        .and_then(|r| r.strip_prefix("1-").map(str::to_owned))
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or(usize::MAX);
    let agent = after("--user-agent").unwrap_or_else(|| "gallery-dl/1.30".into());
    let items = g["items"].as_array().cloned().unwrap_or_default();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut failed = false;
    for item in items.iter().take(limit) {
        let url = item["url"].as_str().unwrap();
        let path = dir.join(item["name"].as_str().unwrap());
        if path.exists() {
            println!("# {}", shown(&path));
            continue;
        }
        if has("--no-download") {
            if let Some(format) = print.as_deref().and_then(|p| p.strip_prefix("file:")) {
                let given = |o: &str| options.contains(&o);
                let field = |on: bool, v: Value| if on { v.to_string() } else { "None".into() };
                let headers = if item["headers"].is_object() {
                    item["headers"].to_string()
                } else {
                    "None".into()
                };
                let line = format
                    .replace(
                        "{gdl_url!j}",
                        &field(
                            given("url-metadata=gdl_url") && !flag("no_meta"),
                            json!(url),
                        ),
                    )
                    .replace(
                        "{gdl_path.path!j}",
                        &field(
                            given("path-metadata=gdl_path") && !flag("no_meta"),
                            json!(path.to_string_lossy()),
                        ),
                    )
                    .replace("{extension!j}", &item["ext"].to_string())
                    .replace("{_http_headers!j}", &headers);
                println!("{line}");
            }
            if !flag("quiet_print") {
                println!("{}", shown(&path));
            }
            continue;
        }
        // Downloading itself: with its own session (its cookie), as slowly as it takes.
        if let Some(ms) = g["slow"].as_u64() {
            std::thread::sleep(Duration::from_millis(ms));
        }
        let body: Option<Vec<u8>> = if let Some(text) = url.strip_prefix("text:") {
            Some(text.as_bytes().to_vec())
        } else if url.starts_with("ytdl:") {
            std::thread::sleep(Duration::from_millis(300));
            Some(b"a video yt-dlp downloaded".to_vec())
        } else {
            rt.block_on(async {
                let mut req = reqwest::Client::new()
                    .get(url)
                    .header("User-Agent", &agent)
                    .header("Cookie", "sid=1");
                if let Some(h) = item["headers"].as_object() {
                    for (k, v) in h {
                        req = req.header(k, v.as_str().unwrap());
                    }
                }
                let resp = req.send().await.ok()?;
                if !resp.status().is_success() {
                    eprintln!("ERROR: [fake] HTTP {} for {url}", resp.status());
                    return None;
                }
                resp.bytes().await.ok().map(|b| b.to_vec())
            })
        };
        match body {
            Some(b) => {
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(&path, b).unwrap();
                println!("{}", shown(&path));
            }
            None => failed = true,
        }
    }
    if failed {
        ExitCode::from(4)
    } else {
        ExitCode::SUCCESS
    }
}

/// The fake, as a copy of this program named `fake-gallery-dl` in `dir`, with `gallery`.
fn fake_in(me: &Path, dir: &Path, gallery: &Value) -> mdm_tools::Installed {
    fake_as(me, dir, gallery, "1.30.0")
}

fn fake_as(me: &Path, dir: &Path, gallery: &Value, version: &str) -> mdm_tools::Installed {
    let exe = dir.join(format!("fake-gallery-dl{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(me, &exe).expect("copy the fake");
    std::fs::write(dir.join("gallery.json"), gallery.to_string()).unwrap();
    mdm_tools::Installed {
        version: version.into(),
        exe,
    }
}

/// The fake's runs so far, each one's arguments.
fn runs(dir: &Path) -> Vec<Vec<String>> {
    std::fs::read_to_string(dir.join("runs.txt"))
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn log_of(p: &Progress) -> Vec<String> {
    p.events.iter().map(|(_, t)| t.clone()).collect()
}

/// `size` bytes of the lab's content for `seed`.
fn bytes(seed: u64, size: usize) -> Vec<u8> {
    let mut v = vec![0; size];
    content::fill(seed, 0, &mut v);
    v
}

/// Every file under `dir`, recursively.
fn all_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap().filter_map(Result::ok) {
        let p = e.path();
        if p.is_dir() {
            out.extend(all_files(&p));
        } else {
            out.push(p);
        }
    }
    out
}

/// What a job did until it ended: its end, the most connections seen at once, and whether
/// gallery-dl was seen downloading itself.
struct Ended {
    p: Progress,
    most: usize,
    helper: bool,
}

fn wait(job: &GalleryJob) -> Ended {
    let end = Instant::now() + Duration::from_secs(90);
    let (mut most, mut helper) = (0, false);
    loop {
        let p = job.progress();
        if let Some(e) = &p.engine {
            most = most.max(e.connections.len());
        }
        helper |= job.helper_running();
        if p.state != State::Running {
            return Ended { p, most, helper };
        }
        assert!(Instant::now() < end, "still running: {:?}", log_of(&p));
        std::thread::sleep(Duration::from_millis(20));
    }
}

const PAGE: &str = "https://gallery.example/album/birds";

fn checks(me: &Path) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let lab = rt.block_on(LabServer::start(Config::default())).unwrap();
    let engine = |connections: u32, limit: Option<u64>| mdm_tools::streams::Engine {
        handle: rt.handle().clone(),
        opts: mdm_engine::Options {
            max_connections: connections,
            retry_delay: Duration::from_millis(50),
            max_retries: 2,
            speed_limit: limit,
            ..Default::default()
        },
    };
    let start = |tool: &mdm_tools::Installed, folder: &Path, ask: &Ask, connections: u32| {
        GalleryJob::start(
            tool,
            PAGE,
            folder,
            ask,
            &Default::default(),
            Some(engine(connections, None)),
        )
        .expect("started")
    };

    // The engine's files in gallery-dl's names and sub-folders, with its headers and the
    // site as their Referer; what it can't download, gallery-dl's.
    let tools = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let folder = out.path().join("gallery.example - birds");
    let g = json!({"items": [
        {"url": lab.url("/gen/512K/rhea.jpg?seed=1&rate=1M&require_referer=https://gallery.example/"),
         "name": "Rhea (de11b288).jpg", "ext": "jpg", "headers": null},
        {"url": lab.url("/gen/768K/owl.png?seed=2&rate=1M&require_referer=https://cdn.example/"),
         "name": "Owl.png", "ext": "png", "headers": {"Referer": "https://cdn.example/owl"}},
        {"url": lab.url("/gen/256K/heron.jpg?seed=3&rate=1M"),
         "name": "2024/Heron.jpg", "ext": "jpg", "headers": null},
        {"url": "ytdl:https://video.example/clip", "name": "clip", "ext": null, "headers": null},
        {"url": "text:A caption of the album", "name": "caption.txt", "ext": "txt",
         "headers": null},
        {"url": lab.url("/gen/128K/members.jpg?seed=4&require_cookie=sid=1"),
         "name": "members.jpg", "ext": "jpg", "headers": null}
    ]});
    let tool = fake_in(me, tools.path(), &g);
    let job = start(&tool, &folder, &Ask::default(), 4);
    let ended = wait(&job);
    let (p, log) = (&ended.p, log_of(&ended.p));
    assert_eq!(p.state, State::Done(folder.clone()), "{log:?}");
    let read = |name: &str| std::fs::read(folder.join(name)).unwrap();
    assert!(read("Rhea (de11b288).jpg") == bytes(1, 512 << 10), "rhea");
    assert!(read("Owl.png") == bytes(2, 768 << 10), "owl");
    assert!(
        read("2024/Heron.jpg") == bytes(3, 256 << 10),
        "heron, in its sub-folder"
    );
    assert_eq!(read("clip"), b"a video yt-dlp downloaded");
    assert_eq!(read("caption.txt"), b"A caption of the album");
    assert!(read("members.jpg") == bytes(4, 128 << 10), "members");
    assert!(
        all_files(&folder)
            .iter()
            .all(|f| !f.to_string_lossy().contains(".ddm.")),
        "no part file or state left: {:?}",
        all_files(&folder)
    );
    assert!(ended.most >= 2, "several at once: {}", ended.most);
    assert!(ended.helper, "gallery-dl was seen downloading itself");
    let files = job.files();
    assert_eq!(
        (files.done, files.listed, files.by_gallery_dl, files.listing),
        (6, Some(6), 3, false)
    );
    assert_eq!(job.saved().len(), 6);
    assert_eq!(job.known_files().len(), 6);
    assert!(
        log.iter()
            .any(|t| t == "clip: gallery-dl downloads it itself (a video it hands to yt-dlp)"),
        "{log:?}"
    );
    assert!(
        log.iter()
            .any(|t| t == "caption.txt: gallery-dl downloads it itself (a text it writes)"),
        "{log:?}"
    );
    assert!(
        log.iter().any(|t| t.starts_with(
            "members.jpg: gallery-dl downloads it itself (DDM's downloader couldn't download it:"
        )),
        "{log:?}"
    );
    assert!(
        log.iter()
            .any(|t| t == "gallery-dl listed 6 files (0 already there)"),
        "{log:?}"
    );
    assert!(
        log.iter().any(|t| t
            == "Saved 6 files (0 already there); gallery-dl downloaded 3 of them itself"),
        "{log:?}"
    );
    // gallery-dl listed them, then downloaded what was left itself, its usual way.
    let r = runs(tools.path());
    assert_eq!(r.len(), 2, "{r:?}");
    assert!(r[0].iter().any(|a| a == "--no-download"));
    assert!(
        !r[0].iter().any(|a| a == "--sleep"),
        "no wait while listing"
    );
    assert!(r[1].iter().any(|a| a == "--sleep") && !r[1].iter().any(|a| a == "--no-download"));
    rt.block_on(lab.settled());
    let log = lab.log();
    let of = |name: &str| -> Vec<_> {
        log.iter()
            .filter(|e| e.path.ends_with(name))
            .cloned()
            .collect()
    };
    // The engine's (gallery-dl's own session has its cookie), as gallery-dl asks: DDM's
    // User-Agent, the site or the extractor's own as the Referer.
    for (name, referer) in [
        ("rhea.jpg", "https://gallery.example/"),
        ("owl.png", "https://cdn.example/owl"),
        ("heron.jpg", "https://gallery.example/"),
    ] {
        let reqs = of(name);
        assert!(!reqs.is_empty(), "{name}");
        assert!(
            reqs.iter().all(|e| e.cookie.is_none()
                && e.user_agent.as_deref() == Some(USER_AGENT)
                && e.referer.as_deref() == Some(referer)),
            "{name}: {reqs:?}"
        );
    }
    // Refused to the engine (no session), then gallery-dl's with its session.
    let members = of("members.jpg");
    assert!(members.iter().any(|e| e.cookie.is_none()), "{members:?}");
    assert!(
        members.iter().any(|e| e.cookie.as_deref() == Some("sid=1")),
        "{members:?}"
    );

    // Downloaded again: every file is there, so nothing is fetched, and gallery-dl isn't
    // started a second time to download.
    lab.clear_log();
    let job = start(&tool, &folder, &Ask::default(), 4);
    let ended = wait(&job);
    let log = log_of(&ended.p);
    assert_eq!(ended.p.state, State::Done(folder.clone()), "{log:?}");
    assert!(
        log.iter().any(|t| t == "Saved 0 files (6 already there)"),
        "{log:?}"
    );
    assert_eq!(job.files().done, 6);
    rt.block_on(lab.settled());
    assert!(
        lab.log().iter().all(|e| !e.path.starts_with("/gen/")),
        "{:?}",
        lab.log()
    );
    assert_eq!(runs(tools.path()).len(), 3, "the listing only");

    // At most so many files: the first ones gallery-dl lists.
    let (tools, out) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let tool = fake_in(me, tools.path(), &g);
    let job = start(
        &tool,
        out.path(),
        &Ask {
            limit: Some(2),
            rate: None,
        },
        4,
    );
    let ended = wait(&job);
    assert_eq!(ended.p.state, State::Done(out.path().to_owned()));
    let mut names: Vec<String> = all_files(out.path())
        .iter()
        .map(|f| f.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["Owl.png", "Rhea (de11b288).jpg"]);
    assert!(
        runs(tools.path())[0]
            .windows(2)
            .any(|w| w == ["--range", "1-2"])
    );

    limit_and_resume_checks(me, &rt, &lab, &engine);
    older_gallery_dl_checks(me, &lab);
    code_page_checks(me, &lab);
}

/// The gallery's speed limit is one for all its files, and a change applies at once; stopped,
/// each file keeps where it got to, and started again it goes on from there.
fn limit_and_resume_checks(
    me: &Path,
    rt: &tokio::runtime::Runtime,
    lab: &LabServer,
    engine: &dyn Fn(u32, Option<u64>) -> mdm_tools::streams::Engine,
) {
    let (tools, out) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let items: Vec<Value> = (0..3)
        .map(|i| {
            json!({"url": lab.url(&format!("/gen/2M/big{i}.bin?seed={}&rate=1M", 10 + i)),
                   "name": format!("big{i}.bin"), "ext": "bin", "headers": null})
        })
        .collect();
    let tool = fake_in(me, tools.path(), &json!({ "items": items }));
    let start = || {
        GalleryJob::start(
            &tool,
            PAGE,
            out.path(),
            &Ask::default(),
            &Default::default(),
            Some(engine(2, None)),
        )
        .expect("started")
    };
    let job = start();
    while job.progress().downloaded < 1 << 20 {
        assert_eq!(
            job.progress().state,
            State::Running,
            "{:?}",
            log_of(&job.progress())
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    // Two files at once at 1 MB/s each; 256 KB/s for both together from now.
    job.set_speed_limit(Some(256 * 1024));
    assert_eq!(job.speed_limit(), Some(256 * 1024));
    std::thread::sleep(Duration::from_millis(600));
    let (a, t) = (job.progress().downloaded, Instant::now());
    std::thread::sleep(Duration::from_secs(2));
    let rate = (job.progress().downloaded - a) as f64 / t.elapsed().as_secs_f64();
    assert!(
        rate < 400.0 * 1024.0,
        "one limit for the whole gallery: {rate:.0} B/s"
    );
    assert!(!job.helper_running());
    job.stop();
    let ended = wait(&job);
    assert_eq!(ended.p.state, State::Stopped, "{:?}", log_of(&ended.p));
    let states: Vec<_> = all_files(out.path())
        .into_iter()
        .filter(|f| f.to_string_lossy().ends_with(".ddm.json"))
        .collect();
    assert!(!states.is_empty(), "the resume states are kept");
    let job = start();
    let ended = wait(&job);
    let log = log_of(&ended.p);
    assert_eq!(ended.p.state, State::Done(out.path().to_owned()), "{log:?}");
    for i in 0..3 {
        assert!(
            std::fs::read(out.path().join(format!("big{i}.bin"))).unwrap()
                == bytes(10 + i, 2 << 20),
            "big{i}.bin whole after the stop"
        );
    }
    assert!(log.iter().any(|t| t.ends_with(": resuming")), "{log:?}");
    rt.block_on(lab.settled());
    assert!(
        lab.log().iter().any(|e| e.path.starts_with("/gen/2M/big")
            && e.range
                .as_deref()
                .is_some_and(|r| !r.starts_with("bytes=0-"))),
        "a file went on from where it stopped"
    );
    assert!(
        all_files(out.path())
            .iter()
            .all(|f| !f.to_string_lossy().contains(".ddm.")),
        "no part file or state left"
    );
}

/// Names outside ASCII among the files gallery-dl downloads itself and those already there, as
/// Windows' gallery-dl.exe writes its lines (#769, found with a real one on Wikimedia Commons:
/// its console's code page, `?` for what it can't, whatever PYTHONIOENCODING says): with
/// `-o output.stdout=utf-8` its lines name them right; one whose lines name them so all the
/// same (a release before 1.25.0 ignores the option, though it couldn't list the files) still
/// has each file it saved counted, looked for on the disk, never one "couldn't be downloaded".
fn code_page_checks(me: &Path, lab: &LabServer) {
    for always in [false, true] {
        let (tools, out) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        // Already there, under a name in another script: counted once, as there.
        std::fs::write(out.path().join("Гора.jpg"), b"the user's").unwrap();
        let gallery = json!({"code_page": true, "code_page_always": always, "quiet_print": always, "items": [
            {"url": lab.url("/gen/16K/a.jpg?seed=31"), "name": "Café.jpg", "ext": "jpg", "headers": null},
            {"url": "text:caption", "name": "Légende ü.txt", "ext": "txt", "headers": null},
            {"url": lab.url("/gen/16K/b.jpg?seed=32"), "name": "Гора.jpg", "ext": "jpg", "headers": null}
        ]});
        let tool = fake_in(me, tools.path(), &gallery);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let job = GalleryJob::start(
            &tool,
            PAGE,
            out.path(),
            &Ask::default(),
            &Default::default(),
            Some(mdm_tools::streams::Engine {
                handle: rt.handle().clone(),
                opts: Default::default(),
            }),
        )
        .expect("started");
        let ended = wait(&job);
        let log = log_of(&ended.p);
        assert_eq!(
            ended.p.state,
            State::Done(out.path().to_owned()),
            "always {always}: {log:?}"
        );
        assert_eq!(
            std::fs::read(out.path().join("Légende ü.txt")).unwrap(),
            b"caption"
        );
        assert_eq!(
            std::fs::read(out.path().join("Гора.jpg")).unwrap(),
            b"the user's"
        );
        assert!(
            !log.iter().any(|t| t.contains("couldn't be downloaded")),
            "always {always}: {log:?}"
        );
        assert!(
            log.iter()
                .any(|t| t
                    == "Saved 2 files (1 already there); gallery-dl downloaded 1 of them itself"),
            "always {always}: {log:?}"
        );
        let r = runs(tools.path());
        assert!(
            r.iter()
                .all(|a| a.windows(2).any(|w| w == ["-o", "output.stdout=utf-8"])),
            "{r:?}"
        );
    }
}

/// A gallery-dl that can't list the files for the engine downloads them all itself, as before:
/// one older than `--print` (DDM doesn't ask it), one that refuses the option (argparse), one
/// that doesn't fill in the addresses.
fn older_gallery_dl_checks(me: &Path, lab: &LabServer) {
    // Each file it downloads itself taking a moment, as a real one's do: the job is seen
    // running it (a fast computer's fake was done between two looks).
    let small = json!({"slow": 150, "items": [
        {"url": lab.url("/gen/64K/a.jpg?seed=21"), "name": "a.jpg", "ext": "jpg", "headers": null},
        {"url": "text:hello", "name": "b.txt", "ext": "txt", "headers": null}
    ]});
    for (version, extra, said) in [
        ("1.25.0", json!({}), None),
        (
            "system",
            json!({"no_print": true}),
            Some("This gallery-dl can't list the files for DDM's downloader"),
        ),
        (
            "1.30.0",
            json!({"no_meta": true}),
            Some("gallery-dl listed no addresses for DDM's downloader"),
        ),
    ] {
        let (tools, out) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let mut gallery = small.clone();
        for (k, v) in extra.as_object().unwrap() {
            gallery[k] = v.clone();
        }
        let tool = fake_as(me, tools.path(), &gallery, version);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let job = GalleryJob::start(
            &tool,
            PAGE,
            out.path(),
            &Ask::default(),
            &Default::default(),
            Some(mdm_tools::streams::Engine {
                handle: rt.handle().clone(),
                opts: Default::default(),
            }),
        )
        .expect("started");
        let ended = wait(&job);
        let log = log_of(&ended.p);
        assert_eq!(
            ended.p.state,
            State::Done(out.path().to_owned()),
            "{version}: {log:?}"
        );
        assert!(
            std::fs::read(out.path().join("a.jpg")).unwrap() == bytes(21, 64 << 10),
            "{version}"
        );
        assert_eq!(std::fs::read(out.path().join("b.txt")).unwrap(), b"hello");
        assert!(ended.helper, "{version}: gallery-dl downloaded them itself");
        assert_eq!(job.files().listed, None, "{version}");
        assert!(
            log.iter()
                .any(|t| t == "gallery-dl saved 2 files (0 already there)"),
            "{version}: {log:?}"
        );
        if let Some(said) = said {
            assert!(
                log.iter().any(|t| t.starts_with(said)),
                "{version}: {log:?}"
            );
        }
        let r = runs(tools.path());
        let downloads = r
            .iter()
            .filter(|a| !a.contains(&"--no-download".to_owned()));
        assert_eq!(downloads.count(), 1, "{version}: {r:?}");
        assert_eq!(
            r.len(),
            if said.is_some() { 2 } else { 1 },
            "{version}: listed first only when it may list them: {r:?}"
        );
    }
}
