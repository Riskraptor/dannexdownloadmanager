//! The process and its threads: the installer's mutex, the crash handler and dumps, thread ids and
//! usage, the working set, other processes, the console of a command line, the OpenGL driver.

use super::*;

// ---- Crash reports (crash.rs, mdm-crash) -------------------------------------------------------

pub(super) static CRASH_HANDLER: std::sync::OnceLock<mdm_crash::crash_handler::CrashHandler> =
    std::sync::OnceLock::new();

/// A named mutex held while the app runs, so the installer can tell (Inno Setup's `AppMutex`)
/// and wait for `/exit` to finish before replacing or removing the program. The handle closes
/// with the process.
pub fn mark_running() {
    use windows_sys::Win32::System::Threading::CreateMutexW;
    let name: Vec<u16> = "DannexDownloadManager.Running\0".encode_utf16().collect();
    // SAFETY: NUL-terminated name; the handle is deliberately kept for the process's life.
    unsafe {
        CreateMutexW(std::ptr::null(), 0, name.as_ptr());
    }
}

/// Catch native crashes (access violations, stack overflows, … : Windows' structured exceptions)
/// and have `reporter`'s monitor dump this process when one comes. False when it can't attach.
pub fn attach_crash_handler(reporter: std::sync::Arc<mdm_crash::client::Reporter>) -> bool {
    use mdm_crash::crash_handler::{CrashEventResult, CrashHandler, make_crash_event};
    // SAFETY: the closure runs in the exception's context; it only sends the dump request over
    // the already-open socket (no allocation needed by minidumper's request).
    let event = unsafe { make_crash_event(move |cc| CrashEventResult::Handled(reporter.dump(cc))) };
    match CrashHandler::attach(event) {
        Ok(h) => CRASH_HANDLER.set(h).is_ok(),
        Err(e) => {
            tracing::warn!("crash handler not attached: {e}");
            false
        }
    }
}

/// Have the monitor dump this process now (a panic, a freeze): every thread, this one asking.
/// The process goes on.
pub fn dump_now() -> bool {
    CRASH_HANDLER.get().is_some_and(|h| {
        matches!(
            h.simulate_exception(None),
            mdm_crash::crash_handler::CrashEventResult::Handled(true)
        )
    })
}

/// This thread's id as Windows (and so the dump) knows it.
pub fn thread_id() -> u32 {
    // SAFETY: a plain query.
    unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() }
}

/// Give this process's working set back to the system while the window is hidden (#72): the
/// pages go to the standby list, still in RAM, and come back on first touch (soft faults, no
/// disk). What Task Manager's "Memory" column shows for a tray app is this working set.
pub fn trim_working_set() {
    use windows_sys::Win32::System::ProcessStatus::EmptyWorkingSet;
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    // SAFETY: a plain call on this process's own pseudo-handle.
    let ok = unsafe { EmptyWorkingSet(GetCurrentProcess()) } != 0;
    if !ok {
        tracing::debug!("EmptyWorkingSet failed");
    }
}

/// Thread `tid`'s processor time so far, and this process's page faults so far (soft ones
/// included: Windows counts no hard faults per process here): crash.rs's freeze notes.
pub fn thread_usage(tid: u32) -> Option<crate::crash::Usage> {
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME};
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetThreadTimes, OpenThread, THREAD_QUERY_LIMITED_INFORMATION,
    };
    // 100 ns units to ms.
    let ms = |t: FILETIME| (((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64) / 10_000;
    // SAFETY: the handle is checked and closed; the out-parameters are locals of the right size.
    unsafe {
        let h = OpenThread(THREAD_QUERY_LIMITED_INFORMATION, 0, tid);
        if h.is_null() {
            return None;
        }
        let z = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let (mut created, mut exited, mut kernel, mut user) = (z, z, z, z);
        let ok = GetThreadTimes(h, &mut created, &mut exited, &mut kernel, &mut user) != 0;
        CloseHandle(h);
        if !ok {
            return None;
        }
        let mut m: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        m.cb = size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        let faults = (GetProcessMemoryInfo(GetCurrentProcess(), &mut m, m.cb) != 0)
            .then_some(m.PageFaultCount as u64);
        Some(crate::crash::Usage {
            cpu_ms: ms(kernel) + ms(user),
            faults,
        })
    }
}

/// Is process `pid` still running?
pub fn process_alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
    };
    // SAFETY: the handle is checked and closed.
    unsafe {
        let h = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if h.is_null() {
            return false;
        }
        let running = WaitForSingleObject(h, 0) == WAIT_TIMEOUT;
        CloseHandle(h);
        running
    }
}

