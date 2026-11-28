//! The notification area's icon (Shell_NotifyIcon): placed, held, its tip, its notifications and
//! its picture at the taskbar's size.

use super::*;

/// The highest icon id [`tray_icon_ask`] tries. tray-icon gives each icon an id of its own
/// from a counter it shares with its icon names, and doesn't say which (a program's first icon
/// has 2).
pub(super) const TRAY_ICON_IDS: u32 = 16;

/// Longer than the shell takes to answer about an icon: a "no" that took this long is
/// Shell_NotifyIcon giving up on a busy Explorer, not the shell's answer.
pub(super) const TRAY_SLOW: std::time::Duration = std::time::Duration::from_millis(1500);

/// What the shell said of DDM's notification-area icon ([`tray_icon_ask`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayAnswer {
    /// No taskbar (no `Shell_TrayWnd` window): Explorer isn't running, or a shell without a
    /// notification area is.
    NoTaskbar,
    /// The taskbar is there but didn't answer: Windows says it stopped responding, or the
    /// question took as long as a timeout.
    Silent,
    /// The shell answered: the icon's id when it holds it (wherever it shows it), and whether it
    /// gives the icon a place on screen.
    Answered { held: Option<u32>, placed: bool },
}

/// Ask the shell about the notification-area icon of `hwnd` (the tray's own hidden window, which
/// has one icon); `known` is its id once found. Both questions wait for Explorer, for seconds when
/// it is busy (#747: 8 s), so this runs on the tray's watcher thread (tray.rs), never on the
/// window's.
///
/// - Held: an empty `NIM_MODIFY` changes nothing and succeeds only for an icon the shell has, in
///   the taskbar or among the hidden icons behind ^ (`known` first, else each id up to
///   [`TRAY_ICON_IDS`]).
/// - Placed: `Shell_NotifyIconGetRect`. Not enough on its own: Windows 10 and Windows 11 builds
///   before 26200 give an icon behind ^ no place (#750, #718, #754: 0.5.2 asked only this, and
///   left the window in the taskbar with the icon there); build 26200 gives it the place of ^.
///
/// `rect_withheld` answers "no place" without asking (MDM_TEST_TRAY_RECT=0: the reporters' case,
/// on a Windows that gives one).
pub fn tray_icon_ask(hwnd: isize, known: Option<u32>, rect_withheld: bool) -> TrayAnswer {
    use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, IsHungAppWindow};
    let hwnd = hwnd as HWND;
    if hwnd.is_null() {
        return TrayAnswer::Answered {
            held: None,
            placed: false,
        };
    }
    let class: Vec<u16> = "Shell_TrayWnd\0".encode_utf16().collect();
    // SAFETY: a NUL-ended class name and no title: no window is sent a message.
    let taskbar = unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) };
    if taskbar.is_null() {
        return TrayAnswer::NoTaskbar;
    }
    // SAFETY: reads the window's state; never waits for it.
    if unsafe { IsHungAppWindow(taskbar) } != 0 {
        return TrayAnswer::Silent;
    }
    let start = std::time::Instant::now();
    let ids = || {
        known
            .into_iter()
            .chain((1..=TRAY_ICON_IDS).filter(move |id| Some(*id) != known))
    };
    let held = ids().find(|id| tray_icon_held(hwnd, *id));
    // An icon it doesn't hold is asked for its place too, under each id: should an empty
    // NIM_MODIFY fail on some Windows for an icon it has, its place still says it is there.
    let placed = !rect_withheld
        && match held {
            Some(id) => tray_icon_placed(hwnd, id),
            None => ids().any(|id| tray_icon_placed(hwnd, id)),
        };
    if held.is_none() && !placed && start.elapsed() >= TRAY_SLOW {
        return TrayAnswer::Silent;
    }
    TrayAnswer::Answered { held, placed }
}

/// Whether the shell holds icon `id` of `hwnd`: an empty `NIM_MODIFY`.
pub(super) fn tray_icon_held(hwnd: HWND, id: u32) -> bool {
    use windows_sys::Win32::UI::Shell::{NIM_MODIFY, NOTIFYICONDATAW, Shell_NotifyIconW};
    let nid = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: id,
        uFlags: 0,
        // SAFETY: a plain C struct; all zero is its empty value.
        ..unsafe { zeroed() }
    };
    // SAFETY: nid lives for the call; the shell only reads it.
    unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) != 0 }
}

/// Whether the shell gives icon `id` of `hwnd` a place on screen.
pub(super) fn tray_icon_placed(hwnd: HWND, id: u32) -> bool {
    use windows_sys::Win32::UI::Shell::{NOTIFYICONIDENTIFIER, Shell_NotifyIconGetRect};
    let ident = NOTIFYICONIDENTIFIER {
        cbSize: size_of::<NOTIFYICONIDENTIFIER>() as u32,
        hWnd: hwnd,
        uID: id,
        // SAFETY: a plain C struct; with a zero GUID the icon is named by its window and id.
        ..unsafe { zeroed() }
    };
    // SAFETY: a plain C struct, written by the call.
    let mut rect: RECT = unsafe { zeroed() };
    // SAFETY: both live for the call.
    unsafe { Shell_NotifyIconGetRect(&ident, &mut rect) >= 0 }
}

