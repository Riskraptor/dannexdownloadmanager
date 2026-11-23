//! Options → Downloads → Virus checking settings: a finished download's file, of the types listed,
//! is handed to the antivirus program the user chose, with its arguments (`[FILE]` stands for the
//! file), beside the window. The program's answer is its exit code: 0 found nothing; anything
//! else is a problem it reports (a threat, or an error of its own). The result goes in the
//! download's log, and a problem into its Download complete window (or a message when that
//! window isn't open).
//!
//! Presets: Microsoft Defender's command line on Windows (`MpCmdRun.exe -Scan -ScanType 3 -File
//! "[FILE]" -DisableRemediation`: it reports, and leaves the file where it is), ClamAV's
//! `clamscan` on Linux and macOS; each offered only when it is there.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// What stands for the file in the arguments.
pub const FILE: &str = "[FILE]";
/// The arguments a program starts with: the file alone.
pub const DEFAULT_ARGS: &str = "\"[FILE]\"";
/// The types checked at first: programs, installers and packages, archives and disk images, and
/// documents that can carry macros or scripts.
pub const DEFAULT_TYPES: &str = "EXE MSI MSIX MSP APPX APPXBUNDLE CAB ZIP RAR 7Z ISO IMG VHD VHDX JAR APK DMG PKG DEB RPM APPIMAGE RUN SH DOC DOCM XLS XLSM PPT PPTM PDF RTF JS VBS PS1 BAT CMD SCR";
/// The longest a check may take before it is ended (a scan of a large archive takes minutes).
const LIMIT: Duration = Duration::from_secs(30 * 60);

/// A program offered with one click: its name, where it is, its arguments.
pub struct Preset {
    pub name: &'static str,
    pub program: PathBuf,
    pub args: &'static str,
}

/// The presets found on this computer.
pub fn presets() -> Vec<Preset> {
    let mut out = Vec::new();
    #[cfg(windows)]
    if let Some(program) = defender() {
        out.push(Preset {
            // A product's name: not translated.
            name: "Microsoft Defender",
            program,
            args: "-Scan -ScanType 3 -File \"[FILE]\" -DisableRemediation",
        });
    }
    #[cfg(unix)]
    if let Some(program) = in_path("clamscan") {
        out.push(Preset {
            // A product's name: not translated.
            name: "ClamAV",
            program,
            args: "--no-summary \"[FILE]\"",
        });
    }
    out
}

/// Microsoft Defender's command-line scanner: the one under Program Files, else the newest of
/// its platform updates (ProgramData\Microsoft\Windows Defender\Platform\<version>).
#[cfg(windows)]
fn defender() -> Option<PathBuf> {
    let fixed = std::env::var_os("ProgramFiles")
        .map(|p| {
            PathBuf::from(p)
                .join("Windows Defender")
                .join("MpCmdRun.exe")
        })
        .filter(|p| p.is_file());
    fixed.or_else(|| {
        let platform = PathBuf::from(std::env::var_os("ProgramData")?)
            .join("Microsoft")
            .join("Windows Defender")
            .join("Platform");
        let mut versions: Vec<PathBuf> = std::fs::read_dir(platform)
            .ok()?
            .flatten()
            .map(|e| e.path().join("MpCmdRun.exe"))
            .filter(|p| p.is_file())
            .collect();
        versions.sort();
        versions.pop()
    })
}

/// A program in one of PATH's folders.
#[cfg(unix)]
fn in_path(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}

/// Should the finished file `name` be checked, as these settings say?
pub fn applies(s: &crate::settings::Settings, name: &str) -> bool {
    s.virus_check
        && !s.virus_program.trim().is_empty()
        && mdm_ipc::capture::type_listed(&s.virus_types, name)
}

