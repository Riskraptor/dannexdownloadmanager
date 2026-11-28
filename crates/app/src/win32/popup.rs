//! Pop-up windows (menus, lists, tips): their styles, shown without taking the focus, their border,
//! and the menus' text width.

use super::*;

/// What a borderless window of ours is, for [`popup_window`]: every one is a plain popup
/// ([`popup_style`], #208); they differ in how they take the pointer and how they look.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Popup {
    /// A menu (chrome.rs): never activated, so the main window keeps the focus and the
    /// keyboard; Windows 11's small rounded corners and a 1 px border in this colour
    /// (0x00BBGGRR); no window-open animation (menus just appear).
    Menu { border: u32 },
    /// A small floating window (the drop target, droptarget.rs): activated when clicked, as
    /// any window; Windows 11's rounded corners, border and shadow, as it had with its caption
    /// style.
    Floating,
}

/// The subclass of every borderless window of ours ([`popup_proc`]): its id, and its data's
/// flag for a window never activated (a menu).
pub(super) const POPUP_SUBCLASS: usize = 0x3e1;

pub(super) const POPUP_NO_ACTIVATE: usize = 1;

/// A borderless window's style: a plain popup, as Win32's own menus are, without the caption,
/// frame and caption buttons winit gives every borderless window (it keeps WS_CAPTION and hides
/// the frame in WM_NCCALCSIZE). With a caption style, whatever lays the window out by its
/// styles rather than by our WM_NCCALCSIZE puts the client a caption lower and a frame to the
/// right: on a Windows 11 Insider build (22635) each menu, and the drop target, showed a black
/// band a caption tall across its top and its picture moved down by as much (#208). Without
/// one, every reading agrees: the client is the whole window.
pub(crate) fn popup_style(style: u32) -> u32 {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        WS_CAPTION, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU,
    };
    (style & !(WS_CAPTION | WS_THICKFRAME | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX))
        | WS_POPUP
}

/// The extended styles a borderless window keeps: no taskbar button or Alt+Tab entry (a tool
/// window, without the app window style winit gives every window, which asks for a button),
/// and a menu is never activated.
pub(super) fn popup_ex_style(ex: u32, no_activate: bool) -> u32 {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };
    (ex & !WS_EX_APPWINDOW) | WS_EX_TOOLWINDOW | if no_activate { WS_EX_NOACTIVATE } else { 0 }
}

