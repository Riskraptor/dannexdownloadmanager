//! The dialogs' frames: IDM's dialog frame, the thin frame of a window without a caption, the frame
//! DDM draws itself with its caption buttons (their hit tests, the maximize button's state), and
//! the main window minimized into the tray.

use super::*;

/// How much of the standard 8 px sizing border IDM's progress window does not have: its frame
/// is 2 px (1 px border + 1 px frame line) left, right and bottom (spec "Conventions").
pub(super) const THIN_TRIM: i32 = 6;

/// Hit-test results (winuser.h).
pub(super) const HTCLIENT: LRESULT = 1;

pub(super) const HTCAPTION: LRESULT = 2;

pub(super) const HTSYSMENU: LRESULT = 3;

pub(super) const HTBORDER: LRESULT = 18;

/// The system menu's "Hide to the notification area" (#63): a command id of ours (below
/// 0xF000, its low 4 bits clear, as WM_SYSCOMMAND wants).
pub(super) const SC_HIDE_TO_TRAY: usize = 0x0E10;

/// Where a drawn title bar's parts are, in client pixels (egui publishes them each frame,
/// `set_caption_hits`): the caption strip's height, the icon (the system menu's place) and the
/// caption buttons, each [left, top, right, bottom].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CaptionHits {
    pub strip_h: i32,
    pub icon: [i32; 4],
    pub buttons: Vec<[i32; 4]>,
    /// Which of `buttons` is Maximize, answered as the system's (a resizable window, #45).
    pub max_button: Option<usize>,
}

thread_local! {
    /// Each framed window's caption parts, by HWND. Read in its subclass proc with try_borrow:
    /// WM_NCHITTEST can arrive while egui's frame holds the map (re-entered messages).
    pub(super) static CAPTION_HITS: std::cell::RefCell<std::collections::HashMap<isize, CaptionHits>> =
        Default::default();
    /// Windows whose system menu's "Hide to the notification area" was chosen.
    pub(super) static HIDE_ASKED: std::cell::RefCell<std::collections::HashSet<isize>> = Default::default();
}

/// The Maximize button of a resizable window whose title bar is ours: under the pointer, or
/// pressed. Windows hit-tests it as the system's (HTMAXBUTTON), so Windows 11 shows its Snap
/// Layouts over it; the clicks are then the frame's, and so is its hover, which egui can't see:
/// the title bar reads it from here ([`max_button_state`]).
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct MaxButton {
    pub hot: bool,
    pub pressed: bool,
}

thread_local! {
    pub(super) static MAX_BUTTON: std::cell::RefCell<std::collections::HashMap<isize, MaxButton>> =
        Default::default();
}

/// The main window's "Minimize to the notification area" (#63): its minimize command (title bar,
/// system menu, taskbar) asks the app to hide it instead. The app can't see the window go
/// minimized: an idle app runs no frame while minimized, so it would never know.
pub(super) struct MainMinimize {
    pub(super) hwnd: isize,
    pub(super) ctx: Option<egui::Context>,
    pub(super) enabled: bool,
    pub(super) asked: bool,
}

thread_local! {
    pub(super) static MAIN_MINIMIZE: std::cell::RefCell<MainMinimize> = const {
        std::cell::RefCell::new(MainMinimize { hwnd: 0, ctx: None, enabled: false, asked: false })
    };
}

/// Give our window titled `title` IDM's modal-dialog frame (WS_EX_DLGMODALFRAME, no sizing
/// frame: 3 px borders, 26 px caption) and resize it so the client stays `client` points, at
/// `scale` pixels per point (the display's scaling times any zoom), with the frame measured for
/// the window's own DPI. `light` dialogs get a light caption. `owned`: the main window owns it
/// ([`dialog_owned_by_main`]); else it stays a window of its own, with its taskbar button.
/// Returns true once applied (call again each frame until it is).
pub fn dialog_frame(title: &str, client: [f32; 2], light: bool, scale: f32, owned: bool) -> bool {
    use windows_sys::Win32::Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};
    use windows_sys::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow};
    let Some(hwnd) = find_own_window(title) else {
        return false;
    };
    unsafe {
        // IDM's light dialogs keep a light caption even in dark mode.
        let dark: i32 = (!light).into();
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
            &dark as *const i32 as *const c_void,
            size_of::<i32>() as u32,
        );
        // Owned by the main window, as IDM's dialogs opened from it are: no taskbar button, kept
        // above it. Not one that opened over another program (`owned` false).
        if owned && let Some(main) = find_own_window(crate::MAIN_TITLE) {
            use windows_sys::Win32::UI::WindowsAndMessaging::{GWLP_HWNDPARENT, WS_EX_APPWINDOW};
            SetWindowLongPtrW(hwnd, GWLP_HWNDPARENT, main as isize);
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & !WS_EX_APPWINDOW;
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex as isize);
        }
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32 & !WS_THICKFRAME;
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 | WS_EX_DLGMODALFRAME;
        SetWindowLongPtrW(hwnd, GWL_STYLE, style as isize);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex as isize);
        let mut rc = RECT {
            left: 0,
            top: 0,
            right: (client[0] * scale).round() as i32,
            bottom: (client[1] * scale).round() as i32,
        };
        AdjustWindowRectExForDpi(&mut rc, style, 0, ex, GetDpiForWindow(hwnd));
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            rc.right - rc.left,
            rc.bottom - rc.top,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
    true
}

