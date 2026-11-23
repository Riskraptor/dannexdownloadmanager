//! Options, menu commands and toolbar buttons doing what their labels say, in the real app with a
//! data folder of its own, the results read on disk, in the log, through the automation API and
//! in a Run value of the test's own (MDM_TEST_RUN_VALUE: the user's own is never read nor
//! written by an off-screen copy).
//!
//! - Off the screens (MDM_OFFSCREEN, Windows): the windows are driven through their
//!   accessibility tree, as a screen reader drives them (UI Automation over AccessKit): a control
//!   is found by its name and clicked through it. The real pointer never moves and no other
//!   program's window is looked at.
//! - Without a window (`--headless`, every system): the Scheduler's start and stop times and
//!   their sounds.
//!
//! They run the real app, and the off-screen ones need OpenGL or Direct3D, so they run on
//! request: `cargo test -p mdm-app --test options_drive -- --ignored --test-threads=1`.

use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output};
use std::time::{Duration, Instant};

use mdm_labserver::{Config, LabServer};

const APP: &str = env!("CARGO_BIN_EXE_dannexdownloadmanager");

/// The app's process, ended with the test whatever happens.
struct Process(Child);

impl Deref for Process {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.0
    }
}

impl DerefMut for Process {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.0
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

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

/// Wait until `done`, failing if the app quits first or `wait` passes.
fn wait_until(app: &mut Child, data: &Path, what: &str, wait: Duration, done: impl Fn() -> bool) {
    let until = Instant::now() + wait;
    while !done() {
        let text = log(data);
        assert!(!text.contains("panicked"), "the app panicked:\n{text}");
        if let Some(status) = app.try_wait().unwrap() {
            panic!("the app closed ({status}) before {what}:\n{text}");
        }
        assert!(Instant::now() < until, "no {what} after {wait:?}:\n{text}");
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Wait for a log line holding `line`.
fn wait_log(app: &mut Child, data: &Path, line: &str, wait: Duration) {
    wait_until(app, data, &format!("'{line}' in the log"), wait, || {
        log(data).contains(line)
    });
}

/// How many log lines hold `line`.
fn count_log(data: &Path, line: &str) -> usize {
    log(data).lines().filter(|l| l.contains(line)).count()
}

/// The app's settings as last saved.
fn saved(data: &Path) -> serde_json::Value {
    std::fs::read_to_string(data.join("downloads.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .map(|v| v["settings"].clone())
        .unwrap_or_default()
}

/// No update check, no first-start questions, English, the automation API on; `more` on top.
fn settings(more: serde_json::Value) -> serde_json::Value {
    let mut s = serde_json::json!({
        "language": "en", "update_check": false, "helpers_asked": true, "language_asked": true,
        "extension_never_ask": true, "clipboard_autostart": false, "automation_api": true,
        "show_complete": false, "torrent_dht": false, "torrent_lsd": false,
        "torrent_upnp": false, "torrent_incoming": false,
    });
    if let (Some(s), Some(more)) = (s.as_object_mut(), more.as_object()) {
        s.extend(more.clone());
    }
    s
}

/// A data folder holding `list` (its `settings`, `items`, `queues`).
fn data_with(dir: &Path, name: &str, list: serde_json::Value) -> PathBuf {
    let data = dir.join(name);
    std::fs::create_dir_all(&data).unwrap();
    let mut list = list;
    list["version"] = 2.into();
    std::fs::write(data.join("downloads.json"), list.to_string()).unwrap();
    data
}

/// The sound settings: the queue's two events on, with files that aren't there (an off-screen
/// copy plays nothing anyway: the log says which would have played).
fn queue_sounds() -> serde_json::Value {
    serde_json::json!([
        [false, ""],
        [false, ""],
        [true, "C:\\nowhere\\queue-started.wav"],
        [true, "C:\\nowhere\\queue-stopped.wav"],
    ])
}

/// A download of the lab server's, slow enough to be running for a while.
fn slow_item(server: &LabServer, id: u64, files: &Path) -> serde_json::Value {
    let name = format!("q{id}.bin");
    serde_json::json!({
        "id": id, "url": server.url(&format!("/gen/16M/{name}?seed={id}&rate=128K")),
        "file_name": name, "dest": files.join(&name), "size": null, "downloaded": 0,
        "status": "Queued", "category": "General", "description": "", "resumable": false,
        "added": "Nov 22 10:00:00 2026", "log": [],
    })
}

/// The automation API's answer to `args` (`status`: the downloads and the queues).
fn cli(data: &Path, args: &[&str]) -> Output {
    Command::new(APP)
        .arg(mdm_cli::FLAG)
        .args(args)
        .env("MDM_DATA_DIR", data)
        .output()
        .unwrap()
}

/// The list as the automation API gives it.
fn status(data: &Path) -> serde_json::Value {
    let out = cli(data, &["--json", "status"]);
    serde_json::from_slice(&out.stdout).unwrap_or_default()
}

/// Ask the app running on `data` to quit, and wait for it.
fn quit(app: &mut Child, data: &Path) {
    let _ = Command::new(APP)
        .arg("/exit")
        .env("MDM_DATA_DIR", data)
        .env("MDM_OFFSCREEN", "1")
        .status();
    let until = Instant::now() + Duration::from_secs(15);
    while app.try_wait().unwrap().is_none() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(200));
    }
}

#[cfg(windows)]
mod window {
    use super::*;

    /// The app off the screens on `data`, with a Run value of `run`'s name.
    pub fn start(data: &Path, run: &str) -> Process {
        let app = Command::new(APP)
            .env("MDM_DATA_DIR", data)
            .env("MDM_OFFSCREEN", "1")
            .env("MDM_REMOTE", "off")
            .env("MDM_TEST_RUN_VALUE", run)
            .spawn()
            .unwrap();
        Process(app)
    }

    /// `/show <what>` to the app running on `data` (a second launch hands it over).
    pub fn show(data: &Path, what: &str) {
        let _ = Command::new(APP)
            .args(["/show", what])
            .env("MDM_DATA_DIR", data)
            .env("MDM_OFFSCREEN", "1")
            .status();
    }

    /// A PowerShell script's output.
    fn powershell(script: &str) -> String {
        let out = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    /// Click the control named `name` in one of process `pid`'s windows, as a screen reader
    /// does (UI Automation's Invoke, Toggle or Select, which AccessKit hands the window as a
    /// click): true when it was found and clicked. Mouse messages posted to an off-screen window
    /// don't do: the shell tells it at once that the real pointer is elsewhere.
    fn click(pid: u32, window: &str, name: &str, nth: usize) -> bool {
        let script = r#"
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type @'
using System; using System.Text; using System.Runtime.InteropServices;
public static class UC {
  public delegate bool P(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(P f, IntPtr l);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint p);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
}
'@
$wins = New-Object System.Collections.ArrayList
[UC]::EnumWindows({ param($h, $l)
  $p = 0; [UC]::GetWindowThreadProcessId($h, [ref]$p) | Out-Null
  $t = New-Object System.Text.StringBuilder 512; [UC]::GetWindowText($h, $t, 512) | Out-Null
  if ($p -eq PID -and [UC]::IsWindowVisible($h) -and $t.ToString().Contains('WINDOW')) { [void]$wins.Add($h) }
  $true }, [IntPtr]::Zero) | Out-Null
$named = New-Object System.Windows.Automation.PropertyCondition(
  [System.Windows.Automation.AutomationElement]::NameProperty, 'NAME')
$found = New-Object System.Collections.ArrayList
foreach ($h in $wins) {
  try { $root = [System.Windows.Automation.AutomationElement]::FromHandle($h) } catch { continue }
  foreach ($e in $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $named)) { [void]$found.Add($e) }
}
if ($found.Count -gt NTH) {
  $e = $found[NTH]
  $pat = $null
  if ($e.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$pat)) { $pat.Invoke(); 'clicked' }
  elseif ($e.TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern, [ref]$pat)) { $pat.Toggle(); 'clicked' }
  elseif ($e.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern, [ref]$pat)) { $pat.Select(); 'clicked' }
  elseif ($e.TryGetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern, [ref]$pat)) { $pat.Expand(); 'clicked' }
}
"#
        .replace("PID", &pid.to_string())
        .replace("WINDOW", &window.replace('\'', "''"))
        .replace("NTH", &nth.to_string())
        .replace("NAME", &name.replace('\'', "''"));
        powershell(&script) == "clicked"
    }

    /// Click the control named `name` once it is there (a window opens, its tree comes a frame
    /// later), failing after 20 s.
    pub fn click_when_there(app: &mut Child, data: &Path, name: &str) {
        click_in(app, data, "", name, 0);
    }

    /// Click the `nth` control named `name` in a window whose title holds `window` ("" any),
    /// once it is there, failing after 20 s.
    pub fn click_in(app: &mut Child, data: &Path, window: &str, name: &str, nth: usize) {
        let pid = app.id();
        wait_until(
            app,
            data,
            &format!("a control named '{name}' in '{window}'"),
            Duration::from_secs(20),
            || click(pid, window, name, nth),
        );
        // The click is the window's next frame's.
        std::thread::sleep(Duration::from_millis(600));
    }

    /// Whether process `pid` shows a window whose title holds `title`.
    pub fn has_window(pid: u32, title: &str) -> bool {
        let script = r#"
Add-Type @'
using System; using System.Text; using System.Runtime.InteropServices;
public static class HW {
  public delegate bool P(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(P f, IntPtr l);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint p);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
}
'@
$script:found = $false
[HW]::EnumWindows({ param($h, $l)
  $p = 0; [HW]::GetWindowThreadProcessId($h, [ref]$p) | Out-Null
  $t = New-Object System.Text.StringBuilder 512; [HW]::GetWindowText($h, $t, 512) | Out-Null
  if ($p -eq PID -and [HW]::IsWindowVisible($h) -and $t.ToString().Contains('TITLE')) { $script:found = $true }
  $true }, [IntPtr]::Zero) | Out-Null
$script:found
"#
        .replace("PID", &pid.to_string())
        .replace("TITLE", &title.replace('\'', "''"));
        powershell(&script) == "True"
    }

    /// The Run value of the test's own name, removed when the test ends.
    pub struct RunValue(pub String);

    const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

    impl RunValue {
        pub fn new() -> Self {
            RunValue(format!(
                "DannexDownloadManagerTestOptions{}",
                std::process::id()
            ))
        }

        /// The command it holds, if it is there.
        pub fn read(&self) -> Option<String> {
            let out = Command::new("reg")
                .args(["query", RUN_KEY, "/v", &self.0])
                .output()
                .unwrap();
            out.status.success().then(|| {
                let text = String::from_utf8_lossy(&out.stdout);
                text.lines()
                    .find_map(|l| l.split_once("REG_SZ").map(|(_, v)| v.trim().to_owned()))
                    .unwrap_or_default()
            })
        }
    }

    impl Drop for RunValue {
        fn drop(&mut self) {
            let _ = Command::new("reg")
                .args(["delete", RUN_KEY, "/v", &self.0, "/f"])
                .output();
        }
    }
}

