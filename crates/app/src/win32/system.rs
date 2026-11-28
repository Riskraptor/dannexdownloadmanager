//! What Windows says of itself and its user: the apps' and the taskbar's modes, the accent and high
//! contrast colours, the wheel, idle time, an assistive technology, the region's formats, the
//! installer's language, the version, the network route, and sleep.

use super::*;

/// The address a connection to `dst` would leave from (PAC's `myIpAddress`), asked of the
/// routing table (GetBestRoute2). No socket is opened: a UDP socket, the usual way to ask, is a
/// listening socket to Windows' firewall, which would ask the user about DDM. Installed as the
/// engine's lookup at start (`mdm_engine::net::set_source_lookup`); unix.rs has none (the
/// engine's UDP way raises no prompt there).
pub fn source_address(dst: std::net::IpAddr) -> Option<std::net::IpAddr> {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
    use windows_sys::Win32::NetworkManagement::IpHelper::{GetBestRoute2, MIB_IPFORWARD_ROW2};
    use windows_sys::Win32::Networking::WinSock::{AF_INET, AF_INET6, SOCKADDR_INET};
    // SAFETY: plain-data structures, zero is a valid value for each; the union is written and
    // read by its family; GetBestRoute2 only reads `dest` and writes `row` and `src`.
    unsafe {
        let mut dest: SOCKADDR_INET = zeroed();
        match dst {
            IpAddr::V4(a) => {
                dest.Ipv4.sin_family = AF_INET;
                dest.Ipv4.sin_addr.S_un.S_addr = u32::from_ne_bytes(a.octets());
            }
            IpAddr::V6(a) => {
                dest.Ipv6.sin6_family = AF_INET6;
                dest.Ipv6.sin6_addr.u.Byte = a.octets();
            }
        }
        let mut row: MIB_IPFORWARD_ROW2 = zeroed();
        let mut src: SOCKADDR_INET = zeroed();
        let r = GetBestRoute2(
            std::ptr::null(),
            0,
            std::ptr::null(),
            &dest,
            0,
            &mut row,
            &mut src,
        );
        if r != 0 {
            return None;
        }
        match src.si_family {
            AF_INET => Some(IpAddr::V4(Ipv4Addr::from(
                src.Ipv4.sin_addr.S_un.S_addr.to_ne_bytes(),
            ))),
            AF_INET6 => Some(IpAddr::V6(Ipv6Addr::from(src.Ipv6.sin6_addr.u.Byte))),
            _ => None,
        }
    }
}

/// Windows' "Choose your app mode": true when apps should be dark (`AppsUseLightTheme` = 0).
/// Missing value (older Windows): light.
pub fn apps_use_dark() -> bool {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
    let key: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\0"
        .encode_utf16()
        .collect();
    let name: Vec<u16> = "AppsUseLightTheme\0".encode_utf16().collect();
    let mut value: u32 = 1;
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
    rc == 0 && value == 0
}

/// Windows' "Choose your default Windows mode": true when the taskbar (and the notification area
/// with it) is light (`SystemUsesLightTheme` = 1), which can differ from the apps' mode. Missing
/// value (Windows 10 before 1903, whose taskbar is always dark): dark.
pub fn taskbar_light() -> bool {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
    let key: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\0"
        .encode_utf16()
        .collect();
    let name: Vec<u16> = "SystemUsesLightTheme\0".encode_utf16().collect();
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
    rc == 0 && value == 1
}

/// Windows' accent colour and its shades, from the personalisation settings (`AccentPalette`:
/// 8 colours of 4 bytes, R G B and an unused byte, lightest first: Light3, Light2, Light1, the
/// accent, Dark1, Dark2, Dark3, and one more). None when Windows has none (older versions).
pub fn accent_palette() -> Option<[[u8; 3]; 7]> {
    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, RRF_RT_REG_BINARY, RegGetValueW,
    };
    let key: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Accent\0"
        .encode_utf16()
        .collect();
    let name: Vec<u16> = "AccentPalette\0".encode_utf16().collect();
    let mut bytes = [0u8; 32];
    let mut size = bytes.len() as u32;
    // SAFETY: a registry read into a 32-byte buffer of the stated size.
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_BINARY,
            std::ptr::null_mut(),
            bytes.as_mut_ptr() as *mut c_void,
            &mut size,
        )
    };
    (rc == 0 && size >= 28).then(|| crate::native::accent_palette_bytes(&bytes))
}

