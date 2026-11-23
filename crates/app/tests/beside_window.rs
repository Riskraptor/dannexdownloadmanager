//! #743: what the main window used to ask the disk on its own thread, asked beside it, in the
//! real app with a data folder and a download folder of its own.
//!
//! - Delete with its files (the automation API's `delete`, as the toolbar's Delete with "also
//!   the files"): the row leaves the list and its finished, part and state files go from the
//!   disk, beside the window; Remove keeps its file. The app runs without a window
//!   (`--headless`).
//! - A `.torrent` file handed over as Windows starts the program for one: whether the path is a
//!   file is looked at beside the window, then Add torrent opens for it. The app runs off the
//!   screens (MDM_OFFSCREEN), which needs OpenGL or Direct3D.
//!
//! Both run the real app, so they run on request:
//! `cargo test -p mdm-app --test beside_window -- --ignored`.

use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

const APP: &str = env!("CARGO_BIN_EXE_dannexdownloadmanager");

/// The app's log so far (every file of `logs/`).
fn log(data: &Path) -> String {
    let mut all = String::new();
    if let Ok(dir) = std::fs::read_dir(data.join("logs")) {
        for f in dir.flatten() {
            all += &std::fs::read_to_string(f.path()).unwrap_or_default();
        }
    }
    all
}

/// Wait until `done`, failing if the app quits first or 60 s pass.
fn wait_for(app: &mut Child, data: &Path, what: &str, done: impl Fn() -> bool) {
    let until = Instant::now() + Duration::from_secs(60);
    while !done() {
        if let Some(status) = app.try_wait().unwrap() {
            panic!("the app closed ({status}) before {what}:\n{}", log(data));
        }
        assert!(Instant::now() < until, "no {what}:\n{}", log(data));
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// dannexdownloadmanager-cli's command `args`, to the app running on `data`.
fn cli(data: &Path, args: &[&str]) -> Output {
    Command::new(APP)
        .arg(mdm_cli::FLAG)
        .args(args)
        .env("MDM_DATA_DIR", data)
        .output()
        .unwrap()
}

/// A finished download of `name` in `dir`.
fn finished(id: u64, name: &str, dir: &Path) -> serde_json::Value {
    serde_json::json!({
        "id": id, "url": format!("https://files.example/{name}"), "file_name": name,
        "dest": dir.join(name), "size": 3, "downloaded": 3, "status": "Complete",
        "category": "Compressed", "description": "", "last_try": null, "error": null,
        "resumable": null,
    })
}

/// No update check, no first-start questions, English; `api`: the automation API on.
fn settings(api: bool) -> serde_json::Value {
    serde_json::json!({
        "language": "en", "update_check": false, "helpers_asked": true, "language_asked": true,
        "extension_never_ask": true, "clipboard_autostart": false, "automation_api": api,
        "torrent_dht": false, "torrent_lsd": false, "torrent_upnp": false,
        "torrent_incoming": false,
    })
}

fn stop(mut app: Child, data: &Path) {
    let _ = Command::new(APP)
        .arg("/exit")
        .env("MDM_DATA_DIR", data)
        .env("MDM_OFFSCREEN", "1")
        .status();
    let until = Instant::now() + Duration::from_secs(15);
    while app.try_wait().unwrap().is_none() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(200));
    }
    let _ = app.kill();
    let _ = app.wait();
}

#[test]
#[ignore = "runs the real app (without a window); run with --ignored"]
fn delete_with_its_files_takes_the_row_and_its_files_beside_the_window() {
    let dir = tempfile::tempdir().unwrap();
    let (data, dl) = (dir.path().join("data"), dir.path().join("dl"));
    for d in [&data, &dl] {
        std::fs::create_dir_all(d).unwrap();
    }
    let done = dl.join("done.zip");
    let (part, state) = (dl.join("done.zip.ddm.part"), dl.join("done.zip.ddm.json"));
    // And under their former names (Dannex up to 0.6.5): they go too.
    let (old_part, old_state) = (dl.join("done.zip.mdm.part"), dl.join("done.zip.mdm.json"));
    for f in [
        &done,
        &part,
        &state,
        &old_part,
        &old_state,
        &dl.join("kept.zip"),
    ] {
        std::fs::write(f, b"abc").unwrap();
    }
    std::fs::write(
        data.join("downloads.json"),
        serde_json::json!({
            "next_id": 3, "version": 2, "settings": settings(true),
            "items": [finished(1, "done.zip", &dl), finished(2, "kept.zip", &dl)],
        })
        .to_string(),
    )
    .unwrap();
    let mut app = Command::new(APP)
        .args(["--headless", "--data"])
        .arg(&data)
        .arg("--downloads")
        .arg(&dl)
        .env("MDM_REMOTE", "off")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        wait_for(&mut app, &data, "the app answering", || {
            cli(&data, &["status"]).status.success()
        });
        let out = cli(&data, &["delete", "1"]);
        let answer = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "{answer}");
        assert!(answer.contains("\"done\":[1]"), "{answer}");
        let out = cli(&data, &["remove", "2"]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stdout)
        );
        // Off the list at once (the next answer has neither), its files gone beside the window.
        let list = String::from_utf8_lossy(&cli(&data, &["status"]).stdout).into_owned();
        assert!(
            !list.contains("done.zip") && !list.contains("kept.zip"),
            "{list}"
        );
        wait_for(&mut app, &data, "its files gone", || {
            !done.exists()
                && !part.exists()
                && !state.exists()
                && !old_part.exists()
                && !old_state.exists()
        });
        assert!(dl.join("kept.zip").exists(), "Remove keeps its file");
    }));
    stop(app, &data);
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
#[ignore = "opens the app's windows (off-screen): needs OpenGL or Direct3D; run with --ignored"]
fn a_torrent_file_handed_over_opens_add_torrent() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(
        data.join("downloads.json"),
        serde_json::json!({ "next_id": 1, "items": [], "version": 2, "settings": settings(false) })
            .to_string(),
    )
    .unwrap();
    let torrent = dir.path().join("DDM test.torrent");
    std::fs::write(&torrent, b"d4:infod4:name8:ddm testee").unwrap();
    let mut app = Command::new(APP)
        .env("MDM_DATA_DIR", &data)
        .env("MDM_OFFSCREEN", "1")
        .spawn()
        .unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        wait_for(&mut app, &data, "the main window", || {
            log(&data).contains("window opened: main")
        });
        let status = Command::new(APP)
            .arg(&torrent)
            .env("MDM_DATA_DIR", &data)
            .env("MDM_OFFSCREEN", "1")
            .status()
            .unwrap();
        assert!(status.success(), "second launch: {status}");
        wait_for(&mut app, &data, "Add torrent", || {
            log(&data).contains("window opened: add-torrent")
        });
    }));
    stop(app, &data);
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}
