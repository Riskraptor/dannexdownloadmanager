//! Files and programs through the shell: sounds, Open with, the torrent handlers, a page opened in
//! a chosen browser, Mark-of-the-Web, renames that never replace, the Recycle Bin, and the program
//! that opens folders.

use super::*;

/// Play a .wav file without waiting (Options → Sounds).
pub fn play_sound(file: &str) {
    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_FILENAME, SND_NODEFAULT};
    let wide: Vec<u16> = file.encode_utf16().chain(Some(0)).collect();
    // SAFETY: a NUL-terminated path; SND_ASYNC copies what it needs before returning.
    unsafe {
        PlaySoundW(
            wide.as_ptr(),
            std::ptr::null_mut(),
            SND_FILENAME | SND_ASYNC | SND_NODEFAULT,
        );
    }
}

/// Windows' own "Open with" for `path` (the list's and Download complete's Open with...): the
/// system's chooser of apps, with its "Always use this app" (Windows keeps that choice, so Open
/// uses the app from then on), which then opens the file with the app picked. Waits until the
/// chooser is closed, so for a thread other than the window's. `Err` with the system's message
/// when it couldn't be shown; a chooser cancelled is `Ok`.
///
/// SHOpenWithDialog itself, not `rundll32 shell32.dll,OpenAs_RunDLL <path>`: that entry point
/// takes the rest of its command line as the path and showed nothing at all for a path with a
/// space, quoted or not (2026-11-22, Windows 11: rundll32 ended at once, no OpenWith.exe).
pub fn open_with(path: &std::path::Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::Com::{
        COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
    };
    use windows_sys::Win32::UI::Shell::{
        OAIF_ALLOW_REGISTRATION, OAIF_EXEC, OAIF_REGISTER_EXT, OPENASINFO, SHOpenWithDialog,
    };
    /// HRESULT_FROM_WIN32(ERROR_CANCELLED): the chooser closed without an app.
    const CANCELLED: i32 = 0x8007_04C7_u32 as i32;
    let file: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let info = OPENASINFO {
        pcszFile: file.as_ptr(),
        pcszClass: std::ptr::null(),
        // The "Always use this app" choice offered and kept, and the file opened with the app.
        oaifInFlags: OAIF_ALLOW_REGISTRATION | OAIF_REGISTER_EXT | OAIF_EXEC,
    };
    // SAFETY: COM for this thread (the shell's chooser wants a single-threaded apartment), undone
    // below when this call set it up; `info` and the NUL-terminated path it points to outlive the
    // call, which returns once the chooser is closed. No owner window: one of DDM's windows would
    // be disabled meanwhile from another thread.
    let hr = unsafe {
        let com = CoInitializeEx(
            std::ptr::null(),
            (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32,
        );
        let hr = SHOpenWithDialog(std::ptr::null_mut(), &info);
        if com >= 0 {
            CoUninitialize();
        }
        hr
    };
    if hr >= 0 || hr == CANCELLED {
        Ok(())
    } else {
        Err(std::io::Error::from_raw_os_error(hr).to_string())
    }
}

/// A program's own name for itself, as Explorer shows it (its version information's
/// FileDescription): "Vivaldi" for vivaldi.exe, for Options → General → Add browser.... None when
/// it has none.
pub fn program_description(program: &std::path::Path) -> Option<String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
    };
    let path: Vec<u16> = program.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut handle = 0u32;
    // SAFETY: a NUL-terminated path and a place for the (unused) handle.
    let size = unsafe { GetFileVersionInfoSizeW(path.as_ptr(), &mut handle) };
    if size == 0 {
        return None;
    }
    let mut data = vec![0u8; size as usize];
    // SAFETY: the buffer is `size` bytes long, as asked.
    let read = unsafe { GetFileVersionInfoW(path.as_ptr(), 0, size, data.as_mut_ptr().cast()) };
    if read == 0 {
        return None;
    }
    // A value of the block: where it is in `data`, and its length (in characters for strings).
    let query = |sub: &str| -> Option<(*const u16, usize)> {
        let sub: Vec<u16> = sub.encode_utf16().chain(Some(0)).collect();
        let mut at: *mut c_void = std::ptr::null_mut();
        let mut len = 0u32;
        // SAFETY: `data` holds the version block GetFileVersionInfoW filled; the answer points
        // into it and lives as long as it does.
        let ok = unsafe { VerQueryValueW(data.as_ptr().cast(), sub.as_ptr(), &mut at, &mut len) };
        (ok != 0 && !at.is_null() && len > 0).then_some((at as *const u16, len as usize))
    };
    // The file's first language and code page, then US English's (Unicode, then Windows-1252).
    let mut tables = Vec::new();
    if let Some((at, len)) = query("\\VarFileInfo\\Translation")
        && len >= 4
    {
        // SAFETY: the translation table's first entry: two 16-bit numbers within `data`.
        let pair = unsafe { std::slice::from_raw_parts(at, 2) };
        tables.push(format!("{:04x}{:04x}", pair[0], pair[1]));
    }
    tables.extend(["040904b0".to_owned(), "040904e4".to_owned()]);
    tables.into_iter().find_map(|t| {
        let (at, len) = query(&format!("\\StringFileInfo\\{t}\\FileDescription"))?;
        // SAFETY: `len` UTF-16 characters within `data`.
        let text = unsafe { std::slice::from_raw_parts(at, len) };
        let text = String::from_utf16_lossy(text);
        let text = text.trim_end_matches('\0').trim();
        (!text.is_empty()).then(|| text.to_owned())
    })
}

