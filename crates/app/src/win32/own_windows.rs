//! DDM's own windows: found by title among the process's, the main window kept and brought
//! forward, dialogs owned by it, a window shown without taking the keyboard, and the screen's work
//! area.

use super::*;

pub(super) struct Find {
    pub(super) pid: u32,
    pub(super) title: Vec<u16>,
    /// Where each window's title is read: as long as the title looked for, one more unit and the
    /// NUL, so a longer title reads longer, never the same. A fixed 256 units cut every title to
    /// 255, and a window named after a long file was never found (#540).
    pub(super) buf: Vec<u16>,
    pub(super) found: HWND,
}

/// The main window of this run, once it exists ([`set_main_window`]). 0: not known yet.
pub(super) static MAIN_WINDOW: std::sync::atomic::AtomicIsize =
    std::sync::atomic::AtomicIsize::new(0);

/// One of our windows by handle, for crates that take a `raw-window-handle` window.
pub(super) struct OwnWindow(pub(super) HWND);

impl raw_window_handle::HasWindowHandle for OwnWindow {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        let h = std::num::NonZeroIsize::new(self.0 as isize)
            .ok_or(raw_window_handle::HandleError::Unavailable)?;
        let raw =
            raw_window_handle::RawWindowHandle::Win32(raw_window_handle::Win32WindowHandle::new(h));
        // SAFETY: the handle belongs to a live window of this process for the call's duration.
        Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(raw) })
    }
}

pub(super) unsafe extern "system" fn enum_cb(
    hwnd: HWND,
    lparam: LPARAM,
) -> windows_sys::core::BOOL {
    // SAFETY: lparam is the &mut Find passed to EnumWindows below, alive for the call.
    let f = unsafe { &mut *(lparam as *mut Find) };
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    if pid != f.pid {
        return 1;
    }
    // The title as the window keeps it, read without a message: GetWindowTextW sends WM_GETTEXT
    // to a window of this process, and one whose thread is busy (on a stalled disk: the file
    // icons' COM window) held the window's thread until it answered (#583).
    // SAFETY: InternalGetWindowText writes at most buf.len() units, the NUL included.
    let n = unsafe { InternalGetWindowText(hwnd, f.buf.as_mut_ptr(), f.buf.len() as i32) };
    if n > 0 && f.buf[..n as usize] == f.title[..] {
        f.found = hwnd;
        return 0;
    }
    1
}

/// The window of this thread that holds the mouse capture (a button is down in it), if any.
pub(super) fn own_capture() -> Option<HWND> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetCapture;
    // SAFETY: a plain query; GetCapture only sees this thread's windows.
    let h = unsafe { GetCapture() };
    (!h.is_null()).then_some(h)
}

/// Whether the main window is in front: no visible window of another program covers any part
/// of it from above. A tray click asks (#520): the click itself takes the focus first, and with
/// the icon under the taskbar's ^ the focus went to the flyout well before, so focus alone
/// can't tell. The taskbar and its flyout, always on top, don't count, nor do hidden, minimized
/// or cloaked windows (another virtual desktop, a suspended app) or this program's own (its
/// dialogs over it). None: the main window isn't known yet.
pub fn main_window_in_front() -> Option<bool> {
    use windows_sys::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GW_HWNDPREV, GetWindow, GetWindowRect, IsIconic, IsWindowVisible, WS_EX_TOPMOST,
    };
    let main = MAIN_WINDOW.load(std::sync::atomic::Ordering::Relaxed) as HWND;
    if main.is_null() {
        return None;
    }
    let rect = |h: HWND| {
        let mut r = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        // SAFETY: a read into a local; a gone window fails harmlessly.
        (unsafe { GetWindowRect(h, &mut r) } != 0 && r.right > r.left && r.bottom > r.top)
            .then_some(r)
    };
    let m = rect(main)?;
    // SAFETY: plain queries about windows, walking the z-order up from ours.
    let me = unsafe { GetCurrentProcessId() };
    let mut h = unsafe { GetWindow(main, GW_HWNDPREV) };
    // A bound, in case windows come and go under the walk.
    for _ in 0..4096 {
        if h.is_null() {
            break;
        }
        let mut pid = 0;
        // SAFETY: as above.
        let (visible, topmost) = unsafe {
            GetWindowThreadProcessId(h, &mut pid);
            (
                IsWindowVisible(h) != 0 && IsIconic(h) == 0,
                GetWindowLongPtrW(h, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST != 0,
            )
        };
        if visible && !topmost && pid != me {
            let mut cloaked: u32 = 0;
            // SAFETY: a u32 read into a local of that size.
            let known = unsafe {
                DwmGetWindowAttribute(
                    h,
                    DWMWA_CLOAKED as _,
                    (&mut cloaked as *mut u32).cast(),
                    std::mem::size_of::<u32>() as u32,
                )
            } >= 0;
            if !(known && cloaked != 0)
                && let Some(r) = rect(h)
                && r.left < m.right
                && m.left < r.right
                && r.top < m.bottom
                && m.top < r.bottom
            {
                return Some(false);
            }
        }
        // SAFETY: as above.
        h = unsafe { GetWindow(h, GW_HWNDPREV) };
    }
    Some(true)
}