/// The command line in this windowed program (main.rs, `dannexdownloadmanager-cli`'s
/// launcher started it): the console it was started from, for what it prints and reads and for
/// Ctrl+C. Input and output the launcher handed over stay as they are (a file, a pipe, the console
/// itself); only those it didn't have get the console's. With the launcher's process id, this
/// process ends when the launcher does first (killed): nothing else would wait for it.
pub fn cli_console(launcher: Option<u32>) {
    use windows_sys::Win32::Foundation::{
        GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE,
        STD_OUTPUT_HANDLE, SetStdHandle,
    };
    use windows_sys::Win32::System::Threading::{
        INFINITE, OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
    };
    let usable = |h: HANDLE| !h.is_null() && h != INVALID_HANDLE_VALUE;
    let ids = [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE];
    // SAFETY: plain queries and settings of this process's standard handles; the names are
    // NUL-terminated, and the handles opened are kept for the process's life.
    unsafe {
        let given = ids.map(|id| GetStdHandle(id));
        if AttachConsole(ATTACH_PARENT_PROCESS) != 0 {
            for (id, h) in ids.into_iter().zip(given) {
                if usable(h) {
                    SetStdHandle(id, h);
                } else if !usable(GetStdHandle(id)) {
                    let name = if id == STD_INPUT_HANDLE {
                        "CONIN$\0"
                    } else {
                        "CONOUT$\0"
                    };
                    let name: Vec<u16> = name.encode_utf16().collect();
                    let console = CreateFileW(
                        name.as_ptr(),
                        GENERIC_READ | GENERIC_WRITE,
                        FILE_SHARE_READ | FILE_SHARE_WRITE,
                        std::ptr::null(),
                        OPEN_EXISTING,
                        0,
                        std::ptr::null_mut(),
                    );
                    if usable(console) {
                        SetStdHandle(id, console);
                    }
                }
            }
        }
        let watched = launcher.map_or(std::ptr::null_mut(), |pid| {
            OpenProcess(PROCESS_SYNCHRONIZE, 0, pid)
        });
        if !watched.is_null() {
            // A handle isn't Send; its value is, and the handle stays open.
            let watched = watched as usize;
            std::thread::spawn(move || {
                WaitForSingleObject(watched as HANDLE, INFINITE);
                std::process::exit(1);
            });
        }
    }
}

/// The OpenGL driver in its own words, for the log and the crash reports (#280): its renderer
/// (the graphics card) and version, which on Windows carries the driver's build (Intel's
/// "4.6.0 - Build 31.0.101.…", NVIDIA's "4.6.0 NVIDIA 5xx.xx"). Linux and macOS: unix.rs.
pub fn gl_driver(gl: &eframe::glow::Context) -> String {
    use eframe::glow::{self, HasContext as _};
    // SAFETY: called while eframe makes the app, with its context current on this thread;
    // glGetString only reads the driver's strings.
    let (renderer, version) = unsafe {
        (
            gl.get_parameter_string(glow::RENDERER),
            gl.get_parameter_string(glow::VERSION),
        )
    };
    format!("{renderer} ({version})")
}

/// End process `pid` (a frozen DDM the user chose to close from the crash report).
pub fn end_process(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};
    // SAFETY: the handle is checked and closed.
    unsafe {
        let h = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if h.is_null() {
            return false;
        }
        let ok = TerminateProcess(h, 1) != 0;
        CloseHandle(h);
        ok
    }
}

/// The crash test's access violation (MDM_CRASH_TEST=1, `/show crash-test:crash`).
pub fn crash_now() {
    // SAFETY: none: a write through a null pointer, on purpose, to test the crash reports.
    unsafe { std::ptr::write_volatile(std::ptr::null_mut::<u32>(), 1) };
}
