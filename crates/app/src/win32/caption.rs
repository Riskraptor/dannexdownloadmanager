//! Title bars the theme draws (`Drawn`): a window without the system's caption, its colour, its
//! window menu and maximizing to the work area; the system's caption and menus made dark.

use super::*;

/// How a window whose title bar is ours is framed without a caption style ([`caption_less`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Drawn {
    /// A fixed size (the progress window, a dialog with a style theme's title bar): a plain
    /// popup, all client, as the drop target and the menus are (#210, #231).
    Fixed,
    /// Resizable (the main window with a style theme): a popup with the system's sizing frame,
    /// for resizing on every edge, Aero Snap and Snap Layouts, the shadow, and maximizing.
    Sizable,
}

/// The subclass of a window framed by [`caption_less`]: its id, and its data for each kind.
pub(super) const DRAWN_SUBCLASS: usize = 0x1d8;

impl Drawn {
    pub(super) fn data(self) -> usize {
        match self {
            Drawn::Fixed => 0,
            Drawn::Sizable => 1,
        }
    }
}

thread_local! {
    /// The colour last given to each resizable window's frame above its drawn title bar.
    pub(super) static CAPTION_COLOUR: std::cell::RefCell<std::collections::HashMap<isize, u32>> =
        Default::default();
}

thread_local! {
    /// The caption each of our windows was last given by [`caption_dark`] (dark: true).
    pub(super) static CAPTION_DARK: std::cell::RefCell<std::collections::HashMap<isize, bool>> =
        Default::default();
}

/// How many times [`caption_dark`] has asked DWM, for the tests.
#[cfg(test)]
pub(super) static CAPTION_DWM_CALLS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// uxtheme's functions that set the mode of the menus Windows draws ([`menus_dark`]).
#[derive(Clone, Copy)]
pub(super) struct MenuModeFns {
    pub(super) set_preferred_app_mode: unsafe extern "system" fn(i32) -> i32,
    pub(super) flush_menu_themes: unsafe extern "system" fn(),
}

/// Whether the windows whose title bar is ours are framed without a caption style
/// ([`caption_less`], #208): on Windows 11 (build 22000 on), which rounds such a window when asked
/// and draws its border. Windows 10 does neither for a window without a caption style, so it
/// keeps the frames above (the caption style, its caption given to the client): the band they
/// showed was seen on a Windows 11 build only.
pub(super) fn caption_less_frames() -> bool {
    use windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE;
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        reg_string_in(
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
            "CurrentBuild",
        )
        .and_then(|b| b.parse::<u32>().ok())
        .is_some_and(|b| b >= 22000)
    })
}

/// The style of a window whose title bar is ours, framed without a caption style (#208): a
/// popup with no caption, border or dialog frame, with the system's sizing frame when it is
/// resizable; its system menu and its Minimize and Maximize kept (the taskbar's and Alt+Space's
/// commands). With a caption style hidden by our WM_NCCALCSIZE, a Windows 11 Insider build
/// (22635) placed the window's picture where the style's client would have been: a black band a
/// title bar tall across the top of the progress window, as it did with the menus (#210) and the
/// drop target (#231). Here what the styles describe is what the window is: the system's own
/// WM_NCCALCSIZE frames it, no other reading can differ.
pub(crate) fn drawn_style(style: u32, kind: Drawn) -> u32 {
    use windows_sys::Win32::UI::WindowsAndMessaging::{WS_CAPTION, WS_POPUP};
    let plain = (style & !(WS_CAPTION | WS_THICKFRAME)) | WS_POPUP;
    match kind {
        Drawn::Fixed => plain,
        Drawn::Sizable => plain | WS_THICKFRAME,
    }
}

/// Its extended styles: without the edges that add to a frame (IDM's dialog frame among them).
/// The rest stays: a taskbar button where it had one, never-activate in an off-screen test copy.
pub(crate) fn drawn_ex_style(ex: u32) -> u32 {
    use windows_sys::Win32::UI::WindowsAndMessaging::{WS_EX_CLIENTEDGE, WS_EX_STATICEDGE};
    ex & !(WS_EX_DLGMODALFRAME | WS_EX_CLIENTEDGE | WS_EX_STATICEDGE)
}