/// Remember the window the app was made for: the one that stays. A way of drawing that fails
/// (Direct3D with no usable adapter, renderer.rs) leaves its own window behind under the same
/// title until the event loop next runs, and a dialog owned by that window goes down with it
/// (#337). Its icons are made crisp there ([`crisp_icons`]).
pub fn set_main_window(cc: &eframe::CreationContext<'_>) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    if let Ok(RawWindowHandle::Win32(h)) = cc.window_handle().map(|h| h.as_raw()) {
        MAIN_WINDOW.store(h.hwnd.get(), std::sync::atomic::Ordering::Relaxed);
        crisp_icons_for(h.hwnd.get() as HWND);
    }
}

/// Run the main window's next pass now, while it is hidden in the notification area or
/// minimized (#3): from any thread, at once. eframe holds a hidden or minimized window's passes
/// at least 100 ms apart (`INVISIBLE_WINDOW_REPAINT_INTERVAL`, against a busy loop: Windows
/// sends such a window no `WM_PAINT`), so a download the browser handed over waited up to
/// 100 ms for the pass that takes it, and as long again for each pass that opens Download File
/// Info (the window is made, then shown). A `WM_PAINT` posted to the window is winit's
/// `RedrawRequested`, which eframe answers with a pass at once. Asked only where a pass is needed
/// (a command came, a window opens), never every frame, so an idle hidden window stays idle. A
/// shown window has its own `WM_PAINT`s: nothing is posted to it.
pub fn nudge_main_window() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IsIconic, IsWindowVisible, PostMessageW, WM_PAINT,
    };
    let main = MAIN_WINDOW.load(std::sync::atomic::Ordering::Relaxed) as HWND;
    if main.is_null() {
        return;
    }
    // SAFETY: plain queries and a posted message without pointers; a window gone meanwhile
    // makes them fail harmlessly.
    unsafe {
        if IsWindowVisible(main) == 0 || IsIconic(main) != 0 {
            PostMessageW(main, WM_PAINT, 0, 0);
        }
    }
}

/// This process's top-level window with exactly `title`; the main window by its handle.
pub(crate) fn find_own_window(title: &str) -> Option<HWND> {
    let known = MAIN_WINDOW.load(std::sync::atomic::Ordering::Relaxed);
    own_window(
        title,
        (title == crate::MAIN_TITLE && known != 0).then_some(known as HWND),
    )
}

/// `known` while that window exists, else the top-level window of this process titled `title`.
pub(super) fn own_window(title: &str, known: Option<HWND>) -> Option<HWND> {
    use windows_sys::Win32::UI::WindowsAndMessaging::IsWindow;
    // SAFETY: a plain query; any value is accepted as the handle.
    if let Some(h) = known.filter(|h| unsafe { IsWindow(*h) } != 0) {
        return Some(h);
    }
    let title: Vec<u16> = title.encode_utf16().collect();
    let mut f = Find {
        pid: unsafe { GetCurrentProcessId() },
        buf: vec![0; title.len() + 2],
        title,
        found: std::ptr::null_mut(),
    };
    unsafe { EnumWindows(Some(enum_cb), &mut f as *mut Find as LPARAM) };
    (!f.found.is_null()).then_some(f.found)
}

