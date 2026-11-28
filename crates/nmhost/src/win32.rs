//! The native host's Windows calls: letting the app come to the front, and finding the browser
//! that started it (Options → General → Add browser...).
#![allow(unsafe_code)]

use std::path::PathBuf;

/// Process `pid`'s parent, from a snapshot of the processes running.
pub fn parent_of(pid: u32) -> Option<u32> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };
    // SAFETY: a snapshot of every process; closed below.
    let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snap == INVALID_HANDLE_VALUE {
        return None;
    }
    // SAFETY: a zeroed entry is valid once its size is set, as the API asks.
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    let mut found = None;
    // SAFETY: the snapshot is open and `entry` is sized for the calls.
    let mut more = unsafe { Process32FirstW(snap, &mut entry) } != 0;
    while more {
        if entry.th32ProcessID == pid {
            found = Some(entry.th32ParentProcessID);
            break;
        }
        // SAFETY: as above.
        more = unsafe { Process32NextW(snap, &mut entry) } != 0;
    }
    // SAFETY: the snapshot's handle, closed once.
    unsafe { CloseHandle(snap) };
    found.filter(|p| *p != 0)
}

/// Process `pid`'s program (its full path), when this user may ask.
pub fn program_of(pid: u32) -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    // SAFETY: a handle for asking its name only; closed below.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return None;
    }
    let mut buf = vec![0u16; 32_768];
    let mut len = buf.len() as u32;
    // SAFETY: `buf` holds `len` characters; the call writes at most that many.
    let ok = unsafe {
        QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len)
    };
    // SAFETY: the process handle, closed once.
    unsafe { CloseHandle(process) };
    (ok != 0).then(|| PathBuf::from(std::ffi::OsString::from_wide(&buf[..len as usize])))
}

/// Let process `pid` (the app) bring its windows to the front. Windows refuses that to a program
/// in the background (the foreground lock), so a window opened at the browser's request (Options
/// from the video button, Download File Info) would open behind it. The browser is in front and
/// started this host, which lets the host pass that right on. Nothing happens when it can't.
pub fn allow_foreground(pid: u32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow;
    // SAFETY: a plain call with a process id; it fails harmlessly for a wrong or ended one.
    unsafe {
        AllowSetForegroundWindow(pid);
    }
}
