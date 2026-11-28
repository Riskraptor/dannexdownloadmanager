//! Handing a command to the running app over mdm-ipc, starting it when it isn't running, and the
//! loop that serves the browser's messages.

use super::*;

/// Why a command didn't reach the app.
pub(super) enum Undelivered {
    /// It isn't running and couldn't be started (in time).
    NotRunning(String),
    /// It is running and said no (or stopped answering): starting another can't help.
    Refused(String),
}

/// How long a Dannex Download Manager just started gets to open its channel (a first start, a
/// slow disk or an antivirus scan can take a while); the browser's download waits meanwhile.
pub(super) const START_WAIT: Duration = Duration::from_secs(20);

/// How often the app just started is asked whether its channel is open.
pub(super) const START_POLL: Duration = Duration::from_millis(20);

/// Started by Safari's app extension (`--safari`): inside its sandbox, which a program started from
/// here would inherit, so the app is started through LaunchServices (`open`) instead.
pub(super) static SAFARI: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// `cmd` handed to the app: the reply saying so, or why not.
pub(super) fn delivered(cmd: &mdm_ipc::Command) -> Reply {
    reply_to(deliver(cmd))
}

/// The reply to a command handed to the app, or not.
pub(super) fn reply_to(got: Result<(), Undelivered>) -> Reply {
    match got {
        Ok(()) => Reply::done(),
        Err(Undelivered::NotRunning(e)) => Reply::failed("not-running", e),
        // The user chose "Download with the browser" for it in DDM a moment ago (#376): left to
        // the browser as a download Options leave to it is (`why` "options": DDM was reached,
        // nothing to fix), with its own code in `declined`, which the status page words. An older
        // extension, not knowing the code, lets the browser keep it all the same.
        Err(Undelivered::Refused(e)) if e == mdm_ipc::HANDED_BACK => Reply {
            declined: Some(mdm_ipc::HANDED_BACK),
            ..Reply::failed(
                "options",
                "handed back to the browser by DDM after the site's bot check",
            )
        },
        Err(Undelivered::Refused(e)) if e.starts_with("too-large") => Reply::failed("too-large", e),
        Err(Undelivered::Refused(e)) => Reply::failed("refused", e),
    }
}

/// The app this helper belongs to, when it is the copy inside Safari's extension
/// (`X.app/Contents/PlugIns/Y.appex/Contents/MacOS/this`): its bundle.
pub(super) fn bundle_of(exe: &std::path::Path) -> Option<PathBuf> {
    exe.ancestors()
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .map(PathBuf::from)
}

/// Hand `cmd` to Dannex Download Manager, starting it only if none is running.
pub(super) fn deliver(cmd: &mdm_ipc::Command) -> Result<(), Undelivered> {
    // Whatever it asks may open a window: the app may bring it to the front (Windows only: Linux
    // and macOS have no foreground lock for a host to lift).
    #[cfg(windows)]
    if let Some(pid) = mdm_ipc::app_pid() {
        win32::allow_foreground(pid);
    }
    match mdm_ipc::send(cmd) {
        Ok(()) => return Ok(()),
        Err(mdm_ipc::SendError::Refused(e)) => return Err(Undelivered::Refused(e)),
        Err(mdm_ipc::SendError::NotRunning(_)) => {}
    }
    let dir = exe_dir().map_err(Undelivered::NotRunning)?;
    // Beside the app, or (an AppImage's host, copied out of its mount: unix.rs) the path
    // written next to this copy.
    let app = match std::fs::read_to_string(dir.join("app-path")) {
        Ok(path) if !path.trim().is_empty() => PathBuf::from(path.trim()),
        _ => dir.join(if cfg!(windows) {
            "dannexdownloadmanager.exe"
        } else {
            "dannexdownloadmanager"
        }),
    };
    let safari = SAFARI.load(std::sync::atomic::Ordering::Relaxed);
    let mut child = if safari {
        let exe = this_program().map_err(Undelivered::NotRunning)?;
        let bundle = bundle_of(&exe)
            .ok_or_else(|| Undelivered::NotRunning(format!("{} is in no app", exe.display())))?;
        // -g: the app opens behind Safari, as it does for the other browsers; in the menu bar,
        // as `start_detached` starts it.
        std::process::Command::new("/usr/bin/open")
            .arg("-g")
            .arg(&bundle)
            .args(["--args", mdm_ipc::STARTUP])
            .spawn()
            .map_err(|e| {
                Undelivered::NotRunning(format!("cannot start {}: {e}", bundle.display()))
            })?
    } else {
        start_detached(&app)
            .map_err(|e| Undelivered::NotRunning(format!("cannot start {}: {e}", app.display())))?
    };
    #[cfg(windows)]
    win32::allow_foreground(child.id());
    let started = std::time::Instant::now();
    while started.elapsed() < START_WAIT {
        // Asked often: the app opens its channel early in its start, and each try is a read of
        // its channel file and a refused connection on this computer, a millisecond (#3: every
        // 100 ms, the download waited up to that much longer than the app's start).
        std::thread::sleep(START_POLL);
        match mdm_ipc::send(cmd) {
            Ok(()) => return Ok(()),
            Err(mdm_ipc::SendError::Refused(e)) => return Err(Undelivered::Refused(e)),
            Err(mdm_ipc::SendError::NotRunning(_)) => {}
        }
        // It quit (another copy took over, or it failed to start): that one may answer still.
        // (`open` returns at once, the app being another process.)
        if !safari
            && child.try_wait().is_ok_and(|s| s.is_some())
            && started.elapsed() > START_WAIT / 4
        {
            break;
        }
    }
    Err(Undelivered::NotRunning(
        "Dannex Download Manager did not start".into(),
    ))
}