/// Off-screen test runs (MDM_OFFSCREEN=1): every top-level window of ours gets WS_EX_NOACTIVATE,
/// so Windows never makes it the foreground window, whatever asks (a click, a dialog coming to
/// the front, an accessibility client): the user's keystrokes stay in their own window while
/// tests run. UI Automation still works on them (Select, Invoke, Toggle need no activation).
/// Cheap: only our own windows are touched, once each.
pub fn keep_from_foreground() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowThreadProcessId, IsWindowVisible, WS_EX_NOACTIVATE,
    };
    unsafe extern "system" fn cb(hwnd: HWND, lparam: LPARAM) -> windows_sys::core::BOOL {
        let pid = lparam as u32;
        let mut owner = 0u32;
        // SAFETY: plain queries and a style change on a window of this process.
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut owner);
            if owner == pid && IsWindowVisible(hwnd) != 0 {
                let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
                if ex & WS_EX_NOACTIVATE == 0 {
                    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, (ex | WS_EX_NOACTIVATE) as isize);
                }
            }
        }
        1
    }
    // SAFETY: the callback only reads its pid from lparam.
    unsafe {
        EnumWindows(Some(cb), GetCurrentProcessId() as LPARAM);
    }
}

/// Our window titled `title`, just shown (dialogs::keep_theme), in front of the other programs'
/// windows if Windows didn't make it the foreground window (#208). Windows gives a program the
/// foreground only when it may take it (the user's click is in it, or the browser's native host
/// passed its right on, `AllowSetForegroundWindow`, which the user's next input elsewhere ends).
/// Refused, the window is placed under the program in front, just above our own windows, and
/// stays there: `HWND_TOP` and `BringWindowToTop` don't move it either. So a window opened at the
/// browser's request (Download File Info, a progress window) could open behind the browser. A
/// round trip through topmost puts it above every window that isn't topmost, as Download
/// complete is (#77), without taking the keyboard from the program being typed in (a click on
/// it does). None while it isn't shown yet (ask again in the next frame); else whether it was
/// lifted.
pub fn come_forward(title: &str) -> Option<bool> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, IsIconic, IsWindowVisible, WS_EX_TOPMOST,
    };
    let hwnd = find_own_window(title)?;
    // SAFETY: plain queries about our own window.
    let (visible, foreground, topmost, minimized) = unsafe {
        (
            IsWindowVisible(hwnd) != 0,
            GetForegroundWindow() == hwnd,
            GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST != 0,
            IsIconic(hwnd) != 0,
        )
    };
    if !visible {
        return None;
    }
    let lift = lifted_when_shown(foreground, topmost, minimized);
    if lift {
        // SAFETY: our own window, on its thread.
        unsafe { above_others(hwnd) };
    }
    Some(lift)
}

/// Whether our window titled `title` exists and is not on the screen: hidden and not minimized
/// (a window whose owner is minimized counts: Windows hides it with its owner).
pub fn window_hidden(title: &str) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{IsIconic, IsWindowVisible};
    // SAFETY: plain queries about our own window.
    find_own_window(title).is_some_and(|h| unsafe { IsWindowVisible(h) == 0 && IsIconic(h) == 0 })
}

/// Our window titled `title` shown (it was hidden behind winit's back), not activated, and above
/// the other programs' windows ([`come_forward`]) unless off-screen.
pub fn show_hidden(title: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SW_SHOWNOACTIVATE, ShowWindow};
    if let Some(h) = find_own_window(title) {
        // SAFETY: our own window, on its thread.
        unsafe { ShowWindow(h, SW_SHOWNOACTIVATE) };
        if !crate::offscreen() {
            come_forward(title);
        }
    }
}

/// Whether a window just shown goes above the others ([`come_forward`]): it isn't the foreground
/// window, isn't minimized, and isn't topmost already (a question, a notice: lifting it would
/// end its topmost).
pub(super) fn lifted_when_shown(foreground: bool, topmost: bool, minimized: bool) -> bool {
    !foreground && !topmost && !minimized
}