/// Frame our window `hwnd`, whose title bar is ours, without a caption style ([`drawn_style`]).
/// Its subclass keeps the styles when winit sets its own again (on every change of its window
/// flags) and hit-tests the drawn title bar. Again on the same window: the styles applied again.
///
/// # Safety
/// `hwnd` is a window of this thread.
pub(super) unsafe fn caption_less(hwnd: HWND, kind: Drawn) {
    use windows_sys::Win32::Graphics::Dwm::{
        DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOSIZE;
    // SAFETY: (the caller's) our own window; the proc lives as long as the program; the DWM
    // attribute is read from a local during the call.
    unsafe {
        // The subclass first: it keeps the styles below when winit sets its own again.
        SetWindowSubclass(hwnd, Some(caption_less_proc), DRAWN_SUBCLASS, kind.data());
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, drawn_ex_style(ex) as isize);
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        SetWindowLongPtrW(hwnd, GWL_STYLE, drawn_style(style, kind) as isize);
        // Windows 11 rounds a window with a caption style by itself, not a popup: asked for.
        let corner = DWMWCP_ROUND;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            &corner as *const _ as *const c_void,
            size_of::<i32>() as u32,
        );
        // The frame measured again (WM_NCCALCSIZE), by the system.
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

/// The system's title bar back on a window framed by [`caption_less`] (the main window, back to
/// a theme without a title bar of its own).
///
/// # Safety
/// `hwnd` is a window of this thread.
pub(super) unsafe fn caption_back(hwnd: HWND) {
    use windows_sys::Win32::Graphics::Dwm::{
        DWMWA_CAPTION_COLOR, DWMWA_COLOR_DEFAULT, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DEFAULT,
        DwmSetWindowAttribute,
    };
    use windows_sys::Win32::UI::Shell::RemoveWindowSubclass;
    use windows_sys::Win32::UI::WindowsAndMessaging::{SWP_NOSIZE, WS_CAPTION, WS_POPUP};
    // SAFETY: (the caller's) our own window; DWM attributes read from locals during the calls.
    unsafe {
        RemoveWindowSubclass(hwnd, Some(caption_less_proc), DRAWN_SUBCLASS);
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        SetWindowLongPtrW(hwnd, GWL_STYLE, ((style & !WS_POPUP) | WS_CAPTION) as isize);
        let corner = DWMWCP_DEFAULT;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            &corner as *const _ as *const c_void,
            size_of::<i32>() as u32,
        );
        let colour = DWMWA_COLOR_DEFAULT;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_CAPTION_COLOR as u32,
            &colour as *const u32 as *const c_void,
            size_of::<u32>() as u32,
        );
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
    CAPTION_COLOUR.with(|m| {
        if let Ok(mut m) = m.try_borrow_mut() {
            m.remove(&(hwnd as isize));
        }
    });
}

/// The colour of the sizing frame's strip above the main window's drawn title bar (`rgb`, the
/// top of the bar), so the two read as one bar (Windows 11 paints the frame of a window without
/// a caption style in its caption colour). Cheap: DWM is asked only when it changes.
pub fn caption_colour(hwnd: isize, rgb: [u8; 3]) {
    use windows_sys::Win32::Graphics::Dwm::{DWMWA_CAPTION_COLOR, DwmSetWindowAttribute};
    if !caption_less_frames() {
        return;
    }
    let colour = u32::from_le_bytes([rgb[0], rgb[1], rgb[2], 0]);
    let changed = CAPTION_COLOUR.with(|m| {
        m.try_borrow_mut()
            .is_ok_and(|mut m| m.insert(hwnd, colour) != Some(colour))
    });
    if changed {
        // SAFETY: our own window; the colour is read from a local during the call.
        unsafe {
            DwmSetWindowAttribute(
                hwnd as HWND,
                DWMWA_CAPTION_COLOR as u32,
                &colour as *const u32 as *const c_void,
                size_of::<u32>() as u32,
            );
        }
    }
}