/// Every borderless window of ours: it keeps its popup styles when winit sets the styles again
/// (it does on every change of its window flags, showing the window included), and its client
/// is the whole window, with no frame (winit's borderless frame moves the client a pixel down,
/// for a shadow drawn round a caption style, which a popup has no need of). A menu's (`data`
/// has [`POPUP_NO_ACTIVATE`]) answers mouse-activation with "don't activate": clicking an item
/// must not take focus from the main window (which would close the menu before the click
/// completes).
pub(super) unsafe extern "system" fn popup_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    data: usize,
) -> LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{STYLESTRUCT, WM_STYLECHANGING};
    const WM_MOUSEACTIVATE: u32 = 0x0021;
    const MA_NOACTIVATE: LRESULT = 3;
    let no_activate = data & POPUP_NO_ACTIVATE != 0;
    if msg == WM_MOUSEACTIVATE && no_activate {
        return MA_NOACTIVATE;
    }
    if msg == WM_STYLECHANGING && lparam != 0 {
        // SAFETY: for WM_STYLECHANGING, lParam points to the STYLESTRUCT of the change, whose
        // styleNew the window may alter (Win32 contract).
        let s = unsafe { &mut *(lparam as *mut STYLESTRUCT) };
        match wparam as i32 {
            GWL_STYLE => s.styleNew = popup_style(s.styleNew),
            GWL_EXSTYLE => s.styleNew = popup_ex_style(s.styleNew, no_activate),
            _ => {}
        }
        return 0;
    }
    if msg == WM_NCCALCSIZE {
        // No non-client area: the proposed window rectangle (rgrc[0], or the RECT itself when
        // wParam is FALSE) stays as the client's.
        return 0;
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

/// Make our borderless window titled `title` a plain popup window whose client is all of it
/// ([`popup_style`], #208), with no taskbar entry, and the rest of what `kind` says. Every
/// borderless window of ours comes here (chrome.rs's `popup_viewport` and `style_popup`). Call
/// it in the window's own pass, before anything is drawn in it: its frame changes, and a
/// picture drawn before that is lost with the old frame (black until something makes it draw
/// again). Cheap to call every frame: returns true only on the call that applied it; the window
/// is created hidden and should be shown then.
pub fn popup_window(title: &str, kind: Popup) -> bool {
    use windows_sys::Win32::Graphics::Dwm::{
        DWMWA_BORDER_COLOR, DWMWA_TRANSITIONS_FORCEDISABLED, DWMWA_WINDOW_CORNER_PREFERENCE,
        DWMWCP_ROUND, DWMWCP_ROUNDSMALL, DwmSetWindowAttribute,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetPropW, SWP_NOSIZE, SetPropW};
    let Some(hwnd) = find_own_window(title) else {
        return false;
    };
    let no_activate = matches!(kind, Popup::Menu { .. });
    // SAFETY: our own window, on its thread; the subclass proc lives as long as the program;
    // the DWM attributes are read from locals during the calls.
    unsafe {
        // Done once (comctl32 5.8, which DDM links, has no GetWindowSubclass to ask by).
        let done: Vec<u16> = "DDM.popup\0".encode_utf16().collect();
        if !GetPropW(hwnd, done.as_ptr()).is_null() {
            return false;
        }
        SetPropW(hwnd, done.as_ptr(), 1usize as _);
        if let Popup::Menu { .. } = kind {
            let off: i32 = 1;
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_TRANSITIONS_FORCEDISABLED as u32,
                &off as *const i32 as *const c_void,
                size_of::<i32>() as u32,
            );
        }
        // The subclass first: it keeps the styles below when winit sets its own again.
        let data = if no_activate { POPUP_NO_ACTIVATE } else { 0 };
        SetWindowSubclass(hwnd, Some(popup_proc), POPUP_SUBCLASS, data);
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, popup_ex_style(ex, no_activate) as isize);
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        SetWindowLongPtrW(hwnd, GWL_STYLE, popup_style(style) as isize);
        // The frame measured again (WM_NCCALCSIZE): no caption, the client the whole window.
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
        // Windows 11 rounds a window with a caption style by itself, not a popup: asked for.
        let corner = match kind {
            Popup::Menu { .. } => DWMWCP_ROUNDSMALL,
            Popup::Floating => DWMWCP_ROUND,
        };
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            &corner as *const _ as *const c_void,
            size_of::<i32>() as u32,
        );
        if let Popup::Menu { border } = kind {
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_BORDER_COLOR as u32,
                &border as *const u32 as *const c_void,
                size_of::<u32>() as u32,
            );
        }
    }
    true
}

/// The 1 px border Windows 11 draws round our borderless window titled `title` (the drop target)
/// in `color` (0x00BBGGRR). Windows 10 has no such border: nothing happens there.
pub fn popup_border(title: &str, color: u32) {
    use windows_sys::Win32::Graphics::Dwm::{DWMWA_BORDER_COLOR, DwmSetWindowAttribute};
    let Some(hwnd) = find_own_window(title) else {
        return;
    };
    // SAFETY: our own window; the colour is read from a local during the call.
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR as u32,
            &color as *const u32 as *const c_void,
            size_of::<u32>() as u32,
        );
    }
}

