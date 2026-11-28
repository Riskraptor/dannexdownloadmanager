use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, mpsc::Sender};

use windows_sys::Win32::Foundation::HWND;

type HResult = i32;
const S_OK: HResult = 0;
const E_NOINTERFACE: HResult = 0x8000_4002_u32 as i32;
const DROPEFFECT_NONE: u32 = 0;
const DROPEFFECT_COPY: u32 = 1;
const CF_UNICODETEXT: u16 = 13;
const CF_HDROP: u16 = 15;
const TYMED_HGLOBAL: u32 = 1;
const DVASPECT_CONTENT: u32 = 1;

#[repr(C)]
#[derive(PartialEq)]
struct Guid(u32, u16, u16, [u8; 8]);
const IID_IUNKNOWN: Guid = Guid(0, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
const IID_IDROPTARGET: Guid = Guid(0x0000_0122, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);

#[repr(C)]
struct PointL {
    x: i32,
    y: i32,
}

#[repr(C)]
struct FormatEtc {
    cf: u16,
    ptd: *mut c_void,
    aspect: u32,
    lindex: i32,
    tymed: u32,
}

#[repr(C)]
struct StgMedium {
    tymed: u32,
    handle: *mut c_void,
    unk_for_release: *mut c_void,
}

#[link(name = "ole32")]
unsafe extern "system" {
    fn OleInitialize(reserved: *mut c_void) -> HResult;
    fn RegisterDragDrop(hwnd: HWND, target: *mut c_void) -> HResult;
    fn RevokeDragDrop(hwnd: HWND) -> HResult;
    fn ReleaseStgMedium(medium: *mut StgMedium);
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GlobalLock(h: *mut c_void) -> *mut c_void;
    fn GlobalUnlock(h: *mut c_void) -> i32;
    fn GlobalSize(h: *mut c_void) -> usize;
}
#[link(name = "user32")]
unsafe extern "system" {
    fn RegisterClipboardFormatW(name: *const u16) -> u32;
}
#[link(name = "shell32")]
unsafe extern "system" {
    fn DragQueryFileW(hdrop: *mut c_void, i: u32, buf: *mut u16, cch: u32) -> u32;
}

/// IDataObject's vtable up to GetData (index 3), all we call.
#[repr(C)]
struct DataObjectVtbl {
    _query_interface: usize,
    _add_ref: usize,
    _release: usize,
    get_data: unsafe extern "system" fn(*mut c_void, *const FormatEtc, *mut StgMedium) -> HResult,
}

#[repr(C)]
struct Vtbl {
    query_interface:
        unsafe extern "system" fn(*mut Target, *const Guid, *mut *mut c_void) -> HResult,
    add_ref: unsafe extern "system" fn(*mut Target) -> u32,
    release: unsafe extern "system" fn(*mut Target) -> u32,
    drag_enter:
        unsafe extern "system" fn(*mut Target, *mut c_void, u32, PointL, *mut u32) -> HResult,
    drag_over: unsafe extern "system" fn(*mut Target, u32, PointL, *mut u32) -> HResult,
    drag_leave: unsafe extern "system" fn(*mut Target) -> HResult,
    drop: unsafe extern "system" fn(*mut Target, *mut c_void, u32, PointL, *mut u32) -> HResult,
}

#[repr(C)]
struct Target {
    vtbl: &'static Vtbl,
    refs: AtomicU32,
    tx: Sender<String>,
    over: Arc<AtomicBool>,
    /// What the current drag carries (decided at DragEnter).
    accepts: AtomicBool,
    /// Woken at enter, leave and drop: OLE's drag loop sends the window no input, so
    /// egui wouldn't draw the highlight, or take the drop, until something else came.
    wake: egui::Context,
}

static VTBL: Vtbl = Vtbl {
    query_interface,
    add_ref,
    release,
    drag_enter,
    drag_over,
    drag_leave,
    drop,
};

unsafe extern "system" fn query_interface(
    this: *mut Target,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> HResult {
    // SAFETY: COM calls with valid pointers.
    unsafe {
        if *iid == IID_IUNKNOWN || *iid == IID_IDROPTARGET {
            add_ref(this);
            *out = this.cast();
            S_OK
        } else {
            *out = std::ptr::null_mut();
            E_NOINTERFACE
        }
    }
}

unsafe extern "system" fn add_ref(this: *mut Target) -> u32 {
    // SAFETY: `this` is a live Target (COM reference counting).
    unsafe { (*this).refs.fetch_add(1, Ordering::AcqRel) + 1 }
}

unsafe extern "system" fn release(this: *mut Target) -> u32 {
    // SAFETY: the last release frees the Box made in `Registration::new`.
    unsafe {
        let n = (*this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
        if n == 0 {
            std::mem::drop(Box::from_raw(this));
        }
        n
    }
}

unsafe extern "system" fn drag_enter(
    this: *mut Target,
    data: *mut c_void,
    _keys: u32,
    _pt: PointL,
    effect: *mut u32,
) -> HResult {
    // SAFETY: valid for the call.
    unsafe {
        let ok = !read(data).is_empty();
        (*this).accepts.store(ok, Ordering::Relaxed);
        (*this).over.store(ok, Ordering::Relaxed);
        (*this).wake.request_repaint();
        *effect = if ok { DROPEFFECT_COPY } else { DROPEFFECT_NONE };
    }
    S_OK
}

unsafe extern "system" fn drag_over(
    this: *mut Target,
    _keys: u32,
    _pt: PointL,
    effect: *mut u32,
) -> HResult {
    // SAFETY: valid for the call.
    unsafe {
        *effect = if (*this).accepts.load(Ordering::Relaxed) {
            DROPEFFECT_COPY
        } else {
            DROPEFFECT_NONE
        };
    }
    S_OK
}

unsafe extern "system" fn drag_leave(this: *mut Target) -> HResult {
    // SAFETY: valid for the call.
    unsafe {
        (*this).over.store(false, Ordering::Relaxed);
        (*this).wake.request_repaint();
    }
    S_OK
}

unsafe extern "system" fn drop(
    this: *mut Target,
    data: *mut c_void,
    _keys: u32,
    _pt: PointL,
    effect: *mut u32,
) -> HResult {
    // SAFETY: valid for the call.
    unsafe {
        (*this).over.store(false, Ordering::Relaxed);
        let found = read(data);
        *effect = if found.is_empty() {
            DROPEFFECT_NONE
        } else {
            DROPEFFECT_COPY
        };
        for s in found {
            let _ = (*this).tx.send(s);
        }
        (*this).wake.request_repaint();
    }
    S_OK
}

/// The addresses (or file paths) a drag carries: a link, else text that holds addresses,
/// else dropped files.
unsafe fn read(data: *mut c_void) -> Vec<String> {
    // SAFETY: `data` is the IDataObject OLE passed in.
    unsafe {
        let name: Vec<u16> = "UniformResourceLocatorW\0".encode_utf16().collect();
        let url_format = RegisterClipboardFormatW(name.as_ptr()) as u16;
        for cf in [url_format, CF_UNICODETEXT] {
            if let Some(text) = get_text(data, cf) {
                let urls: Vec<String> = text
                    .split_whitespace()
                    .filter(|w| {
                        let w = w.to_ascii_lowercase();
                        w.starts_with("http://")
                            || w.starts_with("https://")
                            || w.starts_with("ftp://")
                            || w.starts_with("ftps://")
                            || w.starts_with("magnet:")
                    })
                    .map(str::to_owned)
                    .collect();
                if !urls.is_empty() {
                    return urls;
                }
            }
        }
        files(data)
    }
}

unsafe fn medium(data: *mut c_void, cf: u16) -> Option<StgMedium> {
    // SAFETY: IDataObject::GetData through its vtable.
    unsafe {
        let vtbl = *(data as *const *const DataObjectVtbl);
        let fmt = FormatEtc {
            cf,
            ptd: std::ptr::null_mut(),
            aspect: DVASPECT_CONTENT,
            lindex: -1,
            tymed: TYMED_HGLOBAL,
        };
        let mut m = StgMedium {
            tymed: 0,
            handle: std::ptr::null_mut(),
            unk_for_release: std::ptr::null_mut(),
        };
        (((*vtbl).get_data)(data, &fmt, &mut m) == S_OK && !m.handle.is_null()).then_some(m)
    }
}

unsafe fn get_text(data: *mut c_void, cf: u16) -> Option<String> {
    // SAFETY: a locked HGLOBAL of UTF-16, read within its size, then released.
    unsafe {
        let mut m = medium(data, cf)?;
        let p = GlobalLock(m.handle) as *const u16;
        let text = (!p.is_null()).then(|| {
            let n = GlobalSize(m.handle) / 2;
            let s = std::slice::from_raw_parts(p, n);
            let end = s.iter().position(|c| *c == 0).unwrap_or(n);
            String::from_utf16_lossy(&s[..end])
        });
        GlobalUnlock(m.handle);
        ReleaseStgMedium(&mut m);
        text
    }
}

unsafe fn files(data: *mut c_void) -> Vec<String> {
    // SAFETY: an HDROP medium, queried with DragQueryFileW, then released.
    unsafe {
        let Some(mut m) = medium(data, CF_HDROP) else {
            return Vec::new();
        };
        let n = DragQueryFileW(m.handle, u32::MAX, std::ptr::null_mut(), 0);
        let mut out = Vec::new();
        for i in 0..n {
            let len = DragQueryFileW(m.handle, i, std::ptr::null_mut(), 0);
            let mut buf = vec![0u16; len as usize + 1];
            DragQueryFileW(m.handle, i, buf.as_mut_ptr(), len + 1);
            out.push(String::from_utf16_lossy(&buf[..len as usize]));
        }
        ReleaseStgMedium(&mut m);
        out
    }
}

/// Our target registered on the drop window; revoked when dropped.
pub struct Registration {
    hwnd: HWND,
    target: *mut Target,
}

impl Registration {
    /// None until the window exists (tried again next frame).
    pub fn new(
        title: &str,
        tx: Sender<String>,
        over: Arc<AtomicBool>,
        wake: &egui::Context,
    ) -> Option<Self> {
        let hwnd = crate::win32::find_own_window(title)?;
        let target = Box::into_raw(Box::new(Target {
            vtbl: &VTBL,
            refs: AtomicU32::new(1),
            tx,
            over,
            accepts: AtomicBool::new(false),
            wake: wake.clone(),
        }));
        // SAFETY: OLE on the UI thread (winit initialised it already: S_FALSE is fine);
        // any target winit registered goes first, then ours (RegisterDragDrop AddRefs it).
        unsafe {
            OleInitialize(std::ptr::null_mut());
            RevokeDragDrop(hwnd);
            if RegisterDragDrop(hwnd, target.cast()) != S_OK {
                release(target);
                return None;
            }
        }
        Some(Registration { hwnd, target })
    }

    /// Off-screen test copies only (MDM_TEST_DROP, droptarget.rs): `text` dropped on the
    /// window as a browser drops a link, without a mouse: a data object holding it as
    /// "UniformResourceLocatorW" goes to the drop target OLE has registered for the window
    /// (DragEnter, then Drop), as OLE's drag loop hands it. False when the window holds no
    /// registration of ours.
    pub fn test_drop(&self, text: &str) -> bool {
        use windows_sys::Win32::UI::WindowsAndMessaging::GetPropW;
        // OLE keeps the target registered for a window in this property of it (in the
        // process that registered it: the target itself).
        let prop: Vec<u16> = "OleDropTargetInterface\0".encode_utf16().collect();
        // SAFETY: a property read on our own window.
        let registered = unsafe { GetPropW(self.hwnd, prop.as_ptr()) };
        if registered != self.target.cast() {
            tracing::warn!("test drop: the window holds no drop target of ours");
            return false;
        }
        let data = Box::into_raw(Box::new(TestData {
            vtbl: &TEST_DATA_VTBL,
            text: text.encode_utf16().chain(Some(0)).collect(),
        }));
        let target = registered as *mut Target;
        let mut effect = DROPEFFECT_COPY;
        // SAFETY: our live target, called through its vtable as OLE does, with a data
        // object that lives until both calls return.
        unsafe {
            let pt = PointL { x: 0, y: 0 };
            ((*target).vtbl.drag_enter)(target, data.cast(), 0, pt, &mut effect);
            let pt = PointL { x: 0, y: 0 };
            ((*target).vtbl.drop)(target, data.cast(), 0, pt, &mut effect);
            std::mem::drop(Box::from_raw(data));
        }
        tracing::info!("test drop: {text} (effect {effect})");
        effect == DROPEFFECT_COPY
    }
}

/// The data object of [`Registration::test_drop`]: its text as a link (or plain text), in
/// memory the receiver frees (ReleaseStgMedium).
#[repr(C)]
struct TestData {
    vtbl: &'static TestDataVtbl,
    text: Vec<u16>,
}

/// IDataObject's vtable up to GetData, all the receiver calls.
#[repr(C)]
struct TestDataVtbl {
    query_interface:
        unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> HResult,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    get_data: unsafe extern "system" fn(*mut TestData, *const FormatEtc, *mut StgMedium) -> HResult,
}

static TEST_DATA_VTBL: TestDataVtbl = TestDataVtbl {
    query_interface: test_data_query,
    add_ref: test_data_ref,
    release: test_data_ref,
    get_data: test_data_get,
};

unsafe extern "system" fn test_data_query(
    _this: *mut c_void,
    _iid: *const Guid,
    out: *mut *mut c_void,
) -> HResult {
    // SAFETY: `out` is the caller's.
    unsafe { *out = std::ptr::null_mut() };
    E_NOINTERFACE
}

/// Owned by test_drop for the calls: no counting.
unsafe extern "system" fn test_data_ref(_this: *mut c_void) -> u32 {
    1
}

unsafe extern "system" fn test_data_get(
    this: *mut TestData,
    fmt: *const FormatEtc,
    out: *mut StgMedium,
) -> HResult {
    const DV_E_FORMATETC: HResult = 0x8004_0064_u32 as i32;
    const GMEM_MOVEABLE: u32 = 2;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalAlloc(flags: u32, bytes: usize) -> *mut c_void;
    }
    // SAFETY: valid pointers for the call; the memory is filled within its size and handed
    // to the caller, who releases it.
    unsafe {
        let name: Vec<u16> = "UniformResourceLocatorW\0".encode_utf16().collect();
        let url_format = RegisterClipboardFormatW(name.as_ptr()) as u16;
        if (*fmt).cf != url_format && (*fmt).cf != CF_UNICODETEXT {
            return DV_E_FORMATETC;
        }
        let text = &(*this).text;
        let h = GlobalAlloc(GMEM_MOVEABLE, text.len() * 2);
        let p = GlobalLock(h) as *mut u16;
        if p.is_null() {
            return DV_E_FORMATETC;
        }
        std::ptr::copy_nonoverlapping(text.as_ptr(), p, text.len());
        GlobalUnlock(h);
        *out = StgMedium {
            tymed: TYMED_HGLOBAL,
            handle: h,
            unk_for_release: std::ptr::null_mut(),
        };
    }
    S_OK
}

impl Drop for Registration {
    fn drop(&mut self) {
        // SAFETY: revoke OLE's reference, then ours.
        unsafe {
            RevokeDragDrop(self.hwnd);
            release(self.target);
        }
    }
}