/// The window menu (the system menu) of a window framed by [`caption_less`], opened at screen
/// point `(x, y)` as the system opens it from a caption: right-click on the title bar, a click on
/// its icon. Its items are enabled as the window's state allows; the command chosen is carried
/// out as the system's (WM_SYSCOMMAND). Alt+Space opens it through the system already.
pub(super) fn window_menu(hwnd: HWND, x: i32, y: i32, sizable: bool) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnableMenuItem, GetSystemMenu, IsIconic, IsZoomed, MF_BYCOMMAND, MF_ENABLED, MF_GRAYED,
        PostMessageW, SC_CLOSE, SC_MAXIMIZE, SC_MINIMIZE, SC_MOVE, SC_RESTORE, SC_SIZE,
        SetMenuDefaultItem, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, WM_SYSCOMMAND,
        WS_MAXIMIZEBOX, WS_MINIMIZEBOX,
    };
    // SAFETY: our own window and its system menu, on its thread.
    unsafe {
        let menu = GetSystemMenu(hwnd, 0);
        if menu.is_null() {
            return;
        }
        let (zoomed, iconic) = (IsZoomed(hwnd) != 0, IsIconic(hwnd) != 0);
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        for (cmd, on) in [
            (SC_RESTORE, zoomed || iconic),
            (SC_MOVE, !zoomed),
            (SC_SIZE, sizable && !zoomed && !iconic),
            (SC_MINIMIZE, style & WS_MINIMIZEBOX != 0 && !iconic),
            (
                SC_MAXIMIZE,
                sizable && style & WS_MAXIMIZEBOX != 0 && !zoomed,
            ),
            (SC_CLOSE, true),
        ] {
            let flag = if on { MF_ENABLED } else { MF_GRAYED };
            EnableMenuItem(menu, cmd, MF_BYCOMMAND | flag);
        }
        SetMenuDefaultItem(menu, SC_CLOSE, 0);
        let cmd = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            x,
            y,
            0,
            hwnd,
            std::ptr::null(),
        );
        if cmd != 0 {
            PostMessageW(hwnd, WM_SYSCOMMAND, cmd as WPARAM, 0);
        }
    }
}