/// Windows' high-contrast colours while high contrast is on (Settings → Accessibility →
/// Contrast themes): background, text, selection, selection's text, links, greyed text. None
/// while it is off.
pub fn high_contrast_colors() -> Option<[egui::Color32; 6]> {
    use windows_sys::Win32::Graphics::Gdi::{
        COLOR_GRAYTEXT, COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT, COLOR_HOTLIGHT, COLOR_WINDOW,
        COLOR_WINDOWTEXT, GetSysColor,
    };
    use windows_sys::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
    use windows_sys::Win32::UI::WindowsAndMessaging::{SPI_GETHIGHCONTRAST, SystemParametersInfoW};
    // SAFETY: HIGHCONTRASTW is plain data; its size is set as the call requires.
    let mut hc: HIGHCONTRASTW = unsafe { zeroed() };
    hc.cbSize = size_of::<HIGHCONTRASTW>() as u32;
    // SAFETY: a read into the struct above, of the size it states.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            hc.cbSize,
            &mut hc as *mut HIGHCONTRASTW as *mut c_void,
            0,
        )
    };
    if ok == 0 || hc.dwFlags & HCF_HIGHCONTRASTON == 0 {
        return None;
    }
    let color = |i| {
        // SAFETY: a plain call; COLORREF is 0x00BBGGRR.
        let c = unsafe { GetSysColor(i) };
        egui::Color32::from_rgb(c as u8, (c >> 8) as u8, (c >> 16) as u8)
    };
    Some([
        color(COLOR_WINDOW),
        color(COLOR_WINDOWTEXT),
        color(COLOR_HIGHLIGHT),
        color(COLOR_HIGHLIGHTTEXT),
        color(COLOR_HOTLIGHT),
        color(COLOR_GRAYTEXT),
    ])
}

/// Sleep now (not hibernate: rundll32's SetSuspendState read its own window handle as
/// "hibernate"). `force`: don't let programs delay it. False when Windows refused.
pub fn sleep(force: bool) -> bool {
    use windows_sys::Win32::System::Power::SetSuspendState;
    // SAFETY: a plain call; FALSE / force / FALSE (no wake events disabled).
    unsafe { SetSuspendState(false, force, false) }
}

/// Whether an assistive technology (a screen reader, a UI Automation client) is listening to
/// this desktop's windows now (#131): the windows then keep their accessibility tree ready, so
/// a window read for the first time answers with its contents at once.
pub fn assistive_technology_listening() -> bool {
    // SAFETY: a plain query, no arguments.
    unsafe { windows_sys::Win32::UI::Accessibility::UiaClientsAreListening() != 0 }
}

/// Rows a notch of the mouse wheel scrolls (Mouse settings → "lines to scroll at a time", 3
/// unless changed); None when it is set to a screen at a time.
pub fn wheel_lines() -> Option<f32> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SPI_GETWHEELSCROLLLINES, SystemParametersInfoW,
    };
    let mut lines: u32 = 3;
    // SAFETY: a plain query into a u32 it writes.
    let ok =
        unsafe { SystemParametersInfoW(SPI_GETWHEELSCROLLLINES, 0, (&raw mut lines).cast(), 0) };
    match lines {
        _ if ok == 0 => Some(3.0),
        // WHEEL_PAGESCROLL
        u32::MAX => None,
        n => Some(n as f32),
    }
}

/// Seconds since the last keyboard or mouse input anywhere ("limit while I'm using the
/// computer").
pub fn idle_seconds() -> u64 {
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    let mut info = LASTINPUTINFO {
        cbSize: size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    // SAFETY: `info` is a LASTINPUTINFO with its size set, as the call requires.
    let (ok, now) = unsafe { (GetLastInputInfo(&mut info), GetTickCount()) };
    if ok == 0 {
        return 0;
    }
    u64::from(now.wrapping_sub(info.dwTime) / 1000)
}

/// The language the installer's wizard ran in (tools/installer/mdm.iss writes it on every
/// install), taken once: read, then deleted. A languages.tsv code (`pt-br`); Inno's language
/// names have `_` for `-`.
pub fn take_install_language() -> Option<String> {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RegDeleteKeyValueW};
    const KEY: &str = r"Software\DannexDownloadManager";
    const NAME: &str = "InstallLanguage";
    let value = reg_string(KEY, NAME)?;
    let w = |s: &str| -> Vec<u16> { s.encode_utf16().chain(Some(0)).collect() };
    let (key, name) = (w(KEY), w(NAME));
    // SAFETY: NUL-terminated key and value names.
    unsafe {
        RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr());
    }
    Some(install_language_code(&value))
}

/// An Inno language name (`lang_pt_br`, tools/installer/languages.py) as a languages.tsv code.
pub(super) fn install_language_code(inno: &str) -> String {
    inno.strip_prefix("lang_").unwrap_or(inno).replace('_', "-")
}