/// Width of `text` in Segoe UI 12 px as GDI measures it (whole-pixel hinted advances) — the
/// width Win32 menus size themselves by. Cached per string.
pub fn menu_text_width(text: &str) -> i32 {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use windows_sys::Win32::Foundation::SIZE;
    use windows_sys::Win32::Graphics::Gdi::{
        CreateFontW, GetTextExtentPoint32W, HFONT, SelectObject,
    };
    thread_local! {
        static FONT: HFONT = {
            let face: Vec<u16> = "Segoe UI\0".encode_utf16().collect();
            // SAFETY: plain GDI object creation; the font lives for the thread.
            unsafe { CreateFontW(-12, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 0, 0, face.as_ptr()) }
        };
        static CACHE: RefCell<HashMap<String, i32>> = RefCell::new(HashMap::new());
    }
    if let Some(w) = CACHE.with(|c| c.borrow().get(text).copied()) {
        return w;
    }
    let wide: Vec<u16> = text.encode_utf16().collect();
    let w = FONT.with(|font| unsafe {
        let dc = GetDC(std::ptr::null_mut());
        let old = SelectObject(dc, *font);
        let mut size = SIZE { cx: 0, cy: 0 };
        GetTextExtentPoint32W(dc, wide.as_ptr(), wide.len() as i32, &mut size);
        SelectObject(dc, old);
        ReleaseDC(std::ptr::null_mut(), dc);
        size.cx
    });
    CACHE.with(|c| c.borrow_mut().insert(text.to_owned(), w));
    w
}