/// A window framed by [`caption_less`]: it keeps its styles, the system frames it
/// (WM_NCCALCSIZE as its styles say), and its drawn title bar is hit-tested as a caption: the
/// buttons are the client's (egui), the icon the system menu's, the rest the caption (moved,
/// snapped, shaken, double-clicked by the system). A resizable one's Maximize answers as the
/// system's (Snap Layouts) and it maximizes to the work area, as a window with a caption does.
pub(super) unsafe extern "system" fn caption_less_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    data: usize,
) -> LRESULT {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT, TrackMouseEvent,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, HTMAXBUTTON, IsZoomed, PostMessageW, SC_CLOSE, SC_MAXIMIZE, SC_RESTORE,
        STYLESTRUCT, SendMessageW, WM_GETMINMAXINFO, WM_NCACTIVATE, WM_NCLBUTTONDBLCLK,
        WM_NCLBUTTONDOWN, WM_NCLBUTTONUP, WM_NCMOUSELEAVE, WM_NCMOUSEMOVE, WM_NCRBUTTONUP,
        WM_STYLECHANGING, WM_SYSCOMMAND,
    };
    const HT_MAX: LRESULT = HTMAXBUTTON as LRESULT;
    let sizable = data == Drawn::Sizable.data();
    let kind = if sizable {
        Drawn::Sizable
    } else {
        Drawn::Fixed
    };
    let at = |lp: LPARAM| {
        (
            (lp & 0xffff) as i16 as i32,
            ((lp >> 16) & 0xffff) as i16 as i32,
        )
    };
    match msg {
        WM_STYLECHANGING if lparam != 0 => {
            // SAFETY: for WM_STYLECHANGING, lParam points to the STYLESTRUCT of the change,
            // whose styleNew the window may alter (Win32 contract).
            let s = unsafe { &mut *(lparam as *mut STYLESTRUCT) };
            match wparam as i32 {
                GWL_STYLE => s.styleNew = drawn_style(s.styleNew, kind),
                GWL_EXSTYLE => s.styleNew = drawn_ex_style(s.styleNew),
                _ => {}
            }
            return 0;
        }
        // The system's frame for these styles, nothing of ours or winit's: what any reading of
        // the styles finds.
        WM_NCCALCSIZE => return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        // Activation changes repaint the non-client area: with lParam -1 the system skips that.
        WM_NCACTIVATE => return unsafe { DefSubclassProc(hwnd, msg, wparam, -1) },
        WM_SYSCOMMAND if (wparam & 0xFFF0) == SC_HIDE_TO_TRAY => {
            HIDE_ASKED.with(|s| {
                if let Ok(mut s) = s.try_borrow_mut() {
                    s.insert(hwnd as isize);
                }
            });
            return 0;
        }
        // The window menu from the title bar, which the system opens only from a caption.
        WM_NCRBUTTONUP if wparam as LRESULT == HTCAPTION || wparam as LRESULT == HTSYSMENU => {
            let (x, y) = at(lparam);
            window_menu(hwnd, x, y, sizable);
            return 0;
        }
        WM_NCLBUTTONDOWN if wparam as LRESULT == HTSYSMENU => {
            // Under the icon, as the system opens it from its own.
            let hits = CAPTION_HITS.with(|m| {
                m.try_borrow()
                    .ok()
                    .and_then(|m| m.get(&(hwnd as isize)).map(|h| h.icon))
            });
            let mut pt = match hits {
                Some(icon) => POINT {
                    x: icon[0],
                    y: icon[3],
                },
                None => POINT { x: 0, y: 0 },
            };
            // SAFETY: our own window; the point is a local.
            unsafe { windows_sys::Win32::Graphics::Gdi::ClientToScreen(hwnd, &mut pt) };
            window_menu(hwnd, pt.x, pt.y, sizable);
            return 0;
        }
        WM_NCLBUTTONDBLCLK if wparam as LRESULT == HTSYSMENU => {
            // SAFETY: our own window, a system command it handles.
            unsafe { PostMessageW(hwnd, WM_SYSCOMMAND, SC_CLOSE as WPARAM, 0) };
            return 0;
        }
        // Maximize: pressed and released on it toggles the window (the system's own button would
        // draw itself over ours).
        WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK if sizable && wparam as LRESULT == HT_MAX => {
            set_max_button(
                hwnd,
                MaxButton {
                    hot: true,
                    pressed: true,
                },
            );
            return 0;
        }
        WM_NCLBUTTONUP if sizable && wparam as LRESULT == HT_MAX => {
            let was = max_button_state(hwnd as isize);
            set_max_button(
                hwnd,
                MaxButton {
                    hot: true,
                    pressed: false,
                },
            );
            if was.pressed {
                // SAFETY: our own window, a system command it handles.
                let zoomed = unsafe { IsZoomed(hwnd) } != 0;
                let cmd = if zoomed { SC_RESTORE } else { SC_MAXIMIZE };
                unsafe { SendMessageW(hwnd, WM_SYSCOMMAND, cmd as WPARAM, 0) };
            }
            return 0;
        }
        WM_NCMOUSEMOVE if sizable => {
            let on = wparam as LRESULT == HT_MAX;
            let was = max_button_state(hwnd as isize);
            set_max_button(
                hwnd,
                MaxButton {
                    hot: on,
                    pressed: was.pressed && on,
                },
            );
            if on {
                let mut t = TRACKMOUSEEVENT {
                    cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE | TME_NONCLIENT,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                // SAFETY: a plain call with a filled struct, our own window.
                unsafe { TrackMouseEvent(&mut t) };
            }
        }
        WM_NCMOUSELEAVE if sizable => set_max_button(hwnd, MaxButton::default()),
        WM_GETMINMAXINFO if sizable && lparam != 0 => {
            let res = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
            // SAFETY: for WM_GETMINMAXINFO, lParam points to the MINMAXINFO to fill.
            let info = unsafe {
                &mut *(lparam as *mut windows_sys::Win32::UI::WindowsAndMessaging::MINMAXINFO)
            };
            if let Some((pos, size)) = maximized_to_work_area(hwnd) {
                info.ptMaxPosition = pos;
                info.ptMaxSize = size;
            }
            return res;
        }
        _ => {}
    }
    let res = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
    if msg == WM_NCHITTEST && res == HTCLIENT {
        let (x, y) = at(lparam);
        let mut pt = POINT { x, y };
        // SAFETY: our own window; the point is a local.
        unsafe { ScreenToClient(hwnd, &mut pt) };
        let hit = CAPTION_HITS.with(|m| {
            m.try_borrow().ok().and_then(|m| {
                m.get(&(hwnd as isize)).map(|h| {
                    if sizable
                        && h.max_button
                            .and_then(|i| h.buttons.get(i))
                            .is_some_and(|b| inside(b, pt.x, pt.y))
                    {
                        HT_MAX
                    } else {
                        frame_hit(pt.x, pt.y, h)
                    }
                })
            })
        });
        return hit.unwrap_or(HTCLIENT);
    }
    res
}