/// General → "Launch Dannex Download Manager on startup", ticked and OK: the startup entry is
/// written (the program, with `/startup`) and the setting saved; unticked and OK: the entry goes.
#[cfg(windows)]
#[test]
#[ignore = "opens the app's windows (off-screen): needs OpenGL or Direct3D; run with --ignored"]
fn launch_on_startup_writes_and_removes_the_startup_entry() {
    use window::*;
    let dir = tempfile::tempdir().unwrap();
    let data = data_with(
        dir.path(),
        "startup",
        serde_json::json!({ "next_id": 1, "items": [], "settings": settings(serde_json::json!({})) }),
    );
    let run = RunValue::new();
    let mut app = start(&data, &run.0);
    wait_log(
        &mut app,
        &data,
        "first frame drawn",
        Duration::from_secs(60),
    );
    assert_eq!(run.read(), None, "no startup entry before");

    show(&data, "options");
    click_when_there(&mut app, &data, "Launch Dannex Download Manager on startup");
    click_when_there(&mut app, &data, "OK");
    wait_until(
        &mut app,
        &data,
        "the startup entry",
        Duration::from_secs(10),
        || run.read().is_some(),
    );
    let entry = run.read().unwrap();
    assert!(
        entry.contains("dannexdownloadmanager") && entry.ends_with("/startup"),
        "{entry}"
    );
    wait_until(
        &mut app,
        &data,
        "the setting saved",
        Duration::from_secs(10),
        || saved(&data)["launch_on_startup"] == true,
    );

    show(&data, "options");
    click_when_there(&mut app, &data, "Launch Dannex Download Manager on startup");
    click_when_there(&mut app, &data, "OK");
    wait_until(
        &mut app,
        &data,
        "the startup entry removed",
        Duration::from_secs(10),
        || run.read().is_none(),
    );
    wait_until(
        &mut app,
        &data,
        "the setting saved",
        Duration::from_secs(10),
        || saved(&data)["launch_on_startup"] == false,
    );
    quit(&mut app, &data);
}

