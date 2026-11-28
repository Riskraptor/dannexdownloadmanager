//! Who started the host: the browser (its program, through a shell that started it), and the copy
//! of the extension it is for.

use super::*;

/// The programs a browser may start this host through, rather than directly: Chrome on Windows
/// starts it with cmd.exe; a shell or env on Linux and macOS.
pub(super) const SHELLS: [&str; 8] = [
    "cmd.exe",
    "conhost.exe",
    "sh",
    "bash",
    "dash",
    "zsh",
    "env",
    "flatpak-spawn",
];

/// The name a download will have: the browser's, else the address's last part.
pub(super) fn file_name_of(m: &Message) -> String {
    let from_browser = m
        .filename
        .as_deref()
        .and_then(|f| f.rsplit(['\\', '/']).next())
        .filter(|f| !f.is_empty());
    match from_browser {
        Some(f) => f.to_owned(),
        None => m
            .url
            .split(['?', '#'])
            .next()
            .unwrap_or("")
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_owned(),
    }
}

/// The browser `m` comes from, by the names Options lists: one the user added, known by its
/// program (`named`), else by its User-Agent and brands.
pub(super) fn browser(m: &Message) -> &'static str {
    m.named.unwrap_or_else(|| {
        mdm_ipc::capture::browser_named(m.user_agent.as_deref().unwrap_or_default(), &m.brands)
    })
}

/// The name of the browser the user added (Add browser...) that started this host, when the
/// program that started it is one of theirs (`rules.custom_name`).
pub(super) fn custom_browser(rules: &mdm_ipc::capture::CaptureRules) -> Option<&'static str> {
    // Looked for only when the user added a browser: finding the program that started this host
    // walks a snapshot of every process running, up to five times, which took most of the host's
    // 20 ms for each download handed over (#3), for a list that is nearly always empty.
    if rules.custom_browsers.is_empty() {
        return None;
    }
    let name = rules.custom_name(caller_program()?)?;
    Some(intern(name))
}

/// `name`, kept for the host's life (a host serves one browser: a name or two at most).
pub(super) fn intern(name: &str) -> &'static str {
    static NAMES: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());
    let mut names = NAMES.lock().unwrap();
    if let Some(n) = names.iter().find(|n| **n == name) {
        return n;
    }
    let n: &'static str = Box::leak(name.to_owned().into_boxed_str());
    names.push(n);
    n
}

/// The browser that started this host: its program, past the shells some start it through.
/// Found once; None when the system doesn't say.
pub(super) fn caller_program() -> Option<&'static Path> {
    static PROGRAM: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    PROGRAM
        .get_or_init(|| {
            let mut pid = parent_of(std::process::id())?;
            for _ in 0..4 {
                let program = program_of(pid)?;
                let name = program.file_name()?.to_string_lossy().to_lowercase();
                if !SHELLS.contains(&name.as_str()) {
                    return Some(program);
                }
                pid = parent_of(pid)?;
            }
            None
        })
        .as_deref()
}

/// Process `pid`'s parent.
#[cfg(windows)]
pub(super) fn parent_of(pid: u32) -> Option<u32> {
    win32::parent_of(pid)
}

/// Process `pid`'s program.
#[cfg(windows)]
pub(super) fn program_of(pid: u32) -> Option<PathBuf> {
    win32::program_of(pid)
}

/// Process `pid`'s parent: /proc's `stat` (its fourth field, after the name in parentheses).
#[cfg(target_os = "linux")]
pub(super) fn parent_of(pid: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    stat.rsplit_once(')')?
        .1
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

/// Process `pid`'s program: /proc's `exe` link.
#[cfg(target_os = "linux")]
pub(super) fn program_of(pid: u32) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/exe")).ok()
}

/// Process `pid`'s parent (macOS: as `ps` reports it; this host's own from the system).
#[cfg(target_os = "macos")]
pub(super) fn parent_of(pid: u32) -> Option<u32> {
    if pid == std::process::id() {
        return Some(std::os::unix::process::parent_id());
    }
    ps_field(pid, "ppid=")?.parse().ok()
}

/// Process `pid`'s program (macOS: `ps`'s `comm`, its full path).
#[cfg(target_os = "macos")]
pub(super) fn program_of(pid: u32) -> Option<PathBuf> {
    ps_field(pid, "comm=").map(PathBuf::from)
}

/// One field of `ps` for process `pid`.
#[cfg(target_os = "macos")]
pub(super) fn ps_field(pid: u32, field: &str) -> Option<String> {
    let out = std::process::Command::new("ps")
        .args(["-o", field, "-p", &pid.to_string()])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (out.status.success() && !text.is_empty()).then_some(text)
}

/// A Chromium extension's origin, as browsers name the caller.
pub(super) fn origin(id: &str) -> String {
    format!("chrome-extension://{id}/")
}

/// Is this the caller the browsers start us for? Chromium passes the extension's origin first
/// (unpacked or from the store); Firefox passes the manifest's path, then the add-on ID.
pub(super) fn our_caller(args: &[String]) -> bool {
    args.first()
        .is_some_and(|a| chromium_ids().iter().any(|id| *a == origin(id)))
        || args.get(1).is_some_and(|a| a == FIREFOX_ID)
}

/// Which copy of the extension the browser started this host for (one of ours: `our_caller`).
pub(super) fn caller_copy(args: &[String]) -> &'static str {
    let chromium = args.first().and_then(|a| {
        chromium_ids()
            .into_iter()
            .find(|id| *a == origin(id))
            .map(copy_of)
    });
    chromium.unwrap_or_else(|| {
        if args.get(1).is_some_and(|a| a == FIREFOX_ID) {
            "firefox"
        } else {
            ""
        }
    })
}
