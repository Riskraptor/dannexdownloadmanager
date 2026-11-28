//! Icons: the windows' title bar and taskbar icons at each display's scale, and the shell's own file
//! type icons (the system image lists) turned into pictures.

use super::*;

/// The app's icons made for each size, kept for the program's life (a handful: one per size a
/// screen's scale asks for), by side in pixels.
pub(super) static WINDOW_ICONS: std::sync::Mutex<Vec<(u32, isize)>> =
    std::sync::Mutex::new(Vec::new());

/// The subclass that keeps a window's icons crisp ([`crisp_icons`]).
pub(super) const ICON_SUBCLASS: usize = 0x1c0;

/// The start of the shell's `IImageList` vtable (commoncontrols.h), as far as `GetIcon`. The list
/// is called through it rather than comctl32's `ImageList_GetIcon`, which needs the version 6
/// controls to read the shell's lists.
#[repr(C)]
pub(super) struct ImageListVtbl {
    pub(super) _query_interface: usize,
    pub(super) _add_ref: usize,
    pub(super) release: unsafe extern "system" fn(*mut c_void) -> u32,
    // Add, ReplaceIcon, SetOverlayImage, Replace, AddMasked, Draw, Remove.
    pub(super) _before_get_icon: [usize; 7],
    pub(super) get_icon: unsafe extern "system" fn(
        *mut c_void,
        i32,
        u32,
        *mut windows_sys::Win32::UI::WindowsAndMessaging::HICON,
    ) -> windows_sys::core::HRESULT,
}

/// IID_IImageList, {46EB5926-582E-4017-9FDF-E8998DAA0950}.
pub(super) const IID_IMAGE_LIST: windows_sys::core::GUID = windows_sys::core::GUID {
    data1: 0x46EB_5926,
    data2: 0x582E,
    data3: 0x4017,
    data4: [0x9F, 0xDF, 0xE8, 0x99, 0x8D, 0xAA, 0x09, 0x50],
};

/// An icon `side` pixels square from unpremultiplied RGBA rows, its alpha the icon's own (the
/// AND mask all clear). None when Windows refuses it. The caller destroys it.
pub(super) fn icon_from_rgba(
    rgba: &[u8],
    side: u32,
) -> Option<windows_sys::Win32::UI::WindowsAndMessaging::HICON> {
    use windows_sys::Win32::UI::WindowsAndMessaging::CreateIcon;
    if side == 0 || side > 1024 || rgba.len() != (side * side * 4) as usize {
        return None;
    }
    let bgra: Vec<u8> = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|&[r, g, b, a]| [b, g, r, a])
        .collect();
    // One bit a pixel, each row whole 16-bit words: all clear, the alpha decides.
    let mask = vec![0u8; (side.div_ceil(16) * 2 * side) as usize];
    // SAFETY: both buffers hold `side` rows of `side` pixels in the formats stated (32 bits a
    // pixel, 1 bit a pixel in words), and live for the call.
    let icon = unsafe {
        CreateIcon(
            std::ptr::null_mut(),
            side as i32,
            side as i32,
            1,
            32,
            mask.as_ptr(),
            bgra.as_ptr(),
        )
    };
    (!icon.is_null()).then_some(icon)
}

/// The app icon `side` pixels square, made once ([`crate::logo::rgba`]: the drawing for that
/// size, or the next larger one scaled down smoothly); 0 when Windows refuses it.
pub(super) fn window_icon(side: u32) -> isize {
    let mut made = WINDOW_ICONS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((_, icon)) = made.iter().find(|(s, _)| *s == side) {
        return *icon;
    }
    let icon = icon_from_rgba(&crate::logo::rgba(side), side).map_or(0, |i| i as isize);
    if icon != 0 {
        made.push((side, icon));
    }
    icon
}

/// The window's title bar (small) and taskbar / Alt+Tab (big) icons at its display's scale.
pub(super) fn window_icons(hwnd: HWND) -> (isize, isize) {
    use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
    use windows_sys::Win32::UI::WindowsAndMessaging::{SM_CXICON, SM_CXSMICON};
    // SAFETY: plain queries about our own window.
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    let small = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, dpi) }.clamp(16, 256) as u32;
    let big = unsafe { GetSystemMetricsForDpi(SM_CXICON, dpi) }.clamp(16, 256) as u32;
    (window_icon(small), window_icon(big))
}