/// Proxy / Socks → "Manual proxy/socks configuration" chosen: its protocols (http, https) are
/// ticked with it, and OK saves them; a proxy typed with neither ticked carried no download.
#[cfg(windows)]
#[test]
#[ignore = "opens the app's windows (off-screen): needs OpenGL or Direct3D; run with --ignored"]
fn a_manual_proxy_is_used_for_http_and_https() {
    use window::*;
    let dir = tempfile::tempdir().unwrap();
    let data = data_with(
        dir.path(),
        "proxy",
        serde_json::json!({ "next_id": 1, "items": [], "settings": settings(serde_json::json!({})) }),
    );
    let run = RunValue::new();
    let mut app = start(&data, &run.0);
    wait_log(
        &mut app,
        &data,
        "first frame drawn",
        Duration::from_secs(60),
    );
    show(&data, "options:proxy");
    click_when_there(&mut app, &data, "Manual proxy/socks configuration");
    click_when_there(&mut app, &data, "OK");
    wait_until(
        &mut app,
        &data,
        "the manual proxy saved with both protocols",
        Duration::from_secs(10),
        || {
            let s = saved(&data);
            s["proxy_mode"] == 3 && s["proxy_http"] == true && s["proxy_https"] == true
        },
    );
    quit(&mut app, &data);
}