/// Options → General → "Launch … on startup": our entry under HKCU\…\Run (set or removed).
/// Make DDM (on) or stop it being (off) what Windows opens magnet links and .torrent files
/// with, for the current user (HKCU\Software\Classes; no admin). Off removes only what points
/// at us. Windows keeps a choice the user made in "Open with" above this (it can't be set by
/// programs), so another app chosen there stays in charge until the user picks DDM.
pub fn set_torrent_handlers(on: bool) {
    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegDeleteTreeW, RegGetValueW,
        RegSetKeyValueW,
    };
    const PROGID: &str = "DannexDownloadManager.Torrent";
    // An off-screen copy is a test's (crate::offscreen): the user's associations are never its
    // to change, as the Run value isn't (`run_value`).
    if crate::offscreen() {
        tracing::info!("torrent associations {on}: off-screen, not written");
        return;
    }
    let w = |s: &str| -> Vec<u16> { s.encode_utf16().chain(Some(0)).collect() };
    let set = |key: &str, name: Option<&str>, value: &str| {
        let (key, value) = (w(key), w(value));
        let name = name.map(w);
        // SAFETY: NUL-terminated strings; the size covers the value's bytes.
        unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ref().map_or(std::ptr::null(), |n| n.as_ptr()),
                REG_SZ,
                value.as_ptr() as *const c_void,
                (value.len() * 2) as u32,
            );
        }
    };
    let get = |key: &str, name: Option<&str>| -> Option<String> {
        let key = w(key);
        let name = name.map(w);
        let mut buf = vec![0u16; 1024];
        let mut len = (buf.len() * 2) as u32;
        // SAFETY: NUL-terminated key/name; `len` is the buffer's size in bytes.
        let r = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ref().map_or(std::ptr::null(), |n| n.as_ptr()),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr() as *mut c_void,
                &mut len,
            )
        };
        (r == 0).then(|| String::from_utf16_lossy(&buf[..(len as usize / 2).saturating_sub(1)]))
    };
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let exe = exe.display().to_string();
    let open = format!("\"{exe}\" \"%1\"");
    let icon = format!("\"{exe}\",0");
    if on {
        set("Software\\Classes\\magnet", None, "URL:Magnet link");
        set("Software\\Classes\\magnet", Some("URL Protocol"), "");
        set("Software\\Classes\\magnet\\DefaultIcon", None, &icon);
        set(
            "Software\\Classes\\magnet\\shell\\open\\command",
            None,
            &open,
        );
        let prog = format!("Software\\Classes\\{PROGID}");
        set(&prog, None, "Torrent file");
        set(&format!("{prog}\\DefaultIcon"), None, &icon);
        set(&format!("{prog}\\shell\\open\\command"), None, &open);
        set("Software\\Classes\\.torrent", None, PROGID);
    } else {
        let ours = get("Software\\Classes\\magnet\\shell\\open\\command", None)
            .is_some_and(|c| c.contains(&exe));
        let tree = |k: &str| {
            let k = w(k);
            // SAFETY: NUL-terminated key under HKCU.
            unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, k.as_ptr()) };
        };
        if ours {
            tree("Software\\Classes\\magnet");
        }
        tree(&format!("Software\\Classes\\{PROGID}"));
        if get("Software\\Classes\\.torrent", None).as_deref() == Some(PROGID) {
            let (k, n) = (w("Software\\Classes\\.torrent"), w(""));
            // SAFETY: NUL-terminated key; an empty name is the default value.
            unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, k.as_ptr(), n.as_ptr()) };
        }
    }
    // Tell Explorer the associations changed.
    // SAFETY: no pointers are passed.
    unsafe {
        windows_sys::Win32::UI::Shell::SHChangeNotify(
            windows_sys::Win32::UI::Shell::SHCNE_ASSOCCHANGED as i32,
            windows_sys::Win32::UI::Shell::SHCNF_IDLIST,
            std::ptr::null(),
            std::ptr::null(),
        );
    }
}