/// Start the app outside the browser's reach: browsers end a host's whole process tree (on
/// Windows a job object, elsewhere its process group) once it has answered, which would end an
/// app this host started along with it (MDN, "Native messaging": closing the native app).
///
/// It starts as the startup entries start it ([`mdm_ipc::STARTUP`]): its main window in the
/// notification area, so that only the window of the download handed over (Download File Info,
/// Download video…) comes up, as IDM's does; a plain start would show the main window with it.
pub(super) fn start_detached(app: &std::path::Path) -> io::Result<std::process::Child> {
    use std::process::{Command, Stdio};
    let mut cmd = Command::new(app);
    cmd.arg(mdm_ipc::STARTUP);
    // The browser's message pipes are this host's standard streams: never the app's.
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        let flags = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
        // Leaving the job is refused when the job doesn't allow it: then without.
        if let Ok(child) = cmd
            .creation_flags(flags | CREATE_BREAKAWAY_FROM_JOB)
            .spawn()
        {
            return Ok(child);
        }
        cmd.creation_flags(flags);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd.spawn()
}

pub(super) fn exe_dir() -> Result<PathBuf, String> {
    std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .map(PathBuf::from)
        .ok_or_else(|| "no exe dir".into())
}

/// When this host started (the browser starts one for each message it sends).
pub(super) fn started() -> std::time::Instant {
    static AT: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    *AT.get_or_init(std::time::Instant::now)
}

/// A message that gives the app something to download (a browser's download, a link, its
/// links, a video): its hand-over is timed (#3). Not a question, a setting or a notice.
pub(super) fn hands_over(m: &Message) -> bool {
    let question = m.rules || m.hello || m.panel || m.declined.is_some() || menu_ask(m).is_some();
    !question && (!m.url.is_empty() || !m.links.is_empty())
}

/// The log line for a hand-over: the host's start to the message, the time reading Options and
/// the time to the app's answer, all in milliseconds.
pub(super) fn handed_line(waited_ms: u128, rules_ms: u128, total_ms: u128) -> String {
    format!(
        "start time: browser integration: handed to DDM in {total_ms} ms ({rules_ms} ms reading Options; the message came {waited_ms} ms after the host started)"
    )
}

/// Serve the browser's messages for the copy of the extension that started this host
/// ([`copy_of`]: Test browser integration in the app names it).
pub(super) fn serve(copy: &str) -> io::Result<()> {
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    while let Some(raw) = read_message(&mut stdin)? {
        let raw = match raw {
            Ok(raw) => raw,
            Err(len) => {
                let reply = Reply::failed("too-large", format!("a message of {len} bytes"));
                write_message(
                    &mut stdout,
                    &serde_json::to_vec(&reply).map_err(io::Error::other)?,
                )?;
                continue;
            }
        };
        let read = std::time::Instant::now();
        let reply = match serde_json::from_slice::<Message>(&raw) {
            // The rules as saved now: a change in Options applies to the next download.
            Ok(mut m) => {
                let rules = mdm_ipc::capture::CaptureRules::load();
                let rules_ms = read.elapsed().as_millis();
                m.named = custom_browser(&rules);
                // This browser has the extension: the app stops offering it (extension.rs).
                mdm_ipc::extension::mark_seen(
                    browser(&m),
                    m.v.as_deref().unwrap_or_default(),
                    copy,
                );
                let hands = hands_over(&m);
                let reply = answer(m, &rules);
                // The host's part of the time from a click to the download (#3), for DDM's log:
                // never the address or the file's name. One left to the browser has its own line.
                if hands && reply.ok {
                    log_line(&handed_line(
                        read.duration_since(started()).as_millis(),
                        rules_ms,
                        read.elapsed().as_millis(),
                    ));
                }
                reply
            }
            Err(e) => Reply::failed("refused", e.to_string()),
        };
        write_message(
            &mut stdout,
            &serde_json::to_vec(&reply).map_err(io::Error::other)?,
        )?;
    }
    Ok(())
}