/// The toolbar's Stop All while the main queue runs: the downloads stop, and so does the queue
/// (its "stopped" sound), which then starts nothing more; before, the queue started its next
/// download at once. The queue starts with DDM (its "started" sound).
#[cfg(windows)]
#[tokio::test(flavor = "multi_thread")]
#[ignore = "opens the app's windows (off-screen): needs OpenGL or Direct3D; run with --ignored"]
async fn stop_all_stops_the_running_queue() {
    use window::*;
    let server = LabServer::start(Config::default()).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let files = dir.path().join("files");
    std::fs::create_dir_all(&files).unwrap();
    let items: Vec<_> = (1..=3).map(|id| slow_item(&server, id, &files)).collect();
    let data = data_with(
        dir.path(),
        "stop-all",
        serde_json::json!({
            "next_id": 4, "items": items,
            "queues": [
                { "name": "Main download queue", "files_at_once": 1, "items": [1, 2, 3],
                  "sync": false, "schedule": { "start_on_startup": true } },
                { "name": "Synchronization queue", "files_at_once": 4, "items": [], "sync": true },
            ],
            "settings": settings(serde_json::json!({ "sounds": queue_sounds() })),
        }),
    );
    let result = tokio::task::spawn_blocking(move || {
        let run = RunValue::new();
        let mut app = start(&data, &run.0);
        wait_log(
            &mut app,
            &data,
            "sound: queue started",
            Duration::from_secs(60),
        );
        let downloading = |s: &serde_json::Value| {
            s["downloads"]
                .as_array()
                .is_some_and(|d| d.iter().any(|d| d["status"] == "Downloading"))
        };
        wait_until(
            &mut app,
            &data,
            "the queue's first download running",
            Duration::from_secs(30),
            || downloading(&status(&data)),
        );
        click_when_there(&mut app, &data, "Stop All");
        wait_log(
            &mut app,
            &data,
            "sound: queue stopped",
            Duration::from_secs(10),
        );
        // A few seconds on: the queue didn't start its next download.
        std::thread::sleep(Duration::from_secs(4));
        let s = status(&data);
        assert_eq!(s["queues"][0]["running"], false, "{s}");
        assert!(!downloading(&s), "a download runs after Stop All: {s}");
        quit(&mut app, &data);
    })
    .await;
    if let Err(e) = result {
        std::panic::resume_unwind(e.into_panic());
    }
}

