//! #376, end to end: a download from the browser that the site's bot check refuses, even asked
//! the way a browser connects, fails saying what to do ("Download with the browser"), and its row
//! keeps the mark that offers it, with the browser it came from, in the saved list (a restart
//! still offers it there). The lab server's `browser_only` fault is the bot check, and
//! `refuse_agent` refuses the browser-like connection too (a challenge to answer).
//!
//! It runs the real app (without a window), so it runs on request:
//! `cargo test -p mdm-app --test bot_check -- --ignored`.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use mdm_labserver::{Config, Faults, LabServer};

const APP: &str = env!("CARGO_BIN_EXE_dannexdownloadmanager");
const CHROME: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36";

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

/// One request line to the app on `data`, as the browser's native host sends it; its answer.
fn send(data: &Path, fields: serde_json::Value) -> String {
    let ep: serde_json::Value =
        serde_json::from_slice(&std::fs::read(data.join("ipc.json")).unwrap()).unwrap();
    let mut line = fields;
    line["token"] = ep["token"].clone();
    let port = ep["port"].as_u64().unwrap() as u16;
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    s.write_all(format!("{line}\n").as_bytes()).unwrap();
    let mut answer = String::new();
    BufReader::new(s).read_line(&mut answer).unwrap();
    answer.trim_end().to_owned()
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

#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs the real app (without a window); run with --ignored"]
async fn a_download_the_bot_check_refuses_offers_the_browser() {
    let s = LabServer::start(Config {
        faults: Faults::parse("browser_only=1&refuse_agent=Chrome").unwrap(),
        ..Config::default()
    })
    .await
    .unwrap();
    let url = s.url("/gen/64K/challenged.bin?seed=376");
    tokio::task::spawn_blocking(move || run(&url))
        .await
        .unwrap();
}

fn run(url: &str) {
    let dir = tempfile::tempdir().unwrap();
    let (data, dl) = (dir.path().join("data"), dir.path().join("dl"));
    for d in [&data, &dl] {
        std::fs::create_dir_all(d).unwrap();
    }
    // No update check, no first-start questions, English, the automation API on (its list).
    let settings = serde_json::json!({
        "language": "en", "update_check": false, "helpers_asked": true, "language_asked": true,
        "extension_never_ask": true, "clipboard_autostart": false, "automation_api": true,
        "torrent_dht": false, "torrent_lsd": false, "torrent_upnp": false,
        "torrent_incoming": false,
    });
    std::fs::write(
        data.join("downloads.json"),
        serde_json::json!({ "next_id": 1, "version": 2, "settings": settings, "items": [] })
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
        // From Firefox, as its native host hands a download over. Without a window nothing is
        // handed back: the app takes it.
        let answer = send(
            &data,
            serde_json::json!({
                "url": url, "user_agent": CHROME, "from_browser": "Mozilla Firefox",
                "browser_waits": true,
            }),
        );
        assert_eq!(answer, "ok");
        wait_for(&mut app, &data, "the bot check's refusal", || {
            let out = cli(&data, &["--json", "status"]);
            String::from_utf8_lossy(&out.stdout).contains("Download with the browser")
        });
        let list = String::from_utf8_lossy(&cli(&data, &["--json", "status"]).stdout).into_owned();
        assert!(
            list.contains("this site lets only browsers through") && !list.contains("Alt+click"),
            "{list}"
        );
    }));
    stop(app, &data);
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
    // Saved with the mark that offers Download with the browser, in Firefox.
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(data.join("downloads.json")).unwrap()).unwrap();
    let row = &saved["items"][0];
    assert_eq!(row["status"], "Error", "{row}");
    assert_eq!(
        row["bot_check"],
        serde_json::json!({ "browser": "Mozilla Firefox" }),
        "{row}"
    );
}