/// Publish where window `hwnd`'s drawn title bar's parts are (client pixels), for its hit test.
pub fn set_caption_hits(hwnd: isize, hits: CaptionHits) {
    CAPTION_HITS.with(|m| {
        if let Ok(mut m) = m.try_borrow_mut() {
            m.insert(hwnd, hits);
        }
    });
}

/// The window is gone: forget its caption parts.
pub fn forget_caption(hwnd: isize) {
    CAPTION_HITS.with(|m| {
        if let Ok(mut m) = m.try_borrow_mut() {
            m.remove(&hwnd);
        }
    });
    HIDE_ASKED.with(|s| {
        if let Ok(mut s) = s.try_borrow_mut() {
            s.remove(&hwnd);
        }
    });
}

/// Was "Hide to the notification area" chosen in window `hwnd`'s system menu (once)?
pub fn take_hide_asked(hwnd: isize) -> bool {
    HIDE_ASKED.with(|s| s.try_borrow_mut().is_ok_and(|mut s| s.remove(&hwnd)))
}

pub(super) fn inside(r: &[i32; 4], x: i32, y: i32) -> bool {
    x >= r[0] && x < r[2] && y >= r[1] && y < r[3]
}

/// What client point `(x, y)` is: our caption buttons are the client's (egui hovers and clicks
/// them), the icon is the system menu, the rest of the caption strip is the caption (the system
/// moves, snaps and shakes the window from it, double-clicks and right-clicks it), below it the
/// client's.
pub(super) fn frame_hit(x: i32, y: i32, hits: &CaptionHits) -> LRESULT {
    if hits.buttons.iter().any(|b| inside(b, x, y)) {
        HTCLIENT
    } else if inside(&hits.icon, x, y) {
        HTSYSMENU
    } else if y >= 0 && y < hits.strip_h {
        HTCAPTION
    } else {
        HTCLIENT
    }
}

/// IDM's progress-window frame (2 px borders, fixed size) with the title bar ours (#63, the
/// approach Windows Terminal, Zed and WezTerm take): the window keeps its system frame and
/// WS_CAPTION (Aero Snap, shake, the system menu and Alt+Space, the DWM shadow and rounded
/// corners), its caption given to the client, which draws it; the hit test reads where the
/// drawn parts are.
pub(super) unsafe extern "system" fn thin_frame_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{WM_NCACTIVATE, WM_SYSCOMMAND};
    if msg == WM_NCCALCSIZE && wparam != 0 {
        // SAFETY: with wParam TRUE, lParam points to NCCALCSIZE_PARAMS (Win32 contract).
        let p = unsafe { &mut *(lparam as *mut NCCALCSIZE_PARAMS) };
        let top = p.rgrc[0].top;
        let res = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
        p.rgrc[0].left -= THIN_TRIM;
        p.rgrc[0].right += THIN_TRIM;
        p.rgrc[0].bottom += THIN_TRIM;
        // No system caption: the client starts at the window's top.
        p.rgrc[0].top = top;
        return res;
    }
    // Activation changes repaint the non-client area: with lParam -1 the system skips that,
    // so its own caption never flashes over ours.
    if msg == WM_NCACTIVATE {
        return unsafe { DefSubclassProc(hwnd, msg, wparam, -1) };
    }
    if msg == WM_SYSCOMMAND && (wparam & 0xFFF0) == SC_HIDE_TO_TRAY {
        HIDE_ASKED.with(|s| {
            if let Ok(mut s) = s.try_borrow_mut() {
                s.insert(hwnd as isize);
            }
        });
        return 0;
    }
    let res = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
    if msg == WM_NCHITTEST {
        // Fixed size, like IDM's: the sizing edges (HTLEFT..HTBOTTOMRIGHT) act as plain border.
        if (10..=17).contains(&res) {
            return HTBORDER;
        }
        if res == HTCLIENT {
            use windows_sys::Win32::Foundation::POINT;
            use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
            let mut pt = POINT {
                x: (lparam & 0xffff) as i16 as i32,
                y: ((lparam >> 16) & 0xffff) as i16 as i32,
            };
            // SAFETY: our own window; the point is a local.
            unsafe { ScreenToClient(hwnd, &mut pt) };
            let hit = CAPTION_HITS.with(|m| {
                m.try_borrow()
                    .ok()
                    .and_then(|m| m.get(&(hwnd as isize)).map(|h| frame_hit(pt.x, pt.y, h)))
            });
            return hit.unwrap_or(HTCLIENT);
        }
    }
    res
}

