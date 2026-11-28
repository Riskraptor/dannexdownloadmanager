//! Site grabber in the real app, off the screens, with a data folder and a download folder of its
//! own: a project grabbing the lab server's `/web/` site (crates/labserver/src/web.rs) with the
//! "Whole site for offline reading" template (`/show site-grabber-run:<address>`, as the wizard's
//! Start does), then its files added as one batch (`/show site-grabber-download`, as Download
//! all). They download into one folder, no window opens for any of them, they show under the
//! project rather than All Downloads, the project keeps what it found, and once downloaded the
//! copy's pages lead to each other on the disk, with a page at the folder's top that opens it.
//! And a window replaced while its project explores (as closing it) leaves the grab going: its
//! files are kept in the project when it ends.
//!
//! It opens windows, which needs OpenGL or Direct3D, so it runs on request:
//! `cargo test -p mdm-app --test site_grab -- --ignored`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const APP: &str = env!("CARGO_BIN_EXE_dannexdownloadmanager");

fn log(data: &Path) -> String {
    std::fs::read_to_string(data.join("logs").join("ddm.log")).unwrap_or_default()
}

fn second_launch(data: &Path, downloads: &Path, what: &str) {
    let status = Command::new(APP)
        .args(["/show", what])
        .env("MDM_DATA_DIR", data)
        .env("MDM_OFFSCREEN", "1")
        .env("MDM_DOWNLOADS", downloads)
        .status()
        .unwrap();
    assert!(status.success(), "/show {what}: {status}");
}

fn wait_for(what: &str, data: &Path, secs: u64, done: impl Fn() -> bool) {
    let until = Instant::now() + Duration::from_secs(secs);
    while !done() {
        assert!(Instant::now() < until, "no {what}:\n{}", log(data));
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn json(path: &Path) -> Option<serde_json::Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// The app with a list of its own, off the screens; `run` while it runs, then it is closed.
fn with_app(run: impl FnOnce(&Path, &Path)) {
    let dir = tempfile::tempdir().unwrap();
    let (data, dl) = (dir.path().join("data"), dir.path().join("dl"));
    for d in [&data, &dl] {
        std::fs::create_dir_all(d).unwrap();
    }
    // General's folder (where an offline copy goes) is the test's own, never the computer's
    // Downloads.
    let settings = serde_json::json!({
        "language": "en", "update_check": false, "helpers_asked": true, "language_asked": true,
        "extension_never_ask": true, "clipboard_autostart": false, "torrent_dht": false,
        "torrent_lsd": false, "torrent_upnp": false, "torrent_incoming": false,
        "category_dirs": [["General", dl.display().to_string()]],
    });
    std::fs::write(
        data.join("downloads.json"),
        serde_json::json!({ "next_id": 1, "items": [], "version": 2, "settings": settings })
            .to_string(),
    )
    .unwrap();
    let mut app = Command::new(APP)
        .env("MDM_DATA_DIR", &data)
        .env("MDM_OFFSCREEN", "1")
        .env("MDM_DOWNLOADS", &dl)
        .spawn()
        .unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        wait_for("main window", &data, 60, || {
            log(&data).contains("window opened: main")
        });
        run(&data, &dl);
    }));
    let _ = Command::new(APP)
        .arg("/exit")
        .env("MDM_DATA_DIR", &data)
        .env("MDM_OFFSCREEN", "1")
        .status();
    let until = Instant::now() + Duration::from_secs(15);
    while app.try_wait().unwrap().is_none() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(200));
    }
    let _ = app.kill();
    let _ = app.wait();
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

fn lab() -> (tokio::runtime::Runtime, mdm_labserver::LabServer) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let lab = rt
        .block_on(mdm_labserver::LabServer::start(Default::default()))
        .unwrap();
    (rt, lab)
}

/// The first project's grab ended (its files found are saved with it).
fn grabbed(data: &Path) -> Option<serde_json::Value> {
    let g = json(&data.join("grabber.json"))?;
    let p = g["projects"].get(0)?.clone();
    (!p["ended"].is_null()).then_some(p)
}