/// Move `files` to the Recycle Bin, silently (no confirmation, no progress, no error box): an
/// archive and its volumes after extracting (#37), restorable from there. False when the shell
/// refused or stopped part way; the files left are kept.
pub fn to_recycle_bin(files: &[std::path::PathBuf]) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::{
        FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, SHFILEOPSTRUCTW,
        SHFileOperationW,
    };
    if files.is_empty() {
        return true;
    }
    // Whole paths, each ended by a NUL, the list by another (the shell's own format).
    let mut from: Vec<u16> = Vec::new();
    for f in files {
        let whole = std::path::absolute(f).unwrap_or_else(|_| f.clone());
        from.extend(whole.as_os_str().encode_wide());
        from.push(0);
    }
    from.push(0);
    let mut op = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE,
        pFrom: from.as_ptr(),
        fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT) as u16,
        ..Default::default()
    };
    // SAFETY: `op` points at `from`, a double-NUL-ended list alive for the call; no window.
    let r = unsafe { SHFileOperationW(&mut op) };
    r == 0 && op.fAnyOperationsAborted == 0
}

/// Open `url` in the browser whose program is `program` (from `browsers`).
pub fn open_in(program: &std::path::Path, url: &str) {
    let _ = std::process::Command::new(program).arg(url).spawn();
}

/// Mark-of-the-Web on a file being downloaded from `url` (`referrer`'s page): the zone stream
/// browsers write, so SmartScreen checks a program, Office opens a document in Protected View and
/// archive tools pass the mark on, as for the browser's own download. Written on the part file
/// as it is created (the engine renames it, the stream going along).
pub fn mark_downloaded(path: &std::path::Path, url: &str, referrer: Option<&str>) {
    // Only addresses from the web: never a login's password (the engine has none in `url`).
    let clean = |u: &str| u.split(['\r', '\n']).next().unwrap_or("").to_owned();
    let mut text = String::from("[ZoneTransfer]\r\nZoneId=3\r\n");
    if let Some(r) = referrer.filter(|r| r.starts_with("http")) {
        text.push_str(&format!("ReferrerUrl={}\r\n", clean(r)));
    }
    text.push_str(&format!("HostUrl={}\r\n", clean(url)));
    let mut stream = path.as_os_str().to_owned();
    stream.push(":Zone.Identifier");
    let _ = std::fs::write(stream, text);
}

/// Rename `from` to `to` without ever replacing an existing entry (`MoveFileExW` without
/// `MOVEFILE_REPLACE_EXISTING`, returning `ErrorKind::AlreadyExists` when `to` is already there).
pub fn rename_no_replace(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::MoveFileExW;
    let encode = |p: &std::path::Path| -> std::io::Result<Vec<u16>> {
        let mut wide: Vec<u16> = p.as_os_str().encode_wide().collect();
        if wide.contains(&0) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "interior NUL",
            ));
        }
        wide.push(0);
        Ok(wide)
    };
    let from_w = encode(from)?;
    let to_w = encode(to)?;
    // SAFETY: both UTF-16 buffers are NUL-terminated and live through the call; flags = 0 refuses
    // cross-volume copies and refuses replacing an existing destination.
    let ok = unsafe { MoveFileExW(from_w.as_ptr(), to_w.as_ptr(), 0) };
    if ok != 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

/// The program Windows runs to open folders, when that isn't Explorer (a file manager such as
/// Directory Opus set up to replace it, #446); None for Explorer, or when Windows doesn't say.
pub fn folder_opener() -> Option<String> {
    use windows_sys::Win32::UI::Shell::{ASSOCF_NONE, ASSOCSTR_EXECUTABLE, AssocQueryStringW};
    other_folder_opener(|class, verb| {
        let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
        let class = wide(class);
        let verb = verb.map(wide);
        let mut out = vec![0u16; 1024];
        let mut len = out.len() as u32;
        // SAFETY: the strings end in NUL and outlive the call; `len` is `out`'s size in
        // characters, and the call writes at most that many.
        let hr = unsafe {
            AssocQueryStringW(
                ASSOCF_NONE,
                ASSOCSTR_EXECUTABLE,
                class.as_ptr(),
                verb.as_ref().map_or(std::ptr::null(), |v| v.as_ptr()),
                out.as_mut_ptr(),
                &mut len,
            )
        };
        (hr == 0).then(|| {
            let end = out.iter().position(|&c| c == 0).unwrap_or(out.len());
            String::from_utf16_lossy(&out[..end])
        })
    })
}