/// Window `hwnd` above every window that isn't topmost, not activated, nothing else changed (its
/// size, place, frame and styles; its owner stays where it is).
///
/// # Safety
/// `hwnd` is a window of this thread.
pub(super) unsafe fn above_others(hwnd: HWND) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOOWNERZORDER, SWP_NOSIZE,
    };
    let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER;
    // SAFETY: (the caller's) our own window; z-order changes only.
    unsafe {
        SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, flags);
        SetWindowPos(hwnd, HWND_NOTOPMOST, 0, 0, 0, 0, flags);
    }
}

/// Whether a dialog asked for now is one of the main window's own (owned by it: kept above it, no
/// taskbar button of its own), as IDM's dialogs opened from its window are: the main window is on
/// the screen (shown, not minimized) and DDM is the program in front (one of its windows has the
/// foreground: the user opened the dialog from DDM). Asked as the dialog is asked for, before its
/// window exists.
///
/// Any other dialog is a window of its own: one that opens over another program (Download File
/// Info for the browser's download, Download video, Add torrent, Download complete) or while the
/// main window waits in the notification area or minimized. Owned by the main window, it would
/// bring that window up with it: Windows raises an owner with the window it owns as that one is
/// activated (shown with the foreground the browser's host passed on, or clicked), and hides it
/// with a minimized owner. So the main window stays where it was, as IDM's does.
pub fn dialog_owned_by_main() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, IsIconic, IsWindowVisible,
    };
    let main = MAIN_WINDOW.load(std::sync::atomic::Ordering::Relaxed) as HWND;
    if main.is_null() {
        return false;
    }
    // SAFETY: plain queries; a gone window answers 0.
    unsafe {
        let shown = IsWindowVisible(main) != 0 && IsIconic(main) == 0;
        let front = GetForegroundWindow();
        let mut pid = 0u32;
        if !front.is_null() {
            GetWindowThreadProcessId(front, &mut pid);
        }
        shown && !front.is_null() && pid == GetCurrentProcessId()
    }
}

/// Is this screen point (a saved window's corner, in points) on a monitor now? A window put
/// back where a since-removed screen was would open out of sight.
pub fn on_a_screen(x: i32, y: i32) -> bool {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONULL, MonitorFromPoint};
    // SAFETY: a plain query.
    !unsafe { MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONULL) }.is_null()
}

/// The work area (left, top, right, bottom) of the monitor nearest to screen point (x, y).
pub fn work_area(x: i32, y: i32) -> Option<[i32; 4]> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
    };
    unsafe {
        let mon = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
        let mut mi: MONITORINFO = zeroed();
        mi.cbSize = size_of::<MONITORINFO>() as u32;
        (GetMonitorInfoW(mon, &mut mi) != 0).then_some([
            mi.rcWork.left,
            mi.rcWork.top,
            mi.rcWork.right,
            mi.rcWork.bottom,
        ])
    }
}