/// Every icon a window is given (winit's, eframe's: the 256-pixel picture, which Windows scales
/// down to a blur) becomes the one made for its size at the window's scale; moved to a display
/// of another scale, the window takes that scale's.
pub(super) unsafe extern "system" fn icon_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        ICON_BIG, ICON_SMALL, SendMessageW, WM_DPICHANGED, WM_SETICON,
    };
    match msg {
        WM_SETICON if wparam == ICON_SMALL as usize || wparam == ICON_BIG as usize => {
            let (small, big) = window_icons(hwnd);
            let own = if wparam == ICON_SMALL as usize {
                small
            } else {
                big
            };
            let lparam = if own != 0 { own } else { lparam };
            // SAFETY: the subclass's own message, passed down the chain.
            unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
        }
        WM_DPICHANGED => {
            // SAFETY: as above; then our own window is given its icons again (this subclass
            // turns them into the new scale's).
            let r = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
            let (small, big) = window_icons(hwnd);
            unsafe {
                SendMessageW(hwnd, WM_SETICON, ICON_BIG as usize, big);
                SendMessageW(hwnd, WM_SETICON, ICON_SMALL as usize, small);
            }
            r
        }
        // SAFETY: as above.
        _ => unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) },
    }
}

/// Give `hwnd` (one of DDM's windows, on this thread) the app icon drawn for its title bar and its
/// taskbar button at its display's scale (16 and 32 pixels at 100 %, 24 and 48 at 150 %), and
/// keep them: any icon it is given later, and a move to a display of another scale, keep the
/// drawings made for the size. Without it Windows scales the 256-pixel picture winit and eframe
/// give every window down to 16 pixels, and the title bar's icon is a blur.
pub(super) fn crisp_icons_for(hwnd: HWND) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        ICON_BIG, ICON_SMALL, SendMessageW, WM_SETICON,
    };
    let (small, big) = window_icons(hwnd);
    // SAFETY: our own window, on its own thread; the subclass id is ours alone, and setting it
    // again changes nothing.
    unsafe {
        SetWindowSubclass(hwnd, Some(icon_proc), ICON_SUBCLASS, 0);
        if big != 0 {
            SendMessageW(hwnd, WM_SETICON, ICON_BIG as usize, big);
        }
        if small != 0 {
            SendMessageW(hwnd, WM_SETICON, ICON_SMALL as usize, small);
        }
    }
}

/// [`crisp_icons_for`] the window of this process titled `title` (a dialog, a progress window);
/// false while it isn't there yet.
pub fn crisp_icons(title: &str) -> bool {
    match find_own_window(title) {
        Some(hwnd) => {
            crisp_icons_for(hwnd);
            true
        }
        None => false,
    }
}

/// The shell's icon for a file type or a folder (fileicons.rs), as Explorer shows it, from the
/// system image list that fits `px` pixels: small, large, extra large (16, 32, 48 at 100 %, more
/// with the system's scale), then jumbo (256) for anything bigger. The file need not exist. Run
/// on fileicons.rs's lookup thread: the first lookup of a type loads its shell handler.
///
/// One lookup at a time in the process (the drag picture asks from the UI thread): two threads
/// in the shell at once could miss an icon. A miss is tried once more (a handler's first load).
pub fn file_icon(what: &crate::fileicons::What, px: u32) -> Option<crate::fileicons::Native> {
    static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _held = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    file_icon_once(what, px).or_else(|| file_icon_once(what, px))
}