/// The list's right-click → Open with... reaches the system's chooser for the file (an off-screen
/// copy logs it instead of showing it; win32.rs's own test shows Windows' chooser coming up for a
/// path with spaces). And General → Test browser integration → Add to your browser... opens the
/// extension's window again after its "Don't ask again".
#[cfg(windows)]
#[test]
#[ignore = "opens the app's windows (off-screen): needs OpenGL or Direct3D; run with --ignored"]
fn open_with_and_the_extension_offer_are_reachable() {
    use window::*;
    let dir = tempfile::tempdir().unwrap();
    let files = dir.path().join("a folder with spaces");
    std::fs::create_dir_all(&files).unwrap();
    let file = files.join("a sample file.txt");
    std::fs::write(&file, b"abc").unwrap();
    let item = serde_json::json!({
        "id": 1, "url": "https://files.example/a%20sample%20file.txt",
        "file_name": "a sample file.txt", "dest": file, "size": 3, "downloaded": 3,
        "status": "Complete", "category": "Documents", "description": "", "last_try": null,
        "error": null, "resumable": null,
    });
    let data = data_with(
        dir.path(),
        "open-with",
        serde_json::json!({ "next_id": 2, "items": [item], "settings": settings(serde_json::json!({})) }),
    );
    let run = RunValue::new();
    let mut app = start(&data, &run.0);
    wait_log(
        &mut app,
        &data,
        "first frame drawn",
        Duration::from_secs(60),
    );
    show(&data, "context");
    click_when_there(&mut app, &data, "Open with...");
    wait_log(
        &mut app,
        &data,
        "open with: the chooser asked for",
        Duration::from_secs(10),
    );
    assert!(log(&data).contains("command OpenWith"));

    // The offer was refused ("extension_never_ask"); no extension has connected to this data
    // folder.
    show(&data, "options:test");
    click_when_there(&mut app, &data, "Add to your browser...");
    let pid = app.id();
    wait_until(
        &mut app,
        &data,
        "the extension's window",
        Duration::from_secs(10),
        || has_window(pid, "Add Dannex Download Manager to your browser"),
    );
    quit(&mut app, &data);
}

/// The Scheduler's start and stop times, without a window (`--headless`): the queue starts at its
/// start time with Options → Sounds → "Queue processing started" and stops at its stop time with
/// "Queue processing stopped/finished" (before, only a start by hand and a queue that finished
/// had their sounds).
#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs the real app (without a window) for half a minute; run with --ignored"]
async fn the_schedulers_start_and_stop_play_their_sounds() {
    use chrono::Timelike;
    let server = LabServer::start(Config::default()).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let files = dir.path().join("files");
    std::fs::create_dir_all(&files).unwrap();
    let items: Vec<_> = (1..=2).map(|id| slow_item(&server, id, &files)).collect();
    let at = |secs: i64| {
        let t = chrono::Local::now() + chrono::Duration::seconds(secs);
        serde_json::json!([t.hour(), t.minute(), t.second()])
    };
    let data = data_with(
        dir.path(),
        "scheduled",
        serde_json::json!({
            "next_id": 3, "items": items,
            "queues": [
                { "name": "Main download queue", "files_at_once": 1, "items": [1, 2],
                  "sync": false, "schedule": { "start_at": at(8), "stop_at": at(16) } },
                { "name": "Synchronization queue", "files_at_once": 4, "items": [], "sync": true },
            ],
            "settings": settings(serde_json::json!({ "sounds": queue_sounds() })),
        }),
    );
    let result = tokio::task::spawn_blocking(move || {
        let mut app = Process(
            Command::new(APP)
                .args(["--headless", "--data"])
                .arg(&data)
                .arg("--downloads")
                .arg(&files)
                .env("MDM_REMOTE", "off")
                .spawn()
                .unwrap(),
        );
        wait_log(
            &mut app,
            &data,
            "sound: queue started",
            Duration::from_secs(30),
        );
        assert_eq!(
            count_log(&data, "sound: queue stopped"),
            0,
            "{}",
            log(&data)
        );
        wait_until(
            &mut app,
            &data,
            "the queue running",
            Duration::from_secs(10),
            || status(&data)["queues"][0]["running"] == true,
        );
        wait_log(
            &mut app,
            &data,
            "sound: queue stopped",
            Duration::from_secs(20),
        );
        let s = status(&data);
        assert_eq!(s["queues"][0]["running"], false, "{s}");
        quit(&mut app, &data);
    })
    .await;
    if let Err(e) = result {
        std::panic::resume_unwind(e.into_panic());
    }
}