#[cfg(test)]
mod find_own_window_tests {
    use super::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassW, WNDCLASSW,
    };

    /// Our windows are found by their whole title, however long (a progress window is named
    /// after its file, #540), and never by a title that only starts the same.
    #[test]
    fn a_window_is_found_by_its_whole_title_however_long() {
        let class: Vec<u16> = "mdm-find-own-window-test\0".encode_utf16().collect();
        // SAFETY: a class registered with the system's own procedure, and hidden windows of it
        // on this thread, destroyed below; the strings are NUL-terminated.
        unsafe {
            let mut wc: WNDCLASSW = zeroed();
            wc.lpfnWndProc = Some(DefWindowProcW);
            wc.lpszClassName = class.as_ptr();
            RegisterClassW(&wc);
            let make = |title: &str| {
                let t: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
                let h = CreateWindowExW(
                    0,
                    class.as_ptr(),
                    t.as_ptr(),
                    0,
                    -20000,
                    -20000,
                    100,
                    100,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                );
                assert!(!h.is_null());
                h
            };
            for len in [40, 255, 256, 300, 1000] {
                let head = format!("mdm find test {} {len} ", std::process::id());
                let title = format!("{head}{}", "x".repeat(len - head.len()));
                assert_eq!(title.encode_utf16().count(), len);
                // One title longer by a unit, made first: found first, were it taken for ours.
                let longer = make(&format!("{title}x"));
                let window = make(&title);
                assert_eq!(find_own_window(&title), Some(window), "{len} units");
                DestroyWindow(window);
                assert_eq!(find_own_window(&title), None, "{len} units, gone");
                assert_eq!(find_own_window(&format!("{title}x")), Some(longer));
                DestroyWindow(longer);
            }
        }
    }

    /// A hidden test window of this thread titled `title`, destroyed by the caller.
    fn test_window(title: &str) -> HWND {
        let class: Vec<u16> = "mdm-find-own-window-test-2\0".encode_utf16().collect();
        let t: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
        // SAFETY: a class registered with the system's own procedure (again: a no-op), and a
        // hidden window of it on this thread; the strings are NUL-terminated.
        unsafe {
            let mut wc: WNDCLASSW = zeroed();
            wc.lpfnWndProc = Some(DefWindowProcW);
            wc.lpszClassName = class.as_ptr();
            RegisterClassW(&wc);
            let h = CreateWindowExW(
                0,
                class.as_ptr(),
                t.as_ptr(),
                0,
                -20000,
                -20000,
                100,
                100,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            );
            assert!(!h.is_null());
            h
        }
    }

    /// #583: looking for our window doesn't wait for another thread of this process whose window
    /// isn't answering (busy, on a stalled disk): the titles are read without a message.
    #[test]
    fn a_busy_threads_window_does_not_hold_the_search() {
        let pid = std::process::id();
        let (made, window_made) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel::<()>();
        let busy = std::thread::spawn(move || {
            let h = test_window(&format!("mdm busy window {pid}"));
            made.send(()).unwrap();
            // Busy: no message is taken until the test is done.
            let _ = released.recv();
            // SAFETY: the window made above, on this thread.
            unsafe { DestroyWindow(h) };
        });
        window_made.recv().unwrap();
        let (answer, answered) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = answer.send(find_own_window(&format!("mdm no such window {pid}")).is_some());
        });
        let got = answered.recv_timeout(std::time::Duration::from_secs(3));
        release.send(()).unwrap();
        busy.join().unwrap();
        assert_eq!(
            got,
            Ok(false),
            "the search waited for the busy thread's window"
        );
    }

    /// #633: DWM is asked for a window's caption once per change, not each time it's re-asserted.
    #[test]
    fn the_caption_goes_to_dwm_only_when_it_changes() {
        use std::sync::atomic::Ordering::Relaxed;
        let title = format!("mdm caption test {}", std::process::id());
        let h = test_window(&title);
        let calls = || CAPTION_DWM_CALLS.load(Relaxed);
        let before = calls();
        for _ in 0..50 {
            assert!(caption_dark(&title, true));
        }
        assert_eq!(calls() - before, 1, "dark, asked once");
        for _ in 0..50 {
            assert!(caption_dark(&title, false));
        }
        assert_eq!(calls() - before, 2, "light, once more");
        assert!(caption_dark(&title, true));
        assert_eq!(calls() - before, 3, "dark again");
        // SAFETY: the window made above, on this thread.
        unsafe { DestroyWindow(h) };
        assert!(!caption_dark(&title, true), "gone: not found");
        assert_eq!(calls() - before, 3);
    }
}