#[cfg(test)]
mod popup_style_tests {
    use super::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AdjustWindowRectEx, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_EX_APPWINDOW, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
    };

    /// winit's borderless window as it creates a menu or the drop target (style 0x16CB0000: a
    /// caption, system menu, minimize and maximize boxes), and the extended styles
    /// popup_window gives a menu and the drop target (always on top, a window edge).
    const WINIT_BORDERLESS: u32 = 0x16CB_0000;
    const MENU_EX: u32 = 0x0804_0198;
    const DROP_TARGET_EX: u32 = 0x0000_0198;

    /// The frame (left, top, right, bottom) the styles alone put round a `w` × `h` client: what
    /// a reader of the styles, rather than of our WM_NCCALCSIZE, takes for it.
    fn frame(style: u32, ex: u32, w: i32, h: i32) -> [i32; 4] {
        let mut rc = RECT {
            left: 0,
            top: 0,
            right: w,
            bottom: h,
        };
        // SAFETY: plain computation on a local rectangle.
        unsafe { AdjustWindowRectEx(&mut rc, style, 0, ex) };
        [-rc.left, -rc.top, rc.right - w, rc.bottom - h]
    }

    #[test]
    fn a_menu_window_is_a_popup_whose_client_is_all_of_it() {
        // As created, the styles alone give it a caption and a frame: what a reader of the
        // styles (rather than of our WM_NCCALCSIZE) took for its client on build 22635 (#208).
        assert_ne!(frame(WINIT_BORDERLESS, MENU_EX, 279, 275), [0, 0, 0, 0]);
        let style = popup_style(WINIT_BORDERLESS);
        assert_eq!(
            style,
            WS_POPUP | WS_VISIBLE | WS_CLIPSIBLINGS | WS_CLIPCHILDREN
        );
        // As a popup, no reading of its styles finds a frame (at any scale: none to scale).
        assert_eq!(frame(style, MENU_EX, 279, 275), [0, 0, 0, 0]);
        // Applied again (winit setting its styles once more), nothing changes.
        assert_eq!(popup_style(style), style);
    }

    #[test]
    fn the_drop_target_is_a_popup_whose_client_is_all_of_it() {
        // The black band left on build 22635 once menus were fixed (#208): a caption about
        // 26 px tall over the 56 px box, its picture moved down by as much.
        let [_, top, _, _] = frame(WINIT_BORDERLESS, 0x0004_0118, 56, 56);
        assert!(top > 20, "a caption's height above its client: {top}");
        let style = popup_style(WINIT_BORDERLESS);
        assert_eq!(
            style,
            WS_POPUP | WS_VISIBLE | WS_CLIPSIBLINGS | WS_CLIPCHILDREN
        );
        // As winit makes it (always on top, a window edge, files accepted, an app window).
        let ex = popup_ex_style(0x0004_0118, false);
        assert_eq!(ex, DROP_TARGET_EX);
        assert_eq!(frame(style, ex, 56, 56), [0, 0, 0, 0]);
        assert_eq!(popup_style(style), style);
    }

    #[test]
    fn a_popup_keeps_its_extended_styles() {
        // winit's own extended styles for it (always on top, a window edge, files accepted,
        // an app window), without ours: they come back, but for the app window's.
        let winit_ex = WS_EX_TOPMOST | 0x0000_0100 | 0x0000_0010 | WS_EX_APPWINDOW;
        let menu = popup_ex_style(winit_ex, true);
        assert_eq!(menu & WS_EX_APPWINDOW, 0);
        assert_eq!(menu & WS_EX_NOACTIVATE, WS_EX_NOACTIVATE);
        assert_eq!(menu & WS_EX_TOOLWINDOW, WS_EX_TOOLWINDOW);
        assert_eq!(menu & WS_EX_TOPMOST, WS_EX_TOPMOST);
        // The drop target is activated as any window (a click on it, a drag), but has no
        // taskbar button either; an off-screen test copy's never-activate stays.
        let floating = popup_ex_style(winit_ex, false);
        assert_eq!(floating & WS_EX_NOACTIVATE, 0);
        assert_eq!(floating & WS_EX_TOOLWINDOW, WS_EX_TOOLWINDOW);
        assert_eq!(floating & WS_EX_TOPMOST, WS_EX_TOPMOST);
        assert_eq!(
            popup_ex_style(winit_ex | WS_EX_NOACTIVATE, false) & WS_EX_NOACTIVATE,
            WS_EX_NOACTIVATE
        );
    }

    /// A real (hidden, off-screen) window with winit's borderless styles, made a popup by
    /// popup_window: plain popup styles, its client the whole window, and both kept when its
    /// styles are set again (winit does on every change of its flags).
    #[test]
    fn popup_window_makes_a_real_window_a_popup_and_keeps_it_one() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, GetClientRect, GetWindowRect, WS_CAPTION,
        };
        for (n, kind) in [
            Popup::Floating,
            Popup::Menu {
                border: 0x00ff_0000,
            },
        ]
        .into_iter()
        .enumerate()
        {
            let title = format!("mdm popup test {n} {}", std::process::id());
            let wide: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
            let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            // SAFETY: a hidden window of the system's STATIC class on this thread, destroyed
            // below; the strings are NUL-terminated.
            let hwnd = unsafe {
                CreateWindowExW(
                    0x0000_0108,
                    class.as_ptr(),
                    wide.as_ptr(),
                    WINIT_BORDERLESS & !WS_VISIBLE,
                    -20000,
                    -20000,
                    56,
                    56,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            };
            assert!(!hwnd.is_null());
            assert!(popup_window(&title, kind), "applied once");
            assert!(!popup_window(&title, kind), "only once");
            // SAFETY: our own window; plain queries and style changes, as winit's.
            unsafe {
                let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
                assert_eq!(style & WS_CAPTION, 0);
                assert_ne!(style & WS_POPUP, 0);
                // winit setting its caption style again: the subclass keeps the popup's.
                SetWindowLongPtrW(hwnd, GWL_STYLE, (style | WS_CAPTION) as isize);
                assert_eq!(GetWindowLongPtrW(hwnd, GWL_STYLE) as u32, style);
                let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, 0x0000_0108);
                assert_eq!(GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32, ex);
                assert_eq!(
                    ex & WS_EX_NOACTIVATE != 0,
                    matches!(kind, Popup::Menu { .. })
                );
                // Its size as the drop target's (a window with a caption style can't be made
                // that narrow: a popup can), then its client is the whole window.
                SetWindowPos(
                    hwnd,
                    std::ptr::null_mut(),
                    0,
                    0,
                    56,
                    56,
                    SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
                let (mut w, mut c): (RECT, RECT) = (zeroed(), zeroed());
                GetWindowRect(hwnd, &mut w);
                GetClientRect(hwnd, &mut c);
                assert_eq!([w.right - w.left, w.bottom - w.top], [56, 56]);
                assert_eq!([c.left, c.top, c.right, c.bottom], [0, 0, 56, 56]);
                DestroyWindow(hwnd);
            }
        }
    }
}
