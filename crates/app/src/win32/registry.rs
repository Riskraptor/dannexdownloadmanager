//! The registry read: strings and numbers, subkeys, the default browser and the installed ones, a
//! program's version, the system's proxy, and where the browsers' native host is registered.

use super::*;

/// A REG_SZ / REG_DWORD value under HKEY_CURRENT_USER\`key` (None when absent).
pub(super) fn reg_string(key: &str, name: &str) -> Option<String> {
    reg_string_in(
        windows_sys::Win32::System::Registry::HKEY_CURRENT_USER,
        key,
        name,
    )
}

/// The default browser (#668): the program https links open with (the user's choice in Default
/// apps, `UserChoice`'s ProgId, and that ProgId's `shell\open\command`), and its file's version.
/// None when it isn't a browser DDM knows (DDM then speaks as Chrome).
pub fn default_browser() -> Option<mdm_engine::agent::Found> {
    use mdm_engine::agent::{Browser, Found};
    use windows_sys::Win32::System::Registry::HKEY_CLASSES_ROOT;
    const HTTPS: &str = r"Software\Microsoft\Windows\Shell\Associations\UrlAssociations\https";
    // Windows 11 keeps the choice in UserChoiceLatest since 2025, its ProgId in a key of its
    // own (UserChoiceLatest\ProgId's ProgId); UserChoice stays, and may be an older choice.
    let prog_id = [r"UserChoiceLatest\ProgId", "UserChoiceLatest", "UserChoice"]
        .iter()
        .find_map(|k| {
            reg_string(&format!(r"{HTTPS}\{k}"), "ProgId").filter(|p| !p.trim().is_empty())
        })?;
    let command = reg_string_in(
        HKEY_CLASSES_ROOT,
        &format!(r"{prog_id}\shell\open\command"),
        "",
    )?;
    let exe = std::path::PathBuf::from(crate::defaultbrowser::windows_program(&command)?);
    let stem = exe.file_stem()?.to_string_lossy().into_owned();
    // Opera starts through its launcher.exe, in a folder named after it ("Opera", "Opera GX").
    let name = if stem.eq_ignore_ascii_case("launcher") {
        exe.parent()?.file_name()?.to_string_lossy().into_owned()
    } else {
        stem
    };
    let browser = Browser::named(&name).or_else(|| Browser::named(&prog_id))?;
    Some(Found {
        browser,
        version: file_version(&exe),
        ubuntu: false,
    })
}

/// A program's file version ("157.1.99.55"): its version resource's fixed part.
pub(super) fn file_version(exe: &std::path::Path) -> Option<String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VS_FIXEDFILEINFO, VerQueryValueW,
    };
    let path: Vec<u16> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: a NUL-terminated path; the handle argument is optional and ignored.
    let size = unsafe { GetFileVersionInfoSizeW(path.as_ptr(), std::ptr::null_mut()) };
    if size == 0 {
        return None;
    }
    let mut data = vec![0u8; size as usize];
    // SAFETY: `data` holds `size` bytes, as GetFileVersionInfoSizeW asked.
    if unsafe { GetFileVersionInfoW(path.as_ptr(), 0, size, data.as_mut_ptr().cast()) } == 0 {
        return None;
    }
    let root: Vec<u16> = "\\".encode_utf16().chain(Some(0)).collect();
    let (mut info, mut len) = (std::ptr::null_mut::<c_void>(), 0u32);
    // SAFETY: `data` is the version block just read; the pointer returned points into it.
    let ok = unsafe { VerQueryValueW(data.as_ptr().cast(), root.as_ptr(), &mut info, &mut len) };
    if ok == 0 || info.is_null() || (len as usize) < size_of::<VS_FIXEDFILEINFO>() {
        return None;
    }
    // SAFETY: VerQueryValueW returned a VS_FIXEDFILEINFO of `len` bytes inside `data`, which is
    // still alive; read unaligned, as a byte buffer promises no alignment.
    let fixed = unsafe { std::ptr::read_unaligned(info.cast::<VS_FIXEDFILEINFO>()) };
    let (ms, ls) = (fixed.dwFileVersionMS, fixed.dwFileVersionLS);
    Some(format!(
        "{}.{}.{}.{}",
        ms >> 16,
        ms & 0xffff,
        ls >> 16,
        ls & 0xffff
    ))
}

pub(super) fn reg_string_in(
    root: windows_sys::Win32::System::Registry::HKEY,
    key: &str,
    name: &str,
) -> Option<String> {
    use windows_sys::Win32::System::Registry::{RRF_RT_REG_SZ, RegGetValueW};
    let key: Vec<u16> = key.encode_utf16().chain(Some(0)).collect();
    let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    let mut buf = vec![0u16; 1024];
    let mut size = (buf.len() * 2) as u32;
    // SAFETY: registry read into `buf`, whose size in bytes is passed in `size`.
    let rc = unsafe {
        RegGetValueW(
            root,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buf.as_mut_ptr() as *mut c_void,
            &mut size,
        )
    };
    if rc != 0 {
        return None;
    }
    let len = (size as usize / 2).saturating_sub(1).min(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]))
}

pub(super) fn reg_dword(key: &str, name: &str) -> Option<u32> {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
    let key: Vec<u16> = key.encode_utf16().chain(Some(0)).collect();
    let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    let mut value: u32 = 0;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: plain registry read into a u32 of the stated size.
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            &mut value as *mut u32 as *mut c_void,
            &mut size,
        )
    };
    (rc == 0).then_some(value)
}