#[cfg(test)]
mod come_forward_tests {
    use super::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, GW_HWNDNEXT, GetClientRect, GetTopWindow,
        GetWindow, GetWindowRect, RegisterClassW, WNDCLASSW, WS_EX_TOPMOST,
    };

    /// Lifted only when it didn't get the foreground, and never a topmost one (a question, a
    /// notice) or a minimized one.
    #[test]
    fn only_a_window_left_behind_is_lifted() {
        assert!(lifted_when_shown(false, false, false));
        assert!(!lifted_when_shown(true, false, false));
        assert!(!lifted_when_shown(false, true, false));
        assert!(!lifted_when_shown(false, false, true));
    }

    /// Where `hwnd` is in the z-order of the top-level windows (0: the top).
    fn z(hwnd: HWND) -> usize {
        // SAFETY: plain queries walking the desktop's top-level windows.
        unsafe {
            let mut w = GetTopWindow(std::ptr::null_mut());
            let mut i = 0;
            while !w.is_null() && w != hwnd {
                w = GetWindow(w, GW_HWNDNEXT);
                i += 1;
            }
            assert!(!w.is_null(), "not a top-level window");
            i
        }
    }

    /// Two real (hidden, off-screen) windows with winit's styles for the progress window, one
    /// framed by caption_less as on Windows 11: each in turn goes above the other, neither stays
    /// topmost, and nothing else of it changes (styles, place, size, client: the frame #208's
    /// first part made stays as it was).
    #[test]
    fn above_others_lifts_a_real_window_and_changes_nothing_else() {
        const WINIT_PROGRESS: u32 = 0x04CE_0000;
        const WINIT_EX: u32 = 0x0004_0100;
        let class: Vec<u16> = "mdm-come-forward-test\0".encode_utf16().collect();
        // SAFETY: a class registered with the system's own procedure, and hidden windows of it on
        // this thread, destroyed below; the strings are NUL-terminated; plain queries, and
        // z-order changes of our own windows.
        unsafe {
            let mut wc: WNDCLASSW = zeroed();
            wc.lpfnWndProc = Some(DefWindowProcW);
            wc.lpszClassName = class.as_ptr();
            RegisterClassW(&wc);
            let make = |n: u32| {
                let title: Vec<u16> = format!("mdm come-forward test {n} {}", std::process::id())
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                let h = CreateWindowExW(
                    WINIT_EX,
                    class.as_ptr(),
                    title.as_ptr(),
                    WINIT_PROGRESS,
                    -20000,
                    -20000,
                    548,
                    444,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                );
                assert!(!h.is_null());
                h
            };
            let (a, b) = (make(0), make(1));
            caption_less(a, Drawn::Fixed);
            let state = |h: HWND| {
                let (mut w, mut c): (RECT, RECT) = (zeroed(), zeroed());
                GetWindowRect(h, &mut w);
                GetClientRect(h, &mut c);
                (
                    GetWindowLongPtrW(h, GWL_STYLE) as u32,
                    GetWindowLongPtrW(h, GWL_EXSTYLE) as u32,
                    [w.left, w.top, w.right, w.bottom],
                    [c.right, c.bottom],
                )
            };
            for (up, other) in [(a, b), (b, a), (a, b)] {
                let before = state(up);
                above_others(up);
                assert!(z(up) < z(other), "lifted above the other window");
                let after = state(up);
                assert_eq!(after.1 & WS_EX_TOPMOST, 0, "left topmost");
                assert_eq!(before, after, "only its place in the z-order changes");
            }
            DestroyWindow(a);
            DestroyWindow(b);
        }
    }
}

#[cfg(test)]
mod main_window_tests {
    use super::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassW, WNDCLASSW,
    };

    /// #337: two windows under one title, the way a failed Direct3D start leaves them. The
    /// window the app was made for is the one found, whichever is on top; and once it is gone
    /// the search by title still answers.
    #[test]
    fn the_known_window_wins_over_its_namesake() {
        let class: Vec<u16> = "mdm-main-window-test ".encode_utf16().collect();
        let name = format!("mdm main window test {}", std::process::id());
        let title: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        // SAFETY: a class registered with the system's own procedure, and hidden windows of it
        // on this thread, destroyed below; the strings are NUL-terminated.
        unsafe {
            let mut wc: WNDCLASSW = zeroed();
            wc.lpfnWndProc = Some(DefWindowProcW);
            wc.lpszClassName = class.as_ptr();
            RegisterClassW(&wc);
            let make = || {
                let h = CreateWindowExW(
                    0,
                    class.as_ptr(),
                    title.as_ptr(),
                    0,
                    -20000,
                    -20000,
                    100,
                    100,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                );
                assert!(!h.is_null());
                h
            };
            let (a, b) = (make(), make());
            assert_eq!(own_window(&name, Some(a)), Some(a));
            assert_eq!(own_window(&name, Some(b)), Some(b));
            assert!(own_window(&name, None).is_some());
            DestroyWindow(b);
            assert_eq!(own_window(&name, Some(b)), Some(a), "gone: found by title");
            DestroyWindow(a);
            assert_eq!(own_window(&name, Some(a)), None);
        }
    }
}
