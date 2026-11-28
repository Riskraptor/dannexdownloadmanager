//! Dragging a finished file out of DDM to another program (OLE drag and drop), on the window's
//! thread.

use super::*;

/// A drag out of Download complete is under way ([`drag_file_out`]): one at a time, the mouse
/// holds only one.
pub(super) static DRAGGING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// When the last drag out ended.
pub(super) static DRAG_ENDED: std::sync::Mutex<Option<std::time::Instant>> =
    std::sync::Mutex::new(None);

/// A drag out is under way, or ended a moment ago. Its keys are its own: Escape cancels the
/// drag in OLE's loop, on the drag's thread, while the window (whose thread the keys go to)
/// sees them too, often after the drag already ended, and must not take Escape as Close.
pub fn dragging_out() -> bool {
    use std::sync::atomic::Ordering;
    DRAGGING.load(Ordering::Acquire)
        || DRAG_ENDED
            .lock()
            .ok()
            .and_then(|t| *t)
            .is_some_and(|t| t.elapsed() < std::time::Duration::from_millis(500))
}

/// Start dragging `file` out of our window (to Explorer, a mail, a chat…), like IDM's Download
/// complete grip. The drag preview is the file's shell icon. The window is the one holding the
/// mouse (the button is down on its grip: winit captured the mouse for it), so it is the right
/// one when several show the same title; else our window titled `title`, given in the interface
/// language, as it shows.
///
/// The drag runs on a thread of its own (#561): OLE's drag loop lasts as long as the button is
/// held, and each step calls the program under the pointer (its drop target, in its own process),
/// so on the window thread a busy or hung target, or a target that copies the file before it
/// answers the drop, froze every window until it answered. That thread shares the window
/// thread's input ([`with_shared_input`]): OLE reads the held button and takes the mouse from
/// there, as it did from the window thread.
pub fn drag_file_out(title: &str, file: &std::path::Path) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    let hwnd = own_capture()
        .or_else(|| find_own_window(title))
        .ok_or("window not found")? as isize;
    if DRAGGING.swap(true, Ordering::AcqRel) {
        return Ok(());
    }
    let file = file.to_path_buf();
    let started = with_shared_input("drag out", move |shared| {
        if !shared {
            tracing::warn!("drag out: the window's input isn't shared");
        }
        // Looked for here, beside the window (#743): a drive slow to answer doesn't hold it.
        let result = if file.exists() {
            drag_on_this_thread(hwnd as HWND, &file)
        } else {
            Ok(())
        };
        if let Ok(mut t) = DRAG_ENDED.lock() {
            *t = Some(std::time::Instant::now());
        }
        DRAGGING.store(false, Ordering::Release);
        // OLE's drag loop took the button's release, so the window never saw it: without it
        // the next press on the grip only ended the drag the window still thought was on, and
        // the grip worked every other time.
        release_button(hwnd as HWND);
        if let Err(e) = result {
            tracing::warn!("drag out: {e}");
        }
    });
    if started.is_err() {
        DRAGGING.store(false, Ordering::Release);
    }
    started
}

/// Tell `hwnd` the left button is up, at the pointer, as Windows would have done had OLE's
/// drag loop not taken the release. A move first: the pointer left the window during the drag,
/// and a release with no pointer position is dropped before it reaches the interface.
pub(super) fn release_button(hwnd: HWND) {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
    // The left button's flag in a mouse message's wParam (MK_LBUTTON, winuser.h).
    const MK_LBUTTON: usize = 0x0001;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetCursorPos, PostMessageW, WM_LBUTTONUP, WM_MOUSEMOVE,
    };
    let mut pt = POINT { x: 0, y: 0 };
    // SAFETY: reads into a local; `hwnd` is our own window (a closed one fails harmlessly).
    unsafe {
        GetCursorPos(&mut pt);
        ScreenToClient(hwnd, &mut pt);
    }
    let lparam = (((pt.y as u16 as u32) << 16) | pt.x as u16 as u32) as isize;
    // SAFETY: messages posted to our own window.
    unsafe {
        PostMessageW(hwnd, WM_MOUSEMOVE, MK_LBUTTON, lparam);
        PostMessageW(hwnd, WM_LBUTTONUP, 0, lparam);
    }
}