/// The arguments as one command line (Windows hands a program its command line whole): `[FILE]`
/// becomes the file, in quotes (quotes typed around it are kept, not doubled); without `[FILE]`
/// the file comes last. Windows paths can't hold a quote.
// Linux and macOS hand the arguments one by one (`arguments`); tested everywhere.
#[cfg_attr(unix, allow(dead_code))]
pub fn command_line(args: &str, file: &Path) -> String {
    let quoted = format!("\"{}\"", file.display());
    let args = args.trim().replace("\"[FILE]\"", FILE);
    if args.contains(FILE) {
        args.replace(FILE, &quoted)
    } else if args.is_empty() {
        quoted
    } else {
        format!("{args} {quoted}")
    }
}

/// The arguments one by one (Linux and macOS): split at spaces outside double quotes, `[FILE]`
/// replaced by the file's path within its argument; without `[FILE]`, the file last.
// Windows hands the command line whole (`command_line`); tested everywhere.
#[cfg_attr(windows, allow(dead_code))]
pub fn arguments(args: &str, file: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let (mut quoted, mut any) = (false, false);
    for c in args.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any || !word.is_empty() {
                    out.push(std::mem::take(&mut word));
                }
                any = false;
            }
            c => word.push(c),
        }
    }
    if any || !word.is_empty() {
        out.push(word);
    }
    let path = file.display().to_string();
    if out.iter().any(|a| a.contains(FILE)) {
        out.iter().map(|a| a.replace(FILE, &path)).collect()
    } else {
        out.push(path);
        out
    }
}

/// What a check came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Exit code 0: nothing found.
    Clean,
    /// Another exit code: a threat found, or an error the program reports.
    Problem(i32),
    /// The program couldn't be started (its message).
    NotRun(String),
    /// It ran past `LIMIT` and was ended.
    TimedOut,
}

/// A check's end, for download `id`.
pub struct Done {
    pub id: u64,
    pub verdict: Verdict,
    /// The program that checked it, as the settings named it then.
    pub program: String,
}

/// The program's name as the messages give it ("MpCmdRun.exe").
pub fn program_name(program: &str) -> String {
    Path::new(program.trim())
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| program.trim().to_owned())
}

/// The verdict in words, for the download's log and Download complete.
pub fn text(verdict: &Verdict, program: &str) -> String {
    let name = program_name(program);
    match verdict {
        Verdict::Clean => crate::i18n::fmt(
            // i18n: a download's log: the antivirus program ({program}, its file name) checked the finished file and found nothing
            crate::i18n::t("Virus check: {program} found nothing"),
            &[("program", &name)],
        ),
        Verdict::Problem(code) => crate::i18n::fmt(
            // i18n: a download's log and Download complete: the antivirus program ({program}) reported a problem with the finished file (a threat, or an error of its own); {code} is its exit code
            crate::i18n::t("Virus check: {program} reported a problem (exit code {code})"),
            &[("program", &name), ("code", &code.to_string())],
        ),
        Verdict::NotRun(e) => crate::i18n::fmt(
            // i18n: a download's log and Download complete: the antivirus program ({program}) set in Options couldn't be started; {error} is the system's message
            crate::i18n::t("Virus check: {program} couldn't be started: {error}"),
            &[("program", &name), ("error", e)],
        ),
        Verdict::TimedOut => crate::i18n::fmt(
            // i18n: a download's log and Download complete: the antivirus program ({program}) took more than 30 minutes and was stopped
            crate::i18n::t("Virus check: {program} didn't finish within 30 minutes"),
            &[("program", &name)],
        ),
    }
}

/// Check `file` (download `id`) with `program` and `args`, beside the window: the result comes
/// through `tx`, and the window is woken for it.
pub fn start(
    id: u64,
    file: PathBuf,
    program: String,
    args: String,
    tx: std::sync::mpsc::Sender<Done>,
    ctx: egui::Context,
) {
    crate::launch::spawn("virus check", move || {
        let verdict = run(&program, &args, &file, LIMIT);
        tracing::info!("virus check of download {id}: {verdict:?}");
        let _ = tx.send(Done {
            id,
            verdict,
            program,
        });
        ctx.request_repaint();
    });
}