#[test]
#[ignore = "opens windows; run on request"]
fn a_site_grabbed_offline_downloads_as_one_batch_and_reads_from_the_disk() {
    let (_rt, lab) = lab();
    let port = url::Url::parse(&lab.url("/")).unwrap().port().unwrap();
    with_app(|data, dl| {
        second_launch(data, dl, &format!("site-grabber-run:{}", lab.url("/web/")));
        wait_for("the grab's end", data, 90, || grabbed(data).is_some());
        let project = grabbed(data).unwrap();
        assert_eq!(project["ended"], "Finished", "{project}");
        let found = project["found"].as_array().unwrap().len();
        assert!(found > 30, "{found} files found");
        assert_eq!(project["offline"], true);
        assert_eq!(project["listed"], false);
        // Into the test's folder (checked before anything is downloaded).
        assert!(
            Path::new(project["folder"].as_str().unwrap()).starts_with(dl),
            "{}",
            project["folder"]
        );

        second_launch(data, dl, "site-grabber-download");
        // Every file downloaded (or failed: the other site's picture, if this computer's
        // `localhost` isn't the lab's).
        let items = || {
            json(&data.join("downloads.json"))
                .and_then(|j| j["items"].as_array().cloned())
                .unwrap_or_default()
        };
        wait_for("every download's end", data, 120, || {
            let items = items();
            items.len() == found
                && items
                    .iter()
                    .all(|i| i["status"] == "Complete" || i["status"] == "Error")
        });
        let items = items();
        let failed: Vec<&serde_json::Value> =
            items.iter().filter(|i| i["status"] != "Complete").collect();
        assert!(
            failed
                .iter()
                .all(|i| i["url"].as_str().unwrap().contains("localhost")),
            "failed: {failed:?}"
        );
        // One folder, the site's own folders under it.
        let index = items
            .iter()
            .find(|i| i["url"] == lab.url("/web/"))
            .map(|i| PathBuf::from(i["dest"].as_str().unwrap()))
            .unwrap();
        assert!(
            index.ends_with(
                Path::new(&format!("127.0.0.1_{port}"))
                    .join("web")
                    .join("index.html")
            ),
            "{}",
            index.display()
        );
        let site = index.parent().unwrap().to_path_buf();
        let top = site.parent().unwrap().parent().unwrap().to_path_buf();
        assert!(top.starts_with(dl), "{}", top.display());
        // The pages made to lead to the copy (beside the window, once each is downloaded).
        wait_for("the pages made to work offline", data, 30, || {
            std::fs::read_to_string(&index).is_ok_and(|t| t.contains(r#"href="about.html""#))
        });
        let page = std::fs::read_to_string(&index).unwrap();
        for want in [
            r#"href="docs/index.html""#,
            r#"href="blog/post@id=2.html""#,
            r#"href="moved/index.html""#,
            r#"src="img/lazy.png""#,
            r#"<link rel="stylesheet" href="css/main.css">"#,
        ] {
            assert!(page.contains(want), "{want} not in\n{page}");
        }
        wait_for("a deeper page made to work offline", data, 30, || {
            std::fs::read_to_string(site.join("docs").join("based.html"))
                .is_ok_and(|t| t.contains(r#"href="../files/report.pdf""#))
        });
        let css = std::fs::read_to_string(site.join("css").join("main.css")).unwrap();
        assert!(css.contains("url('../img/logo.png')"), "{css}");
        assert!(site.join("caf\u{e9}").join("men\u{fc}.html").is_file());
        // The page at the folder's top that opens the copy.
        let shortcut = std::fs::read_to_string(top.join("127.0.0.1.html")).unwrap();
        assert!(
            shortcut.contains(&format!("url=127.0.0.1_{port}/web/index.html")),
            "{shortcut}"
        );
        // No window for any of them.
        assert!(
            !log(data).contains("window opened: complete-"),
            "a Download complete window opened"
        );
        // The project knows them all.
        let project = grabbed(data).unwrap();
        assert_eq!(project["added"].as_array().unwrap().len(), found);
    });
}

#[test]
#[ignore = "opens windows; run on request"]
fn a_grab_goes_on_when_its_window_gives_way_and_keeps_what_it_found() {
    let (_rt, lab) = lab();
    with_app(|data, dl| {
        second_launch(
            data,
            dl,
            &format!("site-grabber-run:{}", lab.url("/web/big/")),
        );
        wait_for("the grab under way", data, 30, || {
            lab.log()
                .iter()
                .filter(|e| e.path.starts_with("/web/big/p"))
                .count()
                > 20
        });
        // Another window in its place: the grab's window is gone, as when it is closed.
        second_launch(data, dl, "site-grabber:start");
        wait_for("the grab's end", data, 120, || grabbed(data).is_some());
        let project = grabbed(data).unwrap();
        // The offline template's Most pages (500) came first: where it stopped is kept, to
        // continue from.
        assert_eq!(project["ended"], "MaxPages", "{project}");
        assert_eq!(project["found"].as_array().unwrap().len(), 500);
        assert!(
            !project["frontier"]["waiting"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    });
}