pub(super) fn file_icon_once(
    what: &crate::fileicons::What,
    px: u32,
) -> Option<crate::fileicons::Native> {
    use crate::fileicons::What;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_DIRECTORY;
    use windows_sys::Win32::UI::Shell::SHGFI_SYSICONINDEX;
    com_on_this_thread();
    let (name, attrs) = match what {
        What::FolderOpen => return list_icon(open_folder_index()?, px),
        What::Folder => ("folder".to_string(), FILE_ATTRIBUTE_DIRECTORY),
        What::File(Some(ext)) => (format!("file.{ext}"), FILE_ATTRIBUTE_NORMAL),
        What::File(None) => ("file".to_string(), FILE_ATTRIBUTE_NORMAL),
    };
    let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
    // SAFETY: `info` is a zeroed SHFILEINFOW of the stated size; the name is NUL-terminated;
    // with SHGFI_USEFILEATTRIBUTES the file need not exist. SYSICONINDEX returns no icon to free.
    let index = unsafe {
        let mut info: SHFILEINFOW = zeroed();
        let ok = SHGetFileInfoW(
            wide.as_ptr(),
            attrs,
            &mut info,
            size_of::<SHFILEINFOW>() as u32,
            SHGFI_SYSICONINDEX | SHGFI_USEFILEATTRIBUTES,
        );
        if ok == 0 {
            return None;
        }
        info.iIcon
    };
    list_icon(index, px)
}

/// The system image lists' icon `index` at the list that fits `px` pixels (see `file_icon`).
pub(super) fn list_icon(index: i32, px: u32) -> Option<crate::fileicons::Native> {
    use windows_sys::Win32::UI::Shell::{SHIL_EXTRALARGE, SHIL_JUMBO, SHIL_LARGE, SHIL_SMALL};
    let mut got: Option<(u32, Vec<u8>)> = None;
    for list in [SHIL_SMALL, SHIL_LARGE, SHIL_EXTRALARGE, SHIL_JUMBO] {
        if got.as_ref().is_some_and(|(side, _)| *side >= px) {
            break;
        }
        let Some((side, rgba)) = image_list_icon(list, index) else {
            continue;
        };
        // A type with no picture that big gets its smaller one in the jumbo list's top-left
        // corner: keep the smaller one then.
        if list == SHIL_JUMBO
            && let Some((prev, _)) = &got
            && drawn_within(&rgba, side, *prev)
        {
            break;
        }
        got = Some((side, rgba));
    }
    let (side, rgba) = got?;
    Some(crate::fileicons::Native::Rgba {
        side,
        rgba,
        premultiplied: false,
    })
}

/// Whether every visible pixel of the `side`-square `rgba` lies in its top-left `within` square.
pub(super) fn drawn_within(rgba: &[u8], side: u32, within: u32) -> bool {
    rgba.as_chunks::<4>().0.iter().enumerate().all(|(i, c)| {
        let (x, y) = (i as u32 % side, i as u32 / side);
        c[3] == 0 || (x < within && y < within)
    })
}

/// COM for this thread (apartment-threaded, as the shell's icon handlers expect), once.
pub(super) fn com_on_this_thread() {
    use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
    thread_local!(static DONE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) });
    DONE.with(|done| {
        if !done.replace(true) {
            // SAFETY: no reserved pointer; the thread keeps COM for its lifetime.
            unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) };
        }
    });
}

/// Icon `index` of the system image list `list` (SHIL_*), as (side, RGBA).
pub(super) fn image_list_icon(list: u32, index: i32) -> Option<(u32, Vec<u8>)> {
    use windows_sys::Win32::UI::Controls::ILD_TRANSPARENT;
    use windows_sys::Win32::UI::Shell::SHGetImageList;
    // SAFETY: SHGetImageList hands out an IImageList with a reference for us, released below; the
    // vtable is read only as far as GetIcon, whose icon hicon_rgba takes and destroys.
    unsafe {
        let mut list_ptr: *mut c_void = std::ptr::null_mut();
        if SHGetImageList(list as i32, &IID_IMAGE_LIST, &mut list_ptr) < 0 || list_ptr.is_null() {
            return None;
        }
        let vtbl = &**(list_ptr as *const *const ImageListVtbl);
        let mut icon = std::ptr::null_mut();
        let hr = (vtbl.get_icon)(list_ptr, index, ILD_TRANSPARENT, &mut icon);
        (vtbl.release)(list_ptr);
        if hr < 0 || icon.is_null() {
            return None;
        }
        hicon_rgba(icon)
    }
}