/// The country of the user's Windows region format (Settings → Time & language → Region), as
/// ISO 3166-1 alpha-2 (`KE`); None if Windows can't say (`regional::country`).
pub fn user_country() -> Option<String> {
    use windows_sys::Win32::Globalization::{GetLocaleInfoEx, LOCALE_SISO3166CTRYNAME};
    let mut buf = [0u16; 10];
    // SAFETY: null locale = the user's; the buffer's length is passed in characters.
    let n = unsafe {
        GetLocaleInfoEx(
            std::ptr::null(),
            LOCALE_SISO3166CTRYNAME,
            buf.as_mut_ptr(),
            buf.len() as i32,
        )
    };
    let code = (n > 1).then(|| String::from_utf16_lossy(&buf[..n as usize - 1]))?;
    (code.len() == 2 && code.chars().all(|c| c.is_ascii_alphabetic()))
        .then(|| code.to_ascii_uppercase())
}

/// The user's Windows region (Control Panel → Region): its short date's order, whether its
/// clock is 24-hour, and the first day of the week (0 = Sunday). None if Windows can't say.
pub fn region_formats() -> Option<(crate::regional::DateOrder, bool, u8)> {
    use windows_sys::Win32::Globalization::{
        GetLocaleInfoEx, LOCALE_IFIRSTDAYOFWEEK, LOCALE_SSHORTDATE, LOCALE_STIMEFORMAT,
    };
    let info = |what: u32| -> Option<String> {
        let mut buf = [0u16; 80];
        // SAFETY: null locale = the user's; the buffer's length is passed in characters.
        let n =
            unsafe { GetLocaleInfoEx(std::ptr::null(), what, buf.as_mut_ptr(), buf.len() as i32) };
        (n > 1).then(|| String::from_utf16_lossy(&buf[..n as usize - 1]))
    };
    let date = info(LOCALE_SSHORTDATE)?;
    let at = |c: char| date.find(c).unwrap_or(usize::MAX);
    let (d, m, y) = (at('d'), at('M'), at('y'));
    let order = if y < m && y < d {
        crate::regional::DateOrder::Ymd
    } else if m < d {
        crate::regional::DateOrder::Mdy
    } else {
        crate::regional::DateOrder::Dmy
    };
    // "HH:mm:ss" (24-hour) or "h:mm:ss tt".
    let clock24 = info(LOCALE_STIMEFORMAT).is_some_and(|t| t.contains('H'));
    // Windows counts from Monday (0) to Sunday (6).
    let first = info(LOCALE_IFIRSTDAYOFWEEK)
        .and_then(|s| s.trim().parse::<u8>().ok())
        .map_or(0, |n| (n + 1) % 7);
    Some((order, clock24, first))
}

/// The user's Windows region's own pictures (Control Panel → Region, customised there or not):
/// its short date, long date and time with seconds, in Windows' letters (`dd.MM.yyyy`,
/// `dddd, d. MMMM yyyy`, `HH:mm:ss`). None if Windows can't say.
pub fn region_pictures() -> Option<[String; 3]> {
    use windows_sys::Win32::Globalization::{
        GetLocaleInfoEx, LOCALE_SLONGDATE, LOCALE_SSHORTDATE, LOCALE_STIMEFORMAT,
    };
    let info = |what: u32| -> Option<String> {
        let mut buf = [0u16; 120];
        // SAFETY: null locale = the user's; the buffer's length is passed in characters.
        let n =
            unsafe { GetLocaleInfoEx(std::ptr::null(), what, buf.as_mut_ptr(), buf.len() as i32) };
        (n > 1).then(|| String::from_utf16_lossy(&buf[..n as usize - 1]))
    };
    Some([
        info(LOCALE_SSHORTDATE)?,
        info(LOCALE_SLONGDATE).unwrap_or_default(),
        info(LOCALE_STIMEFORMAT).unwrap_or_default(),
    ])
}

/// Windows' edition, release and build, e.g. "Windows 11 Pro 24H2 (build 26200)" — for problem
/// reports. ProductName still says "Windows 10" on 11; the build tells them apart.
pub fn windows_version() -> String {
    use windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE;
    const KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
    let get = |n| reg_string_in(HKEY_LOCAL_MACHINE, KEY, n);
    let build = get("CurrentBuild").unwrap_or_default();
    let mut product = get("ProductName").unwrap_or_else(|| "Windows".into());
    if build.parse::<u32>().is_ok_and(|b| b >= 22000) {
        product = product.replace("Windows 10", "Windows 11");
    }
    let release = get("DisplayVersion")
        .map(|v| format!(" {v}"))
        .unwrap_or_default();
    format!("{product}{release} (build {build})")
}

#[cfg(test)]
mod install_language_tests {
    #[test]
    fn inno_names_become_codes() {
        assert_eq!(super::install_language_code("lang_pt_br"), "pt-br");
        assert_eq!(super::install_language_code("lang_or"), "or");
        assert_eq!(
            super::install_language_code("lang_ca_es_valencia"),
            "ca-es-valencia"
        );
    }
}