/// Who opens a folder, asked of `executable_for(class, verb)` as the shell reads it: a folder is
/// a `Directory` first (where a replacement registers its verb, as the default or as `open`),
/// then a `Folder` (where Windows registers Explorer). An installed program's other verbs there
/// ("Open with Code") are not the default and aren't asked for.
pub(super) fn other_folder_opener(
    executable_for: impl Fn(&str, Option<&str>) -> Option<String>,
) -> Option<String> {
    let opener = [
        ("Directory", None),
        ("Directory", Some("open")),
        ("Folder", None),
    ]
    .into_iter()
    .find_map(|(class, verb)| executable_for(class, verb))?;
    let name = opener.rsplit(['\\', '/']).next().unwrap_or(&opener);
    (!name.is_empty() && !name.eq_ignore_ascii_case("explorer.exe")).then_some(opener)
}

#[cfg(test)]
mod open_with_tests {
    use std::time::{Duration, Instant};

    /// The ids of the OpenWith.exe processes running (the chooser's host on Windows 10 and 11).
    fn choosers() -> Vec<u32> {
        let out = std::process::Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq OpenWith.exe", "/FO", "CSV", "/NH"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| l.split("\",\"").nth(1)?.parse().ok())
            .collect()
    }

    /// The list's and Download complete's Open with...: Windows' chooser comes up for a file
    /// whose folder and name have spaces (rundll32's OpenAs_RunDLL showed nothing for one), and
    /// is closed again by the test. It shows a system window for a moment, so it runs on request:
    /// `cargo test -p mdm-app open_with -- --ignored`.
    #[test]
    #[ignore = "shows Windows' Open with chooser for a moment; run with --ignored"]
    fn the_chooser_comes_up_for_a_path_with_spaces() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("a folder with spaces");
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("a sample file.ddmtest");
        std::fs::write(&file, b"x").unwrap();
        let before = choosers();
        let asked = file.clone();
        let call = std::thread::spawn(move || super::open_with(&asked));
        let until = Instant::now() + Duration::from_secs(15);
        let new = loop {
            let new: Vec<u32> = choosers()
                .into_iter()
                .filter(|p| !before.contains(p))
                .collect();
            if !new.is_empty() || Instant::now() > until {
                break new;
            }
            std::thread::sleep(Duration::from_millis(200));
        };
        for pid in &new {
            let _ = std::process::Command::new("taskkill")
                .args(["/F", "/PID", &pid.to_string()])
                .output();
        }
        assert!(!new.is_empty(), "no Open with chooser came up");
        // Closed, the call returns (cancelled, or the system's error for the chooser ended).
        let _ = call.join().unwrap();
    }
}

#[cfg(test)]
mod folder_opener_tests {
    use super::other_folder_opener;

    const EXPLORER: &str = r"C:\Windows\Explorer.exe";
    const OPUS: &str = r"C:\Program Files\GPSoftware\Directory Opus\dopusrt.exe";

    /// #446: as Windows is shipped (what `reg query` shows on a clean system), Explorer it is.
    #[test]
    fn explorer_as_shipped_is_no_other_opener() {
        let shipped =
            |c: &str, v: Option<&str>| (c == "Folder" && v.is_none()).then(|| EXPLORER.to_owned());
        assert_eq!(other_folder_opener(shipped), None);
    }

    /// A replacement's default verb on `Directory` wins over Explorer on `Folder`.
    #[test]
    fn a_replacement_on_directory_is_the_opener() {
        let opus = |c: &str, v: Option<&str>| match (c, v) {
            ("Directory", None) => Some(OPUS.to_owned()),
            ("Folder", None) => Some(EXPLORER.to_owned()),
            _ => None,
        };
        assert_eq!(other_folder_opener(opus).as_deref(), Some(OPUS));
    }

    /// One that replaces `Directory`'s `open` verb, or `Folder`'s own.
    #[test]
    fn a_replaced_open_verb_or_folder_is_the_opener() {
        let open = |c: &str, v: Option<&str>| match (c, v) {
            ("Directory", Some("open")) => Some(r"C:\Apps\files.exe".to_owned()),
            ("Folder", None) => Some(EXPLORER.to_owned()),
            _ => None,
        };
        assert_eq!(
            other_folder_opener(open).as_deref(),
            Some(r"C:\Apps\files.exe")
        );
        let folder = |c: &str, v: Option<&str>| {
            (c == "Folder" && v.is_none()).then(|| r"C:\Apps\tc.exe".to_owned())
        };
        assert_eq!(
            other_folder_opener(folder).as_deref(),
            Some(r"C:\Apps\tc.exe")
        );
    }

    /// Explorer written any way stays Explorer; nothing registered, nothing to say.
    #[test]
    fn explorer_any_case_or_no_answer_is_no_other_opener() {
        let upper = |_: &str, _: Option<&str>| Some(r"C:\WINDOWS\EXPLORER.EXE".to_owned());
        assert_eq!(other_folder_opener(upper), None);
        assert_eq!(other_folder_opener(|_, _| None), None);
    }
}
