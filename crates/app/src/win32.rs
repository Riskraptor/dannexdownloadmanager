//! The only Windows FFI in Dannex Download Manager (D-008): IDM's dialog frame for our dialog windows, and the
//! shell's own small file-type icons (the ones Explorer and IDM show — Windows' artwork, not IDM's),
//! and the drop target's OLE `IDropTarget` (`ole`).
//!
//! Its parts are the modules below, each named for what it asks of Windows (`frame`, `tray`,
//! `registry`, `keychain`…); the rest of the app calls them as `win32::…`, through the
//! re-exports here. Only this module tree may use `unsafe` (the `allow` below covers it whole).
#![allow(unsafe_code)]

use std::ffi::c_void;
use std::mem::{size_of, zeroed};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits, ReleaseDC,
};
use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL;
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::UI::Shell::{
    DefSubclassProc, SHFILEINFOW, SHGFI_USEFILEATTRIBUTES, SHGetFileInfoW, SetWindowSubclass,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, EnumWindows, GWL_EXSTYLE, GWL_STYLE, GetIconInfo, GetWindowLongPtrW,
    GetWindowThreadProcessId, ICONINFO, InternalGetWindowText, NCCALCSIZE_PARAMS, SWP_FRAMECHANGED,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER, SetWindowLongPtrW, SetWindowPos, WM_NCCALCSIZE,
    WM_NCHITTEST, WS_EX_DLGMODALFRAME, WS_THICKFRAME,
};

mod caption;
mod drag_out;
mod frame;
mod icons;
mod keychain;
mod own_windows;
mod popup;
mod power;
mod process;
mod registry;
mod shell;
mod startup;
mod system;
mod taskbar;
mod text;
mod tray;

pub use self::caption::*;
pub use self::drag_out::*;
pub use self::frame::*;
pub use self::icons::*;
pub use self::keychain::*;
pub use self::own_windows::*;
pub use self::popup::*;
pub use self::power::*;
pub use self::process::*;
pub use self::registry::*;
pub use self::shell::*;
pub use self::startup::*;
pub use self::system::*;
pub use self::taskbar::*;
pub use self::text::*;
pub use self::tray::*;

/// The drop target's OLE side (`droptarget.rs`): a minimal `IDropTarget` that accepts links
/// (UniformResourceLocatorW), text and files.
pub mod ole;