/// Windows' type name for a file name ("BIN File", "Compressed (zipped) Folder").
pub fn type_name(file_name: &str) -> Option<String> {
    use windows_sys::Win32::UI::Shell::SHGFI_TYPENAME;
    let name: Vec<u16> = file_name.encode_utf16().chain([0]).collect();
    // SAFETY: `info` is a zeroed SHFILEINFOW of the stated size; the name is NUL-terminated;
    // with SHGFI_USEFILEATTRIBUTES the file need not exist.
    unsafe {
        let mut info: SHFILEINFOW = zeroed();
        let ok = SHGetFileInfoW(
            name.as_ptr(),
            FILE_ATTRIBUTE_NORMAL,
            &mut info,
            size_of::<SHFILEINFOW>() as u32,
            SHGFI_TYPENAME | SHGFI_USEFILEATTRIBUTES,
        );
        let len = info.szTypeName.iter().position(|c| *c == 0).unwrap_or(0);
        (ok != 0 && len > 0).then(|| String::from_utf16_lossy(&info.szTypeName[..len]))
    }
}

/// `icon`'s pixels as (side, straight RGBA), at the size its bitmap has; the icon is destroyed.
/// An icon without alpha (an old one) takes its transparency from its mask.
pub(super) unsafe fn hicon_rgba(
    icon: windows_sys::Win32::UI::WindowsAndMessaging::HICON,
) -> Option<(u32, Vec<u8>)> {
    use windows_sys::Win32::Graphics::Gdi::{BITMAP, GetObjectW};
    unsafe {
        let mut ii: ICONINFO = zeroed();
        let got = GetIconInfo(icon, &mut ii) != 0;
        let mut out = None;
        if got && !ii.hbmColor.is_null() {
            let mut bm: BITMAP = zeroed();
            let have = GetObjectW(
                ii.hbmColor as _,
                size_of::<BITMAP>() as i32,
                &mut bm as *mut BITMAP as *mut c_void,
            ) != 0;
            let side = bm.bmWidth;
            if have
                && side > 0
                && side == bm.bmHeight.abs()
                && side <= 1024
                && let Some(mut px) = dib_rgba(ii.hbmColor, side)
            {
                if !px.as_chunks::<4>().0.iter().any(|c| c[3] != 0) {
                    // The mask: white (set) where the icon is transparent.
                    let mask = (!ii.hbmMask.is_null())
                        .then(|| dib_rgba(ii.hbmMask, side))
                        .flatten();
                    for (i, c) in px.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                        c[3] = match &mask {
                            Some(m) if m[i * 4] != 0 => 0,
                            _ => 255,
                        };
                    }
                }
                out = Some((side as u32, px));
            }
        }
        if got {
            DeleteObject(ii.hbmColor as _);
            DeleteObject(ii.hbmMask as _);
        }
        DestroyIcon(icon);
        out
    }
}

/// The first `side` × `side` pixels of `bitmap`, top-down, as RGBA (32 bits per pixel).
pub(super) unsafe fn dib_rgba(
    bitmap: windows_sys::Win32::Graphics::Gdi::HBITMAP,
    side: i32,
) -> Option<Vec<u8>> {
    unsafe {
        let mut bmi: BITMAPINFO = zeroed();
        bmi.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: side,
            biHeight: -side, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..zeroed()
        };
        let mut px = vec![0u8; (side * side * 4) as usize];
        let dc = GetDC(std::ptr::null_mut());
        let lines = GetDIBits(
            dc,
            bitmap,
            0,
            side as u32,
            px.as_mut_ptr() as *mut c_void,
            &mut bmi,
            DIB_RGB_COLORS,
        );
        ReleaseDC(std::ptr::null_mut(), dc);
        (lines == side).then(|| {
            for c in px.as_chunks_mut::<4>().0 {
                c.swap(0, 2); // BGRA → RGBA
            }
            px
        })
    }
}

/// Windows' own "open folder" (Download complete's, as IDM shows), as its system image list index.
pub(super) fn open_folder_index() -> Option<i32> {
    use windows_sys::Win32::UI::Shell::{
        SHGSI_SYSICONINDEX, SHGetStockIconInfo, SHSTOCKICONINFO, SIID_FOLDEROPEN,
    };
    // SAFETY: `info` is zeroed with its size set; SYSICONINDEX returns no icon to free.
    unsafe {
        let mut info: SHSTOCKICONINFO = zeroed();
        info.cbSize = size_of::<SHSTOCKICONINFO>() as u32;
        (SHGetStockIconInfo(SIID_FOLDEROPEN, SHGSI_SYSICONINDEX, &mut info) == 0)
            .then_some(info.iSysImageIndex)
    }
}