/// A download of the lab server's named `name` (`size` and `rate` as the lab's path takes them).
fn lab_item(
    server: &LabServer,
    id: u64,
    name: &str,
    size: &str,
    rate: &str,
    files: &Path,
) -> serde_json::Value {
    serde_json::json!({
        "id": id, "url": server.url(&format!("/gen/{size}/{name}?seed={id}&rate={rate}")),
        "file_name": name, "dest": files.join(name), "size": null, "downloaded": 0,
        "status": "Queued", "category": "General", "description": "", "resumable": false,
        "added": "Nov 23 10:00:00 2026", "log": [],
    })
}

/// Proxy / Socks → Advanced / Socks...: SOCKS 5 with the names looked up by the proxy makes the
/// manual proxy's address `socks5h://`; View → Arrange files → By Save Path is a sort order kept
/// with the list's columns.
#[cfg(windows)]
#[test]
#[ignore = "opens the app's windows (off-screen): needs OpenGL or Direct3D; run with --ignored"]
fn advanced_socks_and_arrange_by_save_path() {
    use window::*;
    let dir = tempfile::tempdir().unwrap();
    let data = data_with(
        dir.path(),
        "socks",
        serde_json::json!({ "next_id": 1, "items": [], "settings": settings(serde_json::json!({
            "proxy_mode": 3, "proxy_host": "proxy.test", "proxy_port": "1080",
            "proxy_http": true, "proxy_https": true,
        })) }),
    );
    let run = RunValue::new();
    let mut app = start(&data, &run.0);
    wait_log(
        &mut app,
        &data,
        "first frame drawn",
        Duration::from_secs(60),
    );
    show(&data, "options:proxy");
    click_when_there(&mut app, &data, "Advanced / Socks...");
    click_in(&mut app, &data, "Advanced / Socks", "SOCKS 5", 0);
    click_in(
        &mut app,
        &data,
        "Advanced / Socks",
        "Let the proxy look up the servers' names (DNS through the proxy)",
        0,
    );
    click_in(&mut app, &data, "Advanced / Socks", "OK", 0);
    click_in(&mut app, &data, "Configuration", "OK", 0);
    wait_until(
        &mut app,
        &data,
        "the SOCKS 5 proxy saved",
        Duration::from_secs(10),
        || {
            let s = saved(&data);
            s["proxy_host"] == "socks5h://proxy.test"
                && s["proxy_port"] == "1080"
                && s["proxy_mode"] == 3
        },
    );

    show(&data, "menu:3");
    click_when_there(&mut app, &data, "Arrange files");
    click_when_there(&mut app, &data, "By Save Path");
    let columns = || {
        std::fs::read_to_string(data.join("downloads.json"))
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .map(|v| v["columns"]["sort"].clone())
            .unwrap_or_default()
    };
    wait_until(
        &mut app,
        &data,
        "the list sorted by save path",
        Duration::from_secs(10),
        || columns() == serde_json::json!([100, true]),
    );
    quit(&mut app, &data);
}