/// Where and how large window `hwnd` is when maximized: its monitor's work area (not over the
/// taskbar), with its sizing frame beyond the work area's edges, as a window with a caption
/// style is maximized (Windows maximizes one without a caption over the whole monitor). In the
/// coordinates WM_GETMINMAXINFO takes: from the monitor's top left.
pub(super) fn maximized_to_work_area(
    hwnd: HWND,
) -> Option<(
    windows_sys::Win32::Foundation::POINT,
    windows_sys::Win32::Foundation::POINT,
)> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
    };
    use windows_sys::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow};
    // SAFETY: plain queries about our own window and its monitor, into locals.
    unsafe {
        let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut mi: MONITORINFO = zeroed();
        mi.cbSize = size_of::<MONITORINFO>() as u32;
        if mon.is_null() || GetMonitorInfoW(mon, &mut mi) == 0 {
            return None;
        }
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let mut f = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        AdjustWindowRectExForDpi(&mut f, style, 0, ex, GetDpiForWindow(hwnd).max(96));
        let (w, m) = (mi.rcWork, mi.rcMonitor);
        Some((
            POINT {
                x: w.left - m.left + f.left,
                y: w.top - m.top + f.top,
            },
            POINT {
                x: (w.right - w.left) + f.right - f.left,
                y: (w.bottom - w.top) + f.bottom - f.top,
            },
        ))
    }
}

/// Whether DWM is to be told `dark` for window `hwnd`: not when that is what it was last told.
/// Windows that are gone are forgotten.
pub(super) fn caption_news(hwnd: isize, dark: bool, alive: impl Fn(isize) -> bool) -> bool {
    CAPTION_DARK.with(|m| {
        let Ok(mut m) = m.try_borrow_mut() else {
            return true;
        };
        if m.get(&hwnd) == Some(&dark) {
            return false;
        }
        m.retain(|h, _| alive(*h));
        m.insert(hwnd, dark);
        true
    })
}

/// Give our window titled `title` a dark or light caption (DWM). Returns false if not found.
/// DWM is asked only when the window's caption changes, never again for the same one: a call
/// into DWM waits on a system that is stalled (its disk, its memory), and the window's thread
/// waited with it, every 2 s (#633). Callers call it when the theme changes and for a new window.
pub fn caption_dark(title: &str, dark: bool) -> bool {
    use windows_sys::Win32::Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};
    use windows_sys::Win32::Graphics::Gdi::{RDW_FRAME, RDW_INVALIDATE, RedrawWindow};
    use windows_sys::Win32::UI::WindowsAndMessaging::IsWindow;
    let Some(hwnd) = find_own_window(title) else {
        return false;
    };
    // SAFETY: a plain query; any value is accepted as the handle.
    let alive = |h: isize| unsafe { IsWindow(h as HWND) } != 0;
    if !caption_news(hwnd as isize, dark, alive) {
        return true;
    }
    #[cfg(test)]
    CAPTION_DWM_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let v: i32 = dark.into();
    // SAFETY: documented attribute with a 4-byte BOOL value.
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
            &v as *const i32 as *const c_void,
            size_of::<i32>() as u32,
        );
        // Windows repaints a caption only on the next activation change; force it now.
        RedrawWindow(
            hwnd,
            std::ptr::null(),
            std::ptr::null_mut(),
            RDW_FRAME | RDW_INVALIDATE,
        );
    }
    true
}

