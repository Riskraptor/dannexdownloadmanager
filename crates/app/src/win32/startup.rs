//! Starting with the session: the Run key's command, kept up to date with the program's place.

use super::*;

pub(super) const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// The Run key's value DDM keeps its startup entry in (Options → "Launch on startup", the
/// installer's "Start when Windows starts"). An off-screen test copy never touches the user's:
/// it uses `MDM_TEST_RUN_VALUE`'s name when given one, else none at all (#226's checks).
pub(super) fn run_value() -> Option<String> {
    if crate::offscreen() {
        return std::env::var("MDM_TEST_RUN_VALUE")
            .ok()
            .filter(|v| v.starts_with("DannexDownloadManagerTest"));
    }
    Some("DannexDownloadManager".to_owned())
}

/// The startup entry's command for the program `exe`: started with the session, DDM waits in
/// the notification area (`/startup`, #226).
pub(super) fn startup_command(exe: &std::path::Path) -> String {
    format!("\"{}\" {}", exe.display(), mdm_ipc::STARTUP)
}

/// A startup entry `value` (a command line) written before #226 for the program `exe`: the same
/// program, without the argument. Another program's entry (a copy of DDM elsewhere) is not this
/// one's to rewrite.
pub(super) fn startup_outdated(value: &str, exe: &std::path::Path) -> bool {
    // The program compared as Windows compares paths: in any case, either slash.
    let norm = |s: &str| s.replace('/', "\\").to_lowercase();
    let exe = norm(&exe.display().to_string());
    let value = value.trim();
    let args = match value.strip_prefix('"') {
        Some(rest) => match rest.split_once('"') {
            Some((program, args)) if norm(program) == exe => args,
            _ => return false,
        },
        // Unquoted (written by hand): the program's path, spaces and all, then its arguments.
        None => match value.get(..exe.len()).zip(value.get(exe.len()..)) {
            Some((program, args))
                if norm(program) == exe && (args.is_empty() || args.starts_with(' ')) =>
            {
                args
            }
            _ => return false,
        },
    };
    !args
        .split_whitespace()
        .any(|a| a.eq_ignore_ascii_case(mdm_ipc::STARTUP))
}

/// Is Dannex Download Manager in the user's Run key (set by Options or by the installer's
/// "Start when Windows starts")? The registry is the truth for Options → "Launch on startup".
pub fn run_at_startup() -> bool {
    run_value().is_some_and(|name| {
        reg_string_in(
            windows_sys::Win32::System::Registry::HKEY_CURRENT_USER,
            RUN_KEY,
            &name,
        )
        .is_some()
    })
}

/// The startup entry rewritten with its argument when it was written without (by versions
/// before 0.5.0, or by their installer) for this very program (#226).
pub fn update_run_at_startup() {
    let (Some(name), Ok(exe)) = (run_value(), std::env::current_exe()) else {
        return;
    };
    let value = reg_string_in(
        windows_sys::Win32::System::Registry::HKEY_CURRENT_USER,
        RUN_KEY,
        &name,
    );
    if value.is_some_and(|v| startup_outdated(&v, &exe)) {
        tracing::info!("startup entry written again, to start in the notification area");
        set_run_at_startup(true);
    }
}

pub fn set_run_at_startup(on: bool) {
    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegSetKeyValueW,
    };
    let Some(name) = run_value() else {
        return;
    };
    let key: Vec<u16> = RUN_KEY.encode_utf16().chain(Some(0)).collect();
    let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    if on {
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let value: Vec<u16> = startup_command(&exe)
            .encode_utf16()
            .chain(Some(0))
            .collect();
        // SAFETY: NUL-terminated key, name and value; the size covers the value's bytes.
        unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                REG_SZ,
                value.as_ptr() as *const c_void,
                (value.len() * 2) as u32,
            );
        }
    } else {
        // SAFETY: NUL-terminated key and value name.
        unsafe {
            RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr());
        }
    }
}

#[cfg(test)]
mod startup_tests {
    use super::{startup_command, startup_outdated};
    use std::path::Path;

    const EXE: &str = r"C:\Program Files\Dannex Download Manager\dannexdownloadmanager.exe";

    /// The entry Options and the installer write (#226): the program, quoted, then `/startup`.
    #[test]
    fn the_startup_entry_starts_mdm_in_the_tray() {
        let cmd = startup_command(Path::new(EXE));
        assert_eq!(
            cmd,
            format!("\"{EXE}\" /startup"),
            "as tools/installer/mdm.iss writes it too"
        );
        let args = mdm_ipc::parse_args(&["/startup".to_owned()]).unwrap();
        assert!(args.startup);
        assert!(!startup_outdated(&cmd, Path::new(EXE)));
    }

    /// An entry of a version before 0.5.0, or of its installer, is written again; one for another
    /// copy of DDM, or one already up to date, is left alone.
    #[test]
    fn old_startup_entries_are_rewritten() {
        let exe = Path::new(EXE);
        // Options before 0.5.0, and the installer's: the program alone, quoted.
        assert!(startup_outdated(&format!("\"{EXE}\""), exe));
        // Written by hand: unquoted, in another case, with other slashes.
        assert!(startup_outdated(
            r"c:\program files\dannex download manager\DANNEXDOWNLOADMANAGER.EXE",
            exe
        ));
        assert!(startup_outdated(
            &format!("\"{}\"", EXE.replace('\\', "/").to_uppercase()),
            exe
        ));
        assert!(!startup_outdated(&format!("{EXE}2"), exe));
        // Already with it (any case).
        assert!(!startup_outdated(&format!("\"{EXE}\" /startup"), exe));
        assert!(!startup_outdated(&format!("\"{EXE}\" /STARTUP"), exe));
        // Another copy's (a portable one, a build elsewhere).
        assert!(!startup_outdated(
            r#""D:\mdm\dannexdownloadmanager.exe""#,
            exe
        ));
        assert!(!startup_outdated("", exe));
    }

    /// The installer's "Start when Windows starts" writes the entry Options writes.
    #[test]
    fn the_installer_writes_the_same_entry() {
        let iss = include_str!("../../../../tools/installer/mdm.iss");
        assert!(iss.contains(
            r#"ValueName: "DannexDownloadManager"; ValueData: """{app}\{#AppExe}"" /startup""#
        ));
    }
}
