//! The taskbar button's progress (ITaskbarList3).

use super::*;

// ---- taskbar progress (ITaskbarList3) -----------------------------------------------------
//
// windows-sys has no COM interface tables, so the three calls used are declared here in
// ITaskbarList3's vtable order: IUnknown (3), ITaskbarList (HrInit, AddTab, DeleteTab,
// ActivateTab, SetActiveAlt), ITaskbarList2 (MarkFullscreenWindow), ITaskbarList3
// (SetProgressValue, SetProgressState, …).

#[repr(C)]
pub(super) struct TaskbarVtbl {
    pub(super) _query_interface: usize,
    pub(super) _add_ref: usize,
    pub(super) _release: usize,
    pub(super) hr_init: unsafe extern "system" fn(*mut c_void) -> i32,
    pub(super) _add_tab: usize,
    pub(super) _delete_tab: usize,
    pub(super) _activate_tab: usize,
    pub(super) _set_active_alt: usize,
    pub(super) _mark_fullscreen_window: usize,
    pub(super) set_progress_value: unsafe extern "system" fn(*mut c_void, HWND, u64, u64) -> i32,
    pub(super) set_progress_state: unsafe extern "system" fn(*mut c_void, HWND, u32) -> i32,
}

/// The taskbar object (created on first use on the UI thread) and the last value shown.
pub(super) struct Taskbar {
    pub(super) obj: *mut *const TaskbarVtbl,
    pub(super) shown: Option<Option<u32>>,
}

thread_local! {
    pub(super) static TASKBAR: std::cell::RefCell<Option<Taskbar>> = const { std::cell::RefCell::new(None) };
}

/// The window's taskbar button shows `progress` (0..=1, the green bar), or nothing (None).
/// Only changes (in 1/1000 steps) reach the shell.
pub fn taskbar_progress(title: &str, progress: Option<f64>) {
    use windows_sys::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    };
    use windows_sys::core::GUID;
    const CLSID_TASKBARLIST: GUID = GUID::from_u128(0x56fdf344_fd6d_11d0_958a_006097c9a090);
    const IID_ITASKBARLIST3: GUID = GUID::from_u128(0xea1afb91_9e28_4b86_90e9_9e9f8a5eefaf);
    const TBPF_NOPROGRESS: u32 = 0;
    const TBPF_NORMAL: u32 = 2;
    let value = progress.map(|p| (p.clamp(0.0, 1.0) * 1000.0).round() as u32);
    TASKBAR.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            let mut obj: *mut c_void = std::ptr::null_mut();
            // SAFETY: COM on the UI thread (a second CoInitializeEx is harmless); the out
            // pointer receives an ITaskbarList3 on success, checked before use.
            unsafe {
                CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
                let hr = CoCreateInstance(
                    &CLSID_TASKBARLIST,
                    std::ptr::null_mut(),
                    CLSCTX_INPROC_SERVER,
                    &IID_ITASKBARLIST3,
                    &mut obj,
                );
                if hr < 0 || obj.is_null() {
                    return;
                }
                let tb = obj as *mut *const TaskbarVtbl;
                ((**tb).hr_init)(obj);
                *slot = Some(Taskbar {
                    obj: tb,
                    shown: None,
                });
            }
        }
        let Some(tb) = slot.as_mut() else { return };
        if tb.shown == Some(value) {
            return;
        }
        let Some(hwnd) = find_own_window(title) else {
            return;
        };
        // SAFETY: `obj` is a live ITaskbarList3 (never released: it lives as long as the UI
        // thread); the calls match the vtable above.
        unsafe {
            let this = tb.obj as *mut c_void;
            match value {
                Some(v) => {
                    ((**tb.obj).set_progress_state)(this, hwnd, TBPF_NORMAL);
                    ((**tb.obj).set_progress_value)(this, hwnd, u64::from(v), 1000);
                }
                None => {
                    ((**tb.obj).set_progress_state)(this, hwnd, TBPF_NOPROGRESS);
                }
            }
        }
        tb.shown = Some(value);
    });
}