/// Window `hwnd`'s Maximize button's state (see [`MaxButton`]).
pub fn max_button_state(hwnd: isize) -> MaxButton {
    MAX_BUTTON.with(|m| {
        m.try_borrow()
            .ok()
            .and_then(|m| m.get(&hwnd).copied())
            .unwrap_or_default()
    })
}

pub(super) fn set_max_button(hwnd: HWND, state: MaxButton) {
    use windows_sys::Win32::Graphics::Gdi::InvalidateRect;
    let changed = MAX_BUTTON.with(|m| {
        m.try_borrow_mut()
            .is_ok_and(|mut m| m.insert(hwnd as isize, state) != Some(state))
    });
    if changed {
        // SAFETY: our own window; a repaint is asked for (winit draws a frame).
        unsafe { InvalidateRect(hwnd, std::ptr::null(), 0) };
    }
}

/// The sizing frame's thickness at the window's top, in client pixels (Windows' own measure for
/// the window's DPI).
pub(super) fn top_frame(hwnd: HWND) -> i32 {
    use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
    use windows_sys::Win32::UI::WindowsAndMessaging::{SM_CXPADDEDBORDER, SM_CYSIZEFRAME};
    // SAFETY: plain calls about our own window.
    unsafe {
        let dpi = GetDpiForWindow(hwnd).max(96);
        GetSystemMetricsForDpi(SM_CYSIZEFRAME, dpi) + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi)
    }
}