/// The drag itself, on the thread [`drag_file_out`] started (OLE initialised there).
pub(super) fn drag_on_this_thread(hwnd: HWND, file: &std::path::Path) -> Result<(), String> {
    let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let what = crate::fileicons::What::of(crate::fileicons::Kind::File(name));
    let preview = file_icon(&what, 32)
        .and_then(|icon| {
            let crate::fileicons::Native::Rgba { side, rgba, .. } = icon else {
                return None;
            };
            let img = image::RgbaImage::from_raw(side, side, rgba)?;
            let mut png = Vec::new();
            img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                .ok()?;
            Some(png)
        })
        .unwrap_or_default();
    drag::start_drag(
        &OwnWindow(hwnd),
        drag::DragItem::Files(vec![file.to_path_buf()]),
        drag::Image::Raw(preview),
        |_, _| {},
        drag::Options::default(),
    )
    .map_err(|e| e.to_string())
}

/// Run `work` on a new thread named `name`, with OLE (a single-threaded apartment) and the
/// calling thread's input state: the keys and mouse buttons held, the mouse capture and the
/// focus (AttachThreadInput), as a modal loop like OLE's drag needs when the button went down in
/// the caller's window. `work` is told whether the input is shared. Returns at once; the input
/// is detached and OLE closed when `work` ends. The caller has a message queue (a window thread).
pub(super) fn with_shared_input(
    name: &str,
    work: impl FnOnce(bool) + Send + 'static,
) -> Result<(), String> {
    use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows_sys::Win32::UI::WindowsAndMessaging::{MSG, PM_NOREMOVE, PeekMessageW};
    #[link(name = "ole32")]
    unsafe extern "system" {
        fn OleInitialize(reserved: *mut c_void) -> i32;
        fn OleUninitialize();
    }
    // SAFETY: a plain query.
    let caller = unsafe { GetCurrentThreadId() };
    std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(move || {
            // SAFETY: OLE for this new thread, closed below on the same thread when it took.
            let ole = unsafe { OleInitialize(std::ptr::null_mut()) } >= 0;
            // A thread is given its message queue by its first window call, and only threads
            // with one can share input: a look at the (empty) queue makes it.
            // SAFETY: a read into a local MSG; nothing is removed.
            let mut msg: MSG = unsafe { zeroed() };
            unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_NOREMOVE) };
            // SAFETY: plain calls with two live threads of this process; undone below.
            let me = unsafe { GetCurrentThreadId() };
            let attached = unsafe { AttachThreadInput(me, caller, 1) } != 0;
            work(attached);
            if attached {
                // SAFETY: as above.
                unsafe { AttachThreadInput(me, caller, 0) };
            }
            if ole {
                // SAFETY: matches the OleInitialize above, on this thread.
                unsafe { OleUninitialize() };
            }
        })
        .map(drop)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod drag_thread_tests {
    use super::*;

    /// The drag's thread (#561): another thread than the window's, which goes on at once, in a
    /// single-threaded apartment (OLE's drag needs one), sharing the window thread's input.
    #[test]
    fn the_drag_runs_beside_the_window_thread_with_its_input() {
        use windows_sys::Win32::System::Com::{APTTYPE, APTTYPEQUALIFIER, CoGetApartmentType};
        use windows_sys::Win32::System::Threading::GetCurrentThreadId;
        use windows_sys::Win32::UI::WindowsAndMessaging::{MSG, PM_NOREMOVE, PeekMessageW};
        // This test's thread stands for the window thread: it has a message queue.
        // SAFETY: a read into a local MSG; nothing is removed.
        let mut msg: MSG = unsafe { zeroed() };
        unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_NOREMOVE) };
        // SAFETY: a plain query.
        let window_thread = unsafe { GetCurrentThreadId() };
        let (tx, rx) = std::sync::mpsc::channel();
        let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
        with_shared_input("drag test", move |shared| {
            // Held until the caller has gone on: the work doesn't hold the caller up.
            go_rx.recv().unwrap();
            let mut kind: APTTYPE = 0;
            let mut qualifier: APTTYPEQUALIFIER = 0;
            // SAFETY: reads into two locals.
            let hr = unsafe { CoGetApartmentType(&mut kind, &mut qualifier) };
            // SAFETY: a plain query.
            let me = unsafe { GetCurrentThreadId() };
            tx.send((me, shared, hr, kind)).unwrap();
        })
        .unwrap();
        go_tx.send(()).unwrap();
        let (me, shared, hr, kind) = rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
        assert_ne!(me, window_thread);
        assert!(shared, "the input is shared with the window thread");
        assert_eq!(hr, 0);
        // APTTYPE_STA (0); the first apartment of the process would be APTTYPE_MAINSTA (3).
        assert!(
            kind == 0 || kind == 3,
            "a single-threaded apartment, not {kind}"
        );
    }
}