/// Set the tip of icon `id` of `hwnd` (127 characters at most, the rest cut). Waits for Explorer
/// as [`tray_icon_ask`] does, so it too runs on the tray's watcher thread. False when the shell
/// didn't take it (no such icon, or no answer).
pub fn tray_icon_tip(hwnd: isize, id: u32, tip: &str) -> bool {
    use windows_sys::Win32::UI::Shell::{NIF_TIP, NIM_MODIFY, NOTIFYICONDATAW, Shell_NotifyIconW};
    let mut nid = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd as HWND,
        uID: id,
        uFlags: NIF_TIP,
        // SAFETY: a plain C struct; all zero is its empty value.
        ..unsafe { zeroed() }
    };
    // The last unit stays the NUL that ends it.
    let room = nid.szTip.len() - 1;
    for (slot, unit) in nid.szTip.iter_mut().zip(tip.encode_utf16().take(room)) {
        *slot = unit;
    }
    // SAFETY: nid lives for the call; the shell only reads it.
    unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) != 0 }
}

/// Show a notification by icon `id` of `hwnd`: `title` (63 characters at most) and `body` (255),
/// the rest cut. Quiet: no sound, and not while the user's focus assist or Do not disturb is on
/// (Windows then keeps it in its notification centre or drops it). Windows 10 and 11 show it as
/// a notification of the app's. Waits for Explorer as [`tray_icon_tip`] does, so it runs on the
/// tray's watcher thread. False when the shell didn't take it.
pub fn tray_icon_notify(hwnd: isize, id: u32, title: &str, body: &str) -> bool {
    use windows_sys::Win32::UI::Shell::{
        NIF_INFO, NIIF_NOSOUND, NIIF_RESPECT_QUIET_TIME, NIIF_USER, NIM_MODIFY, NOTIFYICONDATAW,
        Shell_NotifyIconW,
    };
    let mut nid = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd as HWND,
        uID: id,
        uFlags: NIF_INFO,
        // The icon's own picture beside the text (NIIF_USER with no balloon icon: the tray's).
        dwInfoFlags: NIIF_USER | NIIF_NOSOUND | NIIF_RESPECT_QUIET_TIME,
        // SAFETY: a plain C struct; all zero is its empty value.
        ..unsafe { zeroed() }
    };
    // The last unit of each stays the NUL that ends it.
    let room = nid.szInfoTitle.len() - 1;
    for (slot, unit) in nid
        .szInfoTitle
        .iter_mut()
        .zip(title.encode_utf16().take(room))
    {
        *slot = unit;
    }
    let room = nid.szInfo.len() - 1;
    for (slot, unit) in nid.szInfo.iter_mut().zip(body.encode_utf16().take(room)) {
        *slot = unit;
    }
    // SAFETY: nid lives for the call; the shell only reads it.
    unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) != 0 }
}

/// The notification area's icon size at the taskbar's scale (16 pixels at 100 %, 24 at 150 %),
/// for a picture drawn at that size rather than one the shell scales down.
pub fn tray_icon_px() -> u32 {
    use windows_sys::Win32::UI::HiDpi::{GetDpiForSystem, GetDpiForWindow, GetSystemMetricsForDpi};
    use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, SM_CXSMICON};
    let class: Vec<u16> = "Shell_TrayWnd\0".encode_utf16().collect();
    // SAFETY: a NUL-ended class name and no title: no window is sent a message.
    let taskbar = unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) };
    // SAFETY: plain queries; a gone window gives 0.
    let dpi = if taskbar.is_null() {
        0
    } else {
        unsafe { GetDpiForWindow(taskbar) }
    };
    let dpi = if dpi == 0 {
        unsafe { GetDpiForSystem() }
    } else {
        dpi
    };
    // SAFETY: as above.
    let px = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, dpi.max(96)) };
    px.clamp(16, 256) as u32
}

/// Set the picture of icon `id` of `hwnd` (`side` pixels square, unpremultiplied RGBA). Waits for
/// Explorer as [`tray_icon_tip`] does, so it too runs on the tray's watcher thread. False when the
/// shell didn't take it. The picture it had is kept until the next is in place.
pub fn tray_icon_picture(hwnd: isize, id: u32, rgba: &[u8], side: u32) -> bool {
    use windows_sys::Win32::UI::Shell::{NIF_ICON, NIM_MODIFY, NOTIFYICONDATAW, Shell_NotifyIconW};
    /// The icon the shell took last (0: none of ours yet): tray-icon's own first one stays its.
    static LAST: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
    let Some(icon) = icon_from_rgba(rgba, side) else {
        return false;
    };
    let nid = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd as HWND,
        uID: id,
        uFlags: NIF_ICON,
        hIcon: icon,
        // SAFETY: a plain C struct; all zero is its empty value.
        ..unsafe { zeroed() }
    };
    // SAFETY: nid lives for the call; the shell only reads it.
    let taken = unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) != 0 };
    let gone = if taken {
        LAST.swap(icon as isize, std::sync::atomic::Ordering::Relaxed)
    } else {
        icon as isize
    };
    if gone != 0 {
        // SAFETY: an icon made here, no longer the shell's.
        unsafe { DestroyIcon(gone as _) };
    }
    taken
}
