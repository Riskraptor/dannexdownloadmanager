//! Tasks → Import → From an IDM list (.ef2) and Tasks → Export → As an IDM list (.ef2), in the
//! real app, off the screens, each run with a data folder of its own. A list as IDM exports it
//! (each download's address, referrer, cookies and User-Agent) is imported (its file dialog
//! answered by MDM_TEST_IMPORT_FROM), added from Link grabber (`/show grabber-add`, its Add to
//! queue), and exported again (MDM_TEST_EXPORT_TO): the same list comes out. A second run imports
//! that export and exports it once more: the same again, so a list goes back and forth between
//! IDM and DDM and keeps what its downloads need.
//!
//! It opens windows, which needs OpenGL or Direct3D, so it runs on request:
//! `cargo test -p mdm-app --test ef2_round_trip -- --ignored`.

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

const APP: &str = env!("CARGO_BIN_EXE_dannexdownloadmanager");

/// As IDM writes it: CRLF lines, one block per download.
const LIST: &str = "<\r\nhttps://files.example/reports/annual-report-2026.pdf\r\nreferer: https://files.example/reports/\r\ncookie: session=4f2a; theme=dark\r\nUser-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64)\r\n>\r\n<\r\nhttps://mirror.example/iso/install-disk.iso\r\nreferer: https://mirror.example/\r\ncookie: token=abc123\r\n>\r\n<\r\nhttps://cdn.example/music/track-07.mp3\r\n>\r\n<\r\nftp://ftp.example/pub/tools-2.4.zip\r\nreferer: https://ftp.example/pub/\r\n>\r\n";

fn log(data: &Path) -> String {
    std::fs::read_to_string(data.join("logs").join("ddm.log")).unwrap_or_default()
}

fn wait_for(what: &str, data: &Path, done: impl Fn() -> bool) {
    let until = Instant::now() + Duration::from_secs(45);
    while !done() {
        assert!(Instant::now() < until, "no {what}:\n{}", log(data));
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// One run: a fresh app imports `from` and exports what it then lists to `to`.
fn import_then_export(root: &Path, run: &str, from: &Path, to: &Path) {
    let data = root.join(run);
    let dl = root.join(format!("{run}-dl"));
    for d in [&data, &dl] {
        std::fs::create_dir_all(d).unwrap();
    }
    let settings = serde_json::json!({
        "language": "en", "update_check": false, "helpers_asked": true, "language_asked": true,
        "extension_never_ask": true, "clipboard_autostart": false, "grabber_check": false,
        "grabber_hide_pages": false,
    });
    std::fs::write(
        data.join("downloads.json"),
        serde_json::json!({ "next_id": 1, "items": [], "version": 2, "settings": settings })
            .to_string(),
    )
    .unwrap();
    let env = |c: &mut Command| {
        c.env("MDM_DATA_DIR", &data)
            .env("MDM_OFFSCREEN", "1")
            .env("MDM_DOWNLOADS", &dl)
            .env("MDM_TEST_IMPORT_FROM", from)
            .env("MDM_TEST_EXPORT_TO", to);
    };
    let show = |what: &str| {
        let mut c = Command::new(APP);
        c.args(["/show", what]);
        env(&mut c);
        let status = c.status().unwrap();
        assert!(status.success(), "/show {what}: {status}");
    };
    let mut c = Command::new(APP);
    env(&mut c);
    let mut app = c.spawn().unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        wait_for("main window", &data, || {
            log(&data).contains("window opened: main")
        });
        show("import:ef2");
        wait_for("Link grabber", &data, || {
            log(&data).contains("window opened: grabber")
        });
        show("grabber-add");
        wait_for("the imported downloads saved", &data, || {
            std::fs::read_to_string(data.join("downloads.json"))
                .is_ok_and(|t| t.matches("\"url\"").count() == 4)
        });
        show("export:ef2");
        wait_for("the export", &data, || {
            std::fs::read_to_string(to).is_ok_and(|t| t.matches(">\r\n").count() == 4)
        });
    }));
    let _ = app.kill();
    let _ = app.wait();
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
#[ignore = "opens windows; run on request"]
fn an_idm_list_goes_in_and_out_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let (list, first, second) = (
        root.join("from-idm.ef2"),
        root.join("first.ef2"),
        root.join("second.ef2"),
    );
    std::fs::write(&list, LIST).unwrap();
    import_then_export(root, "a", &list, &first);
    let first_text = std::fs::read_to_string(&first).unwrap();
    assert_eq!(first_text, LIST, "IDM's list, imported then exported");
    import_then_export(root, "b", &first, &second);
    assert_eq!(
        std::fs::read_to_string(&second).unwrap(),
        first_text,
        "DDM's own export, imported then exported again"
    );
}