/// The popup menus Windows draws for us — the notification-area icon's (tray-icon's), the
/// window menu of a drawn title bar — in DDM's mode, not Windows' (#682): dark on a dark surface
/// (dark mode, a dark theme), light otherwise, whatever Windows' own app mode says. Windows has
/// no documented way: uxtheme's SetPreferredAppMode (ordinal 135, ForceDark 2 / ForceLight 3)
/// sets the process's mode and FlushMenuThemes (ordinal 136) makes menus read it again, as the
/// win32-darkmode project and the apps built on it do. They exist from Windows 10 1903 (build
/// 18362) on; 1809 has another function at 135 and older builds no dark menus, so there menus
/// stay as Windows draws them. Called when the mode changes; asks uxtheme only when it is news.
pub fn menus_dark(dark: bool) {
    use std::sync::atomic::{AtomicU8, Ordering};
    static TOLD: AtomicU8 = AtomicU8::new(u8::MAX);
    let mode: i32 = if dark { 2 } else { 3 };
    if TOLD.swap(mode as u8, Ordering::Relaxed) == mode as u8 {
        return;
    }
    let Some(f) = menu_mode_fns() else {
        return;
    };
    // SAFETY: the functions behind these ordinals on build 18362 and later (menu_mode_fns),
    // SetPreferredAppMode(int) -> int and FlushMenuThemes(void); process-wide, any thread.
    unsafe {
        (f.set_preferred_app_mode)(mode);
        (f.flush_menu_themes)();
    }
}

/// [`MenuModeFns`], looked up once: None before Windows 10 1903 or if uxtheme lacks them.
pub(super) fn menu_mode_fns() -> Option<MenuModeFns> {
    use windows_sys::Win32::System::LibraryLoader::{
        GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
    };
    use windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE;
    static FNS: std::sync::OnceLock<Option<MenuModeFns>> = std::sync::OnceLock::new();
    *FNS.get_or_init(|| {
        let build = reg_string_in(
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
            "CurrentBuild",
        )
        .and_then(|b| b.parse::<u32>().ok())?;
        if build < 18362 {
            return None;
        }
        let name: Vec<u16> = "uxtheme.dll\0".encode_utf16().collect();
        // SAFETY: a NUL-terminated name, loaded from System32 only; the module stays loaded for
        // the process's life (never freed), so the pointers taken from it stay valid. An ordinal
        // goes as the name pointer's low word (MAKEINTRESOURCEA).
        unsafe {
            let module = LoadLibraryExW(
                name.as_ptr(),
                std::ptr::null_mut(),
                LOAD_LIBRARY_SEARCH_SYSTEM32,
            );
            if module.is_null() {
                return None;
            }
            let set = GetProcAddress(module, 135usize as *const u8)?;
            let flush = GetProcAddress(module, 136usize as *const u8)?;
            Some(MenuModeFns {
                set_preferred_app_mode: std::mem::transmute::<
                    unsafe extern "system" fn() -> isize,
                    unsafe extern "system" fn(i32) -> i32,
                >(set),
                flush_menu_themes: std::mem::transmute::<
                    unsafe extern "system" fn() -> isize,
                    unsafe extern "system" fn(),
                >(flush),
            })
        }
    })
}