/// Windows' manual proxy (Settings → Network → Proxy), as (host, port), when it is on.
/// `ProxyServer` is "host:port" or "http=host:port;https=…": the http (else first) entry.
pub fn system_proxy() -> Option<(String, String)> {
    const KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings";
    if reg_dword(KEY, "ProxyEnable")? == 0 {
        return None;
    }
    let server = reg_string(KEY, "ProxyServer")?;
    let entries: Vec<&str> = server.split(';').map(str::trim).collect();
    let pick = entries
        .iter()
        .find_map(|e| e.strip_prefix("http="))
        .or_else(|| entries.iter().find(|e| !e.contains('=')).copied())
        .or_else(|| {
            entries
                .first()
                .and_then(|e| e.split_once('='))
                .map(|(_, v)| v)
        })?;
    let (host, port) = pick.rsplit_once(':')?;
    Some((host.to_owned(), port.to_owned()))
}

/// The names of the subkeys of `key` under `root`.
pub(super) fn reg_subkeys(
    root: windows_sys::Win32::System::Registry::HKEY,
    key: &str,
) -> Vec<String> {
    use windows_sys::Win32::System::Registry::{
        HKEY, KEY_READ, RegCloseKey, RegEnumKeyExW, RegOpenKeyExW,
    };
    let wide: Vec<u16> = key.encode_utf16().chain(Some(0)).collect();
    let mut h: HKEY = std::ptr::null_mut();
    // SAFETY: opens a key for reading into `h`, closed below.
    if unsafe { RegOpenKeyExW(root, wide.as_ptr(), 0, KEY_READ, &mut h) } != 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for i in 0.. {
        let mut buf = [0u16; 256];
        let mut len = buf.len() as u32;
        // SAFETY: the subkey's name into `buf`, whose length in characters is in `len`.
        let rc = unsafe {
            RegEnumKeyExW(
                h,
                i,
                buf.as_mut_ptr(),
                &mut len,
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if rc != 0 {
            break;
        }
        out.push(String::from_utf16_lossy(&buf[..len as usize]));
    }
    // SAFETY: `h` was opened above.
    unsafe { RegCloseKey(h) };
    out
}

/// The browsers installed, by name, with their program: those Windows lists for "default
/// browser" (StartMenuInternet, this user's then everyone's), named by their program
/// (mdm_ipc::browsers::windows_named: Helium's chrome.exe isn't Chrome) or else by their own
/// display name; then the known programs registered only under the shell's App Paths (a zip or
/// portable install). The browser extension's offer (extension.rs) opens its page in each.
pub fn browsers() -> Vec<(String, std::path::PathBuf)> {
    use mdm_ipc::browsers::{command_program, windows_can_run_extension, windows_named};
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    const CLIENTS: &str = r"Software\Clients\StartMenuInternet";
    const APP_PATHS: &str = r"Software\Microsoft\Windows\CurrentVersion\App Paths";
    let mut out: Vec<(String, std::path::PathBuf)> = Vec::new();
    let mut add = |name: String, path: std::path::PathBuf| {
        let same = |p: &std::path::PathBuf| {
            p.to_string_lossy()
                .eq_ignore_ascii_case(&path.to_string_lossy())
        };
        if path.is_file() && !out.iter().any(|(n, p)| same(p) || *n == name) {
            out.push((name, path));
        }
    };
    for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        for sub in reg_subkeys(root, CLIENTS) {
            let key = format!(r"{CLIENTS}\{sub}");
            let Some(program) = reg_string_in(root, &format!(r"{key}\shell\open\command"), "")
                .as_deref()
                .and_then(command_program)
            else {
                continue;
            };
            if !windows_can_run_extension(&program) {
                continue;
            }
            let name = windows_named(&program)
                .map(str::to_owned)
                .or_else(|| reg_string_in(root, &key, "").filter(|n| !n.trim().is_empty()));
            add(name.unwrap_or(sub), std::path::PathBuf::from(program));
        }
    }
    for (exe, _, _) in mdm_ipc::browsers::WINDOWS_PROGRAMS {
        let key = format!(r"{APP_PATHS}\{exe}");
        let Some(path) = reg_string_in(HKEY_CURRENT_USER, &key, "")
            .or_else(|| reg_string_in(HKEY_LOCAL_MACHINE, &key, ""))
        else {
            continue;
        };
        let path = path.trim().trim_matches('"').to_owned();
        if let Some(name) = windows_named(&path) {
            add(name.to_owned(), std::path::PathBuf::from(path));
        }
    }
    out
}

/// Where browser integration stands: a browser looking for the native host (Chrome's key,
/// which Chromium browsers read; Firefox's) finds a manifest that exists, for this user or for
/// everyone, naming a helper program that is there (Options → General → Test browser
/// integration).
pub fn host_registration() -> crate::extension::Registration {
    use crate::extension::Registration;
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    let mut broken = None;
    for key in [
        r"Software\Google\Chrome\NativeMessagingHosts\com.dannexdownloadmanager.host",
        r"Software\Mozilla\NativeMessagingHosts\com.dannexdownloadmanager.host",
    ] {
        for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
            let Some(manifest) = reg_string_in(root, key, "") else {
                continue;
            };
            let manifest = std::path::PathBuf::from(manifest.trim().trim_matches('"'));
            if !manifest.is_file() {
                continue;
            }
            match mdm_ipc::browsers::manifest_program(&manifest) {
                Some(p) if p.is_file() => return Registration::Ready,
                Some(p) => broken = broken.or(Some(p)),
                None => {}
            }
        }
    }
    broken.map_or(Registration::Missing, Registration::HelperMissing)
}