/// A resizable window with its title bar drawn by us (the main window with a style theme, #45),
/// as Windows Terminal does it: the system frame and WS_CAPTION stay (resizing on every edge,
/// Aero Snap and shake, the system menu and Alt+Space, the DWM shadow), the caption is given to
/// the client, which draws it; the hit test reads where the drawn parts are. The top sizing edge
/// is ours (the client starts at the window's top), and Maximize answers as the system's.
pub(super) unsafe extern "system" fn drawn_frame_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT, TrackMouseEvent,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HTMAXBUTTON, HTTOP, HTTOPLEFT, HTTOPRIGHT, IsZoomed, SC_MAXIMIZE, SC_RESTORE, SendMessageW,
        WM_NCACTIVATE, WM_NCLBUTTONDBLCLK, WM_NCLBUTTONDOWN, WM_NCLBUTTONUP, WM_NCMOUSELEAVE,
        WM_NCMOUSEMOVE, WM_SYSCOMMAND,
    };
    const HT_MAX: LRESULT = HTMAXBUTTON as LRESULT;
    if msg == WM_NCCALCSIZE && wparam != 0 {
        // SAFETY: with wParam TRUE, lParam points to NCCALCSIZE_PARAMS (Win32 contract).
        let p = unsafe { &mut *(lparam as *mut NCCALCSIZE_PARAMS) };
        let top = p.rgrc[0].top;
        let res = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
        // No system caption: the client starts at the window's top; maximized, the window
        // overhangs the screen by its frame, so the bar starts that much lower.
        // SAFETY: our own window.
        let zoomed = unsafe { IsZoomed(hwnd) } != 0;
        p.rgrc[0].top = if zoomed { top + top_frame(hwnd) } else { top };
        return res;
    }
    if msg == WM_NCACTIVATE {
        return unsafe { DefSubclassProc(hwnd, msg, wparam, -1) };
    }
    if msg == WM_SYSCOMMAND && (wparam & 0xFFF0) == SC_HIDE_TO_TRAY {
        HIDE_ASKED.with(|s| {
            if let Ok(mut s) = s.try_borrow_mut() {
                s.insert(hwnd as isize);
            }
        });
        return 0;
    }
    // Maximize: pressed and released on it toggles the window (the system's own button would
    // draw itself over ours).
    if (msg == WM_NCLBUTTONDOWN || msg == WM_NCLBUTTONDBLCLK) && wparam as LRESULT == HT_MAX {
        set_max_button(
            hwnd,
            MaxButton {
                hot: true,
                pressed: true,
            },
        );
        return 0;
    }
    if msg == WM_NCLBUTTONUP && wparam as LRESULT == HT_MAX {
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
    if msg == WM_NCMOUSEMOVE {
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
    if msg == WM_NCMOUSELEAVE {
        set_max_button(hwnd, MaxButton::default());
    }
    let res = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
    if msg == WM_NCHITTEST && res == HTCLIENT {
        use windows_sys::Win32::Foundation::POINT;
        use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
        let mut pt = POINT {
            x: (lparam & 0xffff) as i16 as i32,
            y: ((lparam >> 16) & 0xffff) as i16 as i32,
        };
        // SAFETY: our own window; the point is a local.
        unsafe { ScreenToClient(hwnd, &mut pt) };
        // The top sizing edge (corners too), where the system's caption had it.
        // SAFETY: our own window.
        if unsafe { IsZoomed(hwnd) } == 0 && pt.y >= 0 && pt.y < top_frame(hwnd) {
            let mut rc: RECT = unsafe { zeroed() };
            unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rc) };
            let corner = 2 * top_frame(hwnd);
            return if pt.x < corner {
                HTTOPLEFT as LRESULT
            } else if pt.x >= rc.right - corner {
                HTTOPRIGHT as LRESULT
            } else {
                HTTOP as LRESULT
            };
        }
        let hit = CAPTION_HITS.with(|m| {
            m.try_borrow().ok().and_then(|m| {
                m.get(&(hwnd as isize)).map(|h| {
                    if h.max_button
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

/// Give the resizable window titled `title` its title bar drawn by us (`on`), or the system's
/// back. Returns the window when done.
pub fn drawn_frame(title: &str, on: bool) -> Option<isize> {
    use windows_sys::Win32::UI::Shell::RemoveWindowSubclass;
    let hwnd = find_own_window(title)?;
    if caption_less_frames() {
        // SAFETY: our own window, on its thread.
        unsafe {
            if on {
                caption_less(hwnd, Drawn::Sizable);
            } else {
                caption_back(hwnd);
            }
        }
        if !on {
            forget_caption(hwnd as isize);
        }
        return Some(hwnd as isize);
    }
    // SAFETY: our own window; the subclass id is ours.
    unsafe {
        if on {
            SetWindowSubclass(hwnd, Some(drawn_frame_proc), 0x1d7, 0);
        } else {
            RemoveWindowSubclass(hwnd, Some(drawn_frame_proc), 0x1d7);
            forget_caption(hwnd as isize);
        }
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED
                | SWP_NOMOVE
                | windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOSIZE
                | SWP_NOZORDER
                | SWP_NOACTIVATE,
        );
    }
    Some(hwnd as isize)
}

/// Give our window titled `title` IDM's thin progress-window frame with its title bar drawn by
/// us (winui::caption, #63): 2 px borders, no system caption, the client staying `client`
/// points (the drawn title bar included) at `scale` pixels per point. With `hide_item` (the
/// translated "Hide to the notification area"), the system menu gets it, Alt+Space reaching it
/// from the keyboard; None takes it out. Call while the window is still hidden, and again when
/// the size, the scale or the item changes. Returns the window, once applied.
pub fn thin_frame(
    title: &str,
    client: [f32; 2],
    scale: f32,
    hide_item: Option<&str>,
) -> Option<isize> {
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, DeleteMenu, GetSystemMenu, MF_BYCOMMAND, MF_STRING,
    };
    let hwnd = find_own_window(title)?;
    let plain = caption_less_frames();
    unsafe {
        if plain {
            caption_less(hwnd, Drawn::Fixed);
        } else {
            SetWindowSubclass(hwnd, Some(thin_frame_proc), 0x1d6, 0);
        }
        let menu = GetSystemMenu(hwnd, 0);
        if !menu.is_null() {
            DeleteMenu(menu, SC_HIDE_TO_TRAY as u32, MF_BYCOMMAND);
            if let Some(text) = hide_item {
                let w: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
                AppendMenuW(menu, MF_STRING, SC_HIDE_TO_TRAY, w.as_ptr());
            }
        }
        // The thin frame's 2 px each side but the top, or none: a plain window is all client
        // (Windows 11 draws its 1 px border over the client's edge).
        let dpi = GetDpiForWindow(hwnd).max(96) as f32 / 96.0;
        let (fw, fh) = if plain { (0.0, 0.0) } else { (4.0, 2.0) };
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            (client[0] * scale + fw * dpi).round() as i32,
            (client[1] * scale + fh * dpi).round() as i32,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
    Some(hwnd as isize)
}

pub(super) unsafe extern "system" fn main_minimize_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetCursorPos, GetWindowRect, WM_SYSCOMMAND};
    const SC_MINIMIZE: usize = 0xF020;
    // Only a minimize asked in the window itself (its title bar's button, its system menu: the
    // pointer is on the window) goes to the tray. A click on its taskbar button, Win+Down and
    // the like minimize it to the taskbar, as Windows does for any window (the owner found the
    // taskbar button vanishing under his click confusing).
    let in_window = || {
        let mut at = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
        let mut rc = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        // SAFETY: plain queries about the pointer and our own window.
        unsafe {
            GetCursorPos(&mut at) != 0
                && GetWindowRect(hwnd, &mut rc) != 0
                && (rc.left..rc.right).contains(&at.x)
                && (rc.top..rc.bottom).contains(&at.y)
        }
    };
    if msg == WM_SYSCOMMAND && (wparam & 0xFFF0) == SC_MINIMIZE && in_window() {
        let caught = MAIN_MINIMIZE.with(|m| {
            let Ok(mut m) = m.try_borrow_mut() else {
                return false;
            };
            if !m.enabled {
                return false;
            }
            m.asked = true;
            if let Some(ctx) = &m.ctx {
                ctx.request_repaint();
            }
            true
        });
        if caught {
            return 0;
        }
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

/// Keep the main window's minimize command in step with "Minimize to the notification area"
/// (`enabled`: the option is on and a tray shows DDM's icon). Call every frame (cheap once the
/// window is found). True once after the command came: hide the window now.
pub fn main_minimize_to_tray(ctx: &egui::Context, enabled: bool) -> bool {
    MAIN_MINIMIZE.with(|m| {
        let Ok(mut m) = m.try_borrow_mut() else {
            return false;
        };
        if m.hwnd == 0
            && let Some(hwnd) = find_own_window(crate::MAIN_TITLE)
        {
            // SAFETY: our own window; the proc lives as long as the program.
            unsafe { SetWindowSubclass(hwnd, Some(main_minimize_proc), 0x7a1, 0) };
            m.hwnd = hwnd as isize;
            m.ctx = Some(ctx.clone());
        }
        m.enabled = enabled;
        std::mem::take(&mut m.asked)
    })
}

#[cfg(test)]
mod frame_tests {
    use super::*;

    #[test]
    fn the_drawn_title_bar_hits_as_caption_icon_and_buttons() {
        // 548 × 32 points at 1.5 px per point, 4 buttons of 46 at the end, the icon at the start.
        let h = CaptionHits {
            strip_h: 48,
            icon: [0, 0, 48, 48],
            buttons: (0..4)
                .map(|i| {
                    let x1 = 822 - i * 69;
                    [x1 - 69, 0, x1, 48]
                })
                .collect(),
            max_button: None,
        };
        assert_eq!(frame_hit(10, 10, &h), HTSYSMENU);
        assert_eq!(frame_hit(300, 10, &h), HTCAPTION);
        assert_eq!(frame_hit(821, 10, &h), HTCLIENT);
        assert_eq!(frame_hit(822 - 4 * 69, 10, &h), HTCLIENT);
        assert_eq!(frame_hit(822 - 4 * 69 - 1, 10, &h), HTCAPTION);
        // Below the bar: the client's.
        assert_eq!(frame_hit(300, 48, &h), HTCLIENT);
        // Nothing published yet: everything the client's (frame_hit isn't asked then).
        assert_eq!(frame_hit(300, 10, &CaptionHits::default()), HTCLIENT);
    }
}