/// Downloads → Virus checking settings: Microsoft Defender's preset, chosen with one click,
/// checks a finished program; a check that reports a problem says so in the download's log and
/// in a message (Download complete is off here).
#[cfg(windows)]
#[tokio::test(flavor = "multi_thread")]
#[ignore = "opens the app's windows (off-screen) and runs Microsoft Defender's scanner; run with --ignored"]
async fn virus_checking_runs_after_a_download() {
    use window::*;
    let server = LabServer::start(Config::default()).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let files = dir.path().join("files");
    std::fs::create_dir_all(&files).unwrap();
    let defender = Path::new(r"C:\Program Files\Windows Defender\MpCmdRun.exe").exists();
    let item = lab_item(&server, 1, "tool.exe", "256K", "64M", &files);
    let failing = lab_item(&server, 2, "other.exe", "256K", "64M", &files);
    let data = data_with(
        dir.path(),
        "virus",
        serde_json::json!({ "next_id": 3, "items": [item, failing],
            "settings": settings(serde_json::json!({})) }),
    );
    let result = tokio::task::spawn_blocking(move || {
        let run = RunValue::new();
        let mut app = start(&data, &run.0);
        wait_log(
            &mut app,
            &data,
            "first frame drawn",
            Duration::from_secs(60),
        );
        if defender {
            show(&data, "options:downloads");
            // The page's second Edit... (the first is Customize "Download progress" dialog's).
            click_in(&mut app, &data, "Configuration", "Edit...", 1);
            click_in(
                &mut app,
                &data,
                "Virus checking",
                "Use Microsoft Defender",
                0,
            );
            click_in(&mut app, &data, "Virus checking", "OK", 0);
            click_in(&mut app, &data, "Configuration", "OK", 0);
            wait_until(
                &mut app,
                &data,
                "Defender saved",
                Duration::from_secs(10),
                || {
                    let s = saved(&data);
                    s["virus_check"] == true
                        && s["virus_program"]
                            .as_str()
                            .is_some_and(|p| p.ends_with("MpCmdRun.exe"))
                        && s["virus_args"]
                            .as_str()
                            .is_some_and(|a| a.contains("-ScanType 3"))
                },
            );
            let out = cli(&data, &["start", "1"]);
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stdout)
            );
            wait_log(
                &mut app,
                &data,
                "virus check of download 1: Clean",
                Duration::from_secs(180),
            );
        }
        quit(&mut app, &data);

        // A program that reports a problem (exit code 3).
        let mut list: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(data.join("downloads.json")).unwrap())
                .unwrap();
        list["settings"]["virus_check"] = true.into();
        list["settings"]["virus_program"] = r"C:\Windows\System32\cmd.exe".into();
        list["settings"]["virus_args"] = "/c exit 3 \"[FILE]\"".into();
        std::fs::write(data.join("downloads.json"), list.to_string()).unwrap();
        let mut app = start(&data, &run.0);
        // The log has the first run's lines: wait for this run to answer.
        wait_until(
            &mut app,
            &data,
            "the app answering",
            Duration::from_secs(60),
            || cli(&data, &["status"]).status.success(),
        );
        let out = cli(&data, &["start", "2"]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stdout)
        );
        wait_log(
            &mut app,
            &data,
            "virus check of download 2: Problem(3)",
            Duration::from_secs(60),
        );
        let pid = app.id();
        wait_until(
            &mut app,
            &data,
            "the message",
            Duration::from_secs(10),
            || has_window(pid, "Virus check"),
        );
        quit(&mut app, &data);
    })
    .await;
    if let Err(e) = result {
        std::panic::resume_unwind(e.into_panic());
    }
}

/// Save to → Temporary directory, as saved: the part file is there while the download runs,
/// none beside the file, and only the finished file is left at the end. Without a window
/// (`--headless`), as the setting reaches the engine whatever the window does.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs the real app (without a window); run with --ignored"]
async fn the_temporary_directory_holds_the_part_file() {
    let server = LabServer::start(Config::default()).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let (files, temp) = (dir.path().join("files"), dir.path().join("temp parts"));
    std::fs::create_dir_all(&files).unwrap();
    let item = lab_item(&server, 1, "slow.bin", "4M", "512K", &files);
    let data = data_with(
        dir.path(),
        "temp-dir",
        serde_json::json!({ "next_id": 2, "items": [item],
            "settings": settings(serde_json::json!({ "temp_dir": temp })) }),
    );
    let result = tokio::task::spawn_blocking(move || {
        let mut app = Process(
            Command::new(APP)
                .args(["--headless", "--data"])
                .arg(&data)
                .arg("--downloads")
                .arg(&files)
                .env("MDM_REMOTE", "off")
                .spawn()
                .unwrap(),
        );
        wait_until(
            &mut app,
            &data,
            "the app answering",
            Duration::from_secs(30),
            || cli(&data, &["status"]).status.success(),
        );
        assert!(cli(&data, &["start", "1"]).status.success());
        let part_in_temp = || {
            std::fs::read_dir(&temp).is_ok_and(|d| {
                d.flatten()
                    .any(|e| e.file_name().to_string_lossy().ends_with(".ddm.part"))
            })
        };
        wait_until(
            &mut app,
            &data,
            "the part file in the temporary directory",
            Duration::from_secs(20),
            part_in_temp,
        );
        assert!(!files.join("slow.bin.ddm.part").exists(), "none beside it");
        wait_until(
            &mut app,
            &data,
            "the download complete",
            Duration::from_secs(60),
            || status(&data)["downloads"][0]["status"] == "Complete",
        );
        assert_eq!(
            std::fs::metadata(files.join("slow.bin")).unwrap().len(),
            4 << 20
        );
        assert!(
            !part_in_temp(),
            "the part file left the temporary directory"
        );
        quit(&mut app, &data);
    })
    .await;
    if let Err(e) = result {
        std::panic::resume_unwind(e.into_panic());
    }
}