/// Run the check and wait for it, `limit` at most.
fn run(program: &str, args: &str, file: &Path, limit: Duration) -> Verdict {
    let mut cmd = Command::new(program.trim());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.raw_arg(command_line(args, file));
        // A console program (MpCmdRun) shows no window flashing up (CREATE_NO_WINDOW).
        cmd.creation_flags(0x0800_0000);
    }
    #[cfg(unix)]
    cmd.args(arguments(args, file));
    let child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => return Verdict::NotRun(e.to_string()),
    };
    let until = Instant::now() + limit;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return match status.code() {
                    Some(0) => Verdict::Clean,
                    Some(code) => Verdict::Problem(code),
                    // Ended by a signal (Linux, macOS).
                    None => Verdict::Problem(-1),
                };
            }
            Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(250)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Verdict::TimedOut;
            }
            Err(e) => return Verdict::NotRun(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_takes_the_placeholders_place_quoted_once() {
        let f = Path::new(r"C:\Down loads\a b.exe");
        assert_eq!(
            command_line("-Scan -ScanType 3 -File \"[FILE]\" -DisableRemediation", f),
            r#"-Scan -ScanType 3 -File "C:\Down loads\a b.exe" -DisableRemediation"#
        );
        assert_eq!(
            command_line("/scan [FILE]", f),
            r#"/scan "C:\Down loads\a b.exe""#
        );
        assert_eq!(
            command_line("/quiet", f),
            r#"/quiet "C:\Down loads\a b.exe""#
        );
        assert_eq!(command_line("", f), r#""C:\Down loads\a b.exe""#);
    }

    #[test]
    fn arguments_split_outside_quotes() {
        let f = Path::new("/home/me/Down loads/a b.zip");
        assert_eq!(
            arguments("--no-summary \"[FILE]\"", f),
            ["--no-summary", "/home/me/Down loads/a b.zip"]
        );
        assert_eq!(
            arguments("--log=\"/tmp/my log\" --file=[FILE]", f),
            ["--log=/tmp/my log", "--file=/home/me/Down loads/a b.zip"]
        );
        assert_eq!(arguments("-r", f), ["-r", "/home/me/Down loads/a b.zip"]);
        assert_eq!(arguments("", f), ["/home/me/Down loads/a b.zip"]);
        assert_eq!(
            arguments("\"\" x", f),
            ["", "x", "/home/me/Down loads/a b.zip"]
        );
    }

    #[test]
    fn only_listed_types_are_checked_and_only_when_on() {
        let mut s = crate::settings::Settings::default();
        assert!(!applies(&s, "setup.exe"), "off by default");
        s.virus_check = true;
        assert!(!applies(&s, "setup.exe"), "no program");
        s.virus_program = "scan.exe".into();
        assert!(applies(&s, "setup.exe") && applies(&s, "report.DOCM"));
        assert!(!applies(&s, "movie.mkv") && !applies(&s, "noext"));
    }

    /// A program's exit code is its answer; one that can't start says why.
    #[test]
    fn the_exit_code_is_the_verdict() {
        let file = std::env::temp_dir().join("ddm virus check.txt");
        std::fs::write(&file, b"x").unwrap();
        #[cfg(windows)]
        let (clean, problem) = (
            ("cmd", "/c exit 0 [FILE]"),
            ("cmd", "/c if exist [FILE] exit 2"),
        );
        #[cfg(unix)]
        let (clean, problem) = (
            ("sh", "-c \"exit 0\" [FILE]"),
            ("sh", "-c \"exit 2\" [FILE]"),
        );
        let wait = Duration::from_secs(30);
        assert_eq!(run(clean.0, clean.1, &file, wait), Verdict::Clean);
        assert_eq!(run(problem.0, problem.1, &file, wait), Verdict::Problem(2));
        assert!(matches!(
            run("ddm-no-such-antivirus", "", &file, wait),
            Verdict::NotRun(_)
        ));
        let _ = std::fs::remove_file(&file);
    }
}