#[cfg(test)]
mod caption_less_tests {
    use super::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AdjustWindowRectEx, CreateWindowExW, DestroyWindow, GetClientRect, GetWindowRect,
        MINMAXINFO, SendMessageW, WM_GETMINMAXINFO, WS_CAPTION, WS_EX_APPWINDOW, WS_POPUP,
    };

    /// The styles winit gives the progress window (a caption, a sizing frame trimmed by us, a
    /// system menu, Minimize; 0x14CE0000 less WS_VISIBLE) and the main window (Maximize too), and
    /// their extended styles (a window edge, an app window: a taskbar button).
    const WINIT_PROGRESS: u32 = 0x04CE_0000;
    const WINIT_MAIN: u32 = 0x04CF_0000;
    const WINIT_EX: u32 = 0x0004_0100;

    /// The frame (left, top, right, bottom) the styles alone put round a client: what a reader
    /// of the styles, rather than of the window's WM_NCCALCSIZE, takes for it.
    fn styles_frame(style: u32, ex: u32) -> [i32; 4] {
        let mut rc: RECT = unsafe { zeroed() };
        // SAFETY: plain computation on a local rectangle.
        unsafe { AdjustWindowRectEx(&mut rc, style, 0, ex) };
        [-rc.left, -rc.top, rc.right, rc.bottom]
    }

    #[test]
    fn a_drawn_title_bar_s_window_has_no_caption_style() {
        // The caption style the band came from (#208): 31 px above the client, by the styles.
        assert!(styles_frame(WINIT_PROGRESS, WINIT_EX)[1] > 20);
        let fixed = drawn_style(WINIT_PROGRESS, Drawn::Fixed);
        assert_eq!(fixed & WS_CAPTION, 0);
        assert_eq!(fixed & WS_THICKFRAME, 0);
        assert_ne!(fixed & WS_POPUP, 0);
        // The system menu and Minimize stay (Alt+Space, the taskbar's commands).
        assert_eq!(fixed & 0x000A_0000, 0x000A_0000);
        assert_eq!(styles_frame(fixed, drawn_ex_style(WINIT_EX)), [0, 0, 0, 0]);
        // Resizable: the sizing frame, the same on every side (none taller at the top).
        let sizable = drawn_style(WINIT_MAIN, Drawn::Sizable);
        assert_eq!(sizable & WS_CAPTION, 0);
        assert_ne!(sizable & WS_THICKFRAME, 0);
        assert_eq!(sizable & 0x000B_0000, 0x000B_0000);
        let [l, t, r, b] = styles_frame(sizable, WINIT_EX);
        assert!(l > 0 && l == t && t == r && r == b, "{l} {t} {r} {b}");
        // Applied again (winit setting its own styles once more), nothing changes.
        assert_eq!(drawn_style(fixed, Drawn::Fixed), fixed);
        assert_eq!(drawn_style(sizable, Drawn::Sizable), sizable);
        // IDM's dialog frame goes; the taskbar button stays.
        let ex = drawn_ex_style(WINIT_EX | WS_EX_DLGMODALFRAME);
        assert_eq!(ex & WS_EX_DLGMODALFRAME, 0);
        assert_ne!(ex & WS_EX_APPWINDOW, 0);
    }

    /// A real (hidden, off-screen) window with winit's styles, framed by caption_less: its client
    /// is where its styles say, it keeps its styles when they are set again (winit does on every
    /// change of its flags), its edges size it (resizable) or don't (fixed), and maximized it
    /// fills its monitor's work area, not the whole monitor.
    #[test]
    fn caption_less_frames_a_real_window_as_its_styles_say() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{HTBOTTOMRIGHT, HTLEFT, HTTOP};
        for (n, (style, kind)) in [(WINIT_PROGRESS, Drawn::Fixed), (WINIT_MAIN, Drawn::Sizable)]
            .into_iter()
            .enumerate()
        {
            let title = format!("mdm caption-less test {n} {}", std::process::id());
            let wide: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
            // A class of its own whose procedure is the system's (a STATIC control answers
            // every hit test as transparent).
            let class: Vec<u16> = "mdm-caption-less-test\0".encode_utf16().collect();
            // SAFETY: a class registered with the system's own procedure, and a hidden window
            // of it on this thread, destroyed below; the strings are NUL-terminated; plain
            // queries and messages to it.
            unsafe {
                use windows_sys::Win32::UI::WindowsAndMessaging::{
                    DefWindowProcW, RegisterClassW, WNDCLASSW,
                };
                let mut wc: WNDCLASSW = zeroed();
                wc.lpfnWndProc = Some(DefWindowProcW);
                wc.lpszClassName = class.as_ptr();
                // Registered once for both windows (the second call fails: already there).
                RegisterClassW(&wc);
                let hwnd = CreateWindowExW(
                    WINIT_EX,
                    class.as_ptr(),
                    wide.as_ptr(),
                    style,
                    -20000,
                    -20000,
                    548,
                    444,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                );
                assert!(!hwnd.is_null());
                caption_less(hwnd, kind);
                let st = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
                let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
                assert_eq!(st, drawn_style(style, kind));
                // winit setting its caption style again: the subclass keeps ours.
                SetWindowLongPtrW(hwnd, GWL_STYLE, (st | WS_CAPTION) as isize);
                assert_eq!(GetWindowLongPtrW(hwnd, GWL_STYLE) as u32, st);
                // The client is where the styles alone put it: no reading of them can differ.
                let (mut w, mut c): (RECT, RECT) = (zeroed(), zeroed());
                GetWindowRect(hwnd, &mut w);
                GetClientRect(hwnd, &mut c);
                let mut at = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
                windows_sys::Win32::Graphics::Gdi::ClientToScreen(hwnd, &mut at);
                let [l, t, r, b] = styles_frame(st, ex);
                assert_eq!([at.x - w.left, at.y - w.top], [l, t]);
                assert_eq!(
                    [c.right, c.bottom],
                    [w.right - w.left - l - r, w.bottom - w.top - t - b]
                );
                // Its edges: the system's sizing edges where it is resizable, else the client's
                // (the progress window keeps its size).
                let hit = |x: i32, y: i32| {
                    SendMessageW(
                        hwnd,
                        WM_NCHITTEST,
                        0,
                        ((y as u16 as u32) << 16 | x as u16 as u32) as LPARAM,
                    )
                };
                let mid = (w.top + w.bottom) / 2;
                let sizable = kind == Drawn::Sizable;
                assert_eq!(
                    hit(w.left + 1, mid),
                    if sizable { HTLEFT as LRESULT } else { HTCLIENT }
                );
                assert_eq!(
                    hit((w.left + w.right) / 2, w.top + 1),
                    if sizable { HTTOP as LRESULT } else { HTCLIENT }
                );
                assert_eq!(
                    hit(w.right - 2, w.bottom - 2),
                    if sizable {
                        HTBOTTOMRIGHT as LRESULT
                    } else {
                        HTCLIENT
                    }
                );
                // Resized, the client follows as the styles say.
                SetWindowPos(
                    hwnd,
                    std::ptr::null_mut(),
                    0,
                    0,
                    700,
                    500,
                    SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
                GetClientRect(hwnd, &mut c);
                assert_eq!([c.right, c.bottom], [700 - l - r, 500 - t - b]);
                // Maximized (asked, not done: that would show it): its monitor's work area.
                if sizable {
                    let mut info: MINMAXINFO = zeroed();
                    SendMessageW(
                        hwnd,
                        WM_GETMINMAXINFO,
                        0,
                        &mut info as *mut MINMAXINFO as LPARAM,
                    );
                    // The work area with the sizing frame beyond its edges, from the monitor's
                    // top left.
                    use windows_sys::Win32::Graphics::Gdi::{
                        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
                    };
                    let mut mi: MONITORINFO = zeroed();
                    mi.cbSize = size_of::<MONITORINFO>() as u32;
                    GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &mut mi);
                    let (wa, m) = (mi.rcWork, mi.rcMonitor);
                    assert_eq!(
                        [info.ptMaxPosition.x, info.ptMaxPosition.y],
                        [wa.left - m.left - l, wa.top - m.top - t]
                    );
                    assert_eq!(
                        [info.ptMaxSize.x, info.ptMaxSize.y],
                        [wa.right - wa.left + l + r, wa.bottom - wa.top + t + b]
                    );
                }
                DestroyWindow(hwnd);
            }
        }
    }
}

#[cfg(test)]
mod menus_dark_tests {
    /// #682: uxtheme has the menu mode functions on every Windows DDM is tested on (10 1903 and
    /// later, Server 2022's runner included), and setting the mode either way returns.
    #[test]
    fn the_menu_mode_functions_are_found_and_answer() {
        assert!(super::menu_mode_fns().is_some());
        super::menus_dark(true);
        super::menus_dark(false);
    }
}
