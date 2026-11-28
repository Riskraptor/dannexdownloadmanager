//! Power: waking the computer for the Scheduler (wake timers), keeping it awake while downloads
//! run, and the power options.

use super::*;

// ---- waking the computer for a queue (#142) -----------------------------------------------

/// Windows' "Allow wake timers" power option for the active plan (0 off, 1 on, 2 important
/// wake timers only), on mains power (`ac`) and on battery (`dc`), and the power source now.
pub struct WakeTimers {
    pub ac: u32,
    pub dc: u32,
    pub on_battery: bool,
    pub has_battery: bool,
}

/// A change to the wake timer, and where its result goes.
pub(super) struct WakeCmd {
    /// When (seconds since 1970, UTC) and for which queue; None cancels.
    pub(super) at: Option<(i64, String)>,
    pub(super) reply: std::sync::mpsc::Sender<Result<(), String>>,
}

/// Power Options → Sleep → Allow wake timers, as `powercfg /q SCHEME_CURRENT SUB_SLEEP
/// RTCWAKE` shows it. Read only; a setting that can't be read counts as on (nothing to warn of).
pub fn wake_timers() -> WakeTimers {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::System::Power::{
        GetSystemPowerStatus, PowerGetActiveScheme, PowerReadACValueIndex, PowerReadDCValueIndex,
        SYSTEM_POWER_STATUS,
    };
    use windows_sys::core::GUID;
    const SUB_SLEEP: GUID = GUID::from_u128(0x238c9fa8_0aad_41ed_83f4_97be242c8f20);
    const RTCWAKE: GUID = GUID::from_u128(0xbd3b718a_0680_4d9d_8ab2_e1d2b4ac806d);
    let (mut ac, mut dc) = (1u32, 1u32);
    let mut scheme: *mut GUID = std::ptr::null_mut();
    // SAFETY: PowerGetActiveScheme allocates the GUID it returns (freed with LocalFree below);
    // the reads only write the u32 given.
    unsafe {
        if PowerGetActiveScheme(std::ptr::null_mut(), &mut scheme) == 0 && !scheme.is_null() {
            let mut v = 0u32;
            if PowerReadACValueIndex(std::ptr::null_mut(), scheme, &SUB_SLEEP, &RTCWAKE, &mut v)
                == 0
            {
                ac = v;
            }
            if PowerReadDCValueIndex(std::ptr::null_mut(), scheme, &SUB_SLEEP, &RTCWAKE, &mut v)
                == 0
            {
                dc = v;
            }
            LocalFree(scheme.cast());
        }
    }
    // SAFETY: plain data, zero is valid; a plain query into it.
    let mut st: SYSTEM_POWER_STATUS = unsafe { zeroed() };
    let ok = unsafe { GetSystemPowerStatus(&mut st) } != 0;
    WakeTimers {
        ac,
        dc,
        // ACLineStatus 0: on battery; BatteryFlag 128: no system battery, 255: unknown.
        on_battery: ok && st.ACLineStatus == 0,
        has_battery: ok && st.BatteryFlag != 128 && st.BatteryFlag != 255,
    }
}

/// Power Options' advanced settings, where "Allow wake timers" is (under Sleep). Never from an
/// off-screen test run: its windows belong to the person at the computer.
pub fn open_power_options() {
    if crate::offscreen() {
        return;
    }
    use std::os::windows::process::CommandExt;
    let _ = std::process::Command::new("control.exe")
        .arg("powercfg.cpl,,3")
        .creation_flags(0x0800_0000)
        .spawn();
}

/// Wake the computer at `at` (seconds since 1970, UTC; for the queue named), or cancel the wake
/// (None). A waitable timer set to resume the computer (SetWaitableTimer, fResume): it lives in
/// DDM's process (Windows cancels it if the thread that set it ends, so one thread of its own
/// keeps it), lists in `powercfg /waketimers`, and goes with DDM: nothing is left behind when
/// DDM quits or is uninstalled.
pub fn wake_at(at: Option<(i64, &str)>) -> Result<(), String> {
    use std::sync::{Mutex, OnceLock, mpsc};
    static THREAD: OnceLock<Mutex<mpsc::Sender<WakeCmd>>> = OnceLock::new();
    let tx = THREAD.get_or_init(|| {
        let (tx, rx) = mpsc::channel();
        let _ = std::thread::Builder::new()
            .name("ddm-wake".into())
            .spawn(move || wake_thread(rx));
        Mutex::new(tx)
    });
    let (reply, answer) = mpsc::channel();
    let cmd = WakeCmd {
        at: at.map(|(t, q)| (t, q.to_owned())),
        reply,
    };
    let sent = tx.lock().map(|tx| tx.send(cmd).is_ok()).unwrap_or(false);
    let lost = || "the wake timer's thread is gone".to_owned();
    if !sent {
        return Err(lost());
    }
    answer
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap_or_else(|_| Err(lost()))
}

pub(super) fn wake_thread(rx: std::sync::mpsc::Receiver<WakeCmd>) {
    use std::sync::mpsc::RecvTimeoutError;
    use windows_sys::Win32::Foundation::{ERROR_NOT_SUPPORTED, GetLastError, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{
        CancelWaitableTimer, CreateWaitableTimerExW, SetWaitableTimer, TIMER_ALL_ACCESS,
        WaitForSingleObject,
    };
    // A synchronization timer (auto-reset): the check below sees each firing once.
    // SAFETY: no name, default security; the handle is this thread's for the process's life.
    let timer =
        unsafe { CreateWaitableTimerExW(std::ptr::null(), std::ptr::null(), 0, TIMER_ALL_ACCESS) };
    let mut queue = String::new();
    loop {
        match rx.recv_timeout(std::time::Duration::from_secs(1)) {
            Ok(cmd) => {
                let result = if timer.is_null() {
                    // SAFETY: a plain query.
                    Err(format!("CreateWaitableTimerExW failed ({})", unsafe {
                        GetLastError()
                    }))
                } else if let Some((unix, q)) = cmd.at {
                    // FILETIME: 100 ns steps since 1601, UTC (positive: an absolute time).
                    let due: i64 = (unix + 11_644_473_600) * 10_000_000;
                    // SAFETY: our timer; no completion routine; fResume = TRUE. GetLastError
                    // right after, as the call's documentation asks.
                    let (ok, err) = unsafe {
                        let ok = SetWaitableTimer(timer, &due, 0, None, std::ptr::null(), 1) != 0;
                        (ok, GetLastError())
                    };
                    queue = q;
                    if !ok {
                        Err(format!("SetWaitableTimer failed ({err})"))
                    } else if err == ERROR_NOT_SUPPORTED {
                        // Set, but this computer can't be woken by it.
                        Err(crate::i18n::t(
                            // i18n: the Scheduler, under "Wake the computer to run this queue": Windows set the timer but can't wake this computer with it
                            "This computer can't be woken by a timer",
                        )
                        .to_owned())
                    } else {
                        Ok(())
                    }
                } else {
                    // SAFETY: our timer.
                    unsafe { CancelWaitableTimer(timer) };
                    Ok(())
                };
                let _ = cmd.reply.send(result);
            }
            Err(RecvTimeoutError::Timeout) => {
                // SAFETY: our timer; a zero wait only looks.
                if !timer.is_null() && unsafe { WaitForSingleObject(timer, 0) } == WAIT_OBJECT_0 {
                    tracing::info!("wake: the wake timer went off (queue {queue:?})");
                }
            }
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// Keep the computer from sleeping while `reason` is given (a power request, listed with its
/// reason in `powercfg /requests`); None lets it sleep again as its power plan says. False when
/// Windows refused.
pub fn keep_awake(reason: Option<&str>) -> bool {
    use std::sync::Mutex;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Power::{
        PowerClearRequest, PowerCreateRequest, PowerRequestSystemRequired, PowerSetRequest,
    };
    use windows_sys::Win32::System::Threading::{
        POWER_REQUEST_CONTEXT_SIMPLE_STRING, REASON_CONTEXT,
    };
    static REQUEST: Mutex<Option<isize>> = Mutex::new(None);
    let mut held = REQUEST.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(h) = held.take() {
        // SAFETY: the request made below, cleared and closed once.
        unsafe {
            PowerClearRequest(h as _, PowerRequestSystemRequired);
            CloseHandle(h as _);
        }
    }
    let Some(reason) = reason else {
        return true;
    };
    let mut text: Vec<u16> = reason.encode_utf16().chain(Some(0)).collect();
    // SAFETY: plain data, zero is valid.
    let mut ctx: REASON_CONTEXT = unsafe { zeroed() };
    // POWER_REQUEST_CONTEXT_VERSION is 0.
    ctx.Version = 0;
    ctx.Flags = POWER_REQUEST_CONTEXT_SIMPLE_STRING;
    ctx.Reason.SimpleReasonString = text.as_mut_ptr();
    // SAFETY: `ctx` and the string it points to live through the call (Windows copies it).
    let h = unsafe { PowerCreateRequest(&ctx) };
    if h.is_null() || h as isize == -1 {
        tracing::warn!("wake: PowerCreateRequest failed");
        return false;
    }
    // SAFETY: the request just made; closed if it can't be set.
    if unsafe { PowerSetRequest(h, PowerRequestSystemRequired) } == 0 {
        tracing::warn!("wake: PowerSetRequest failed");
        unsafe { CloseHandle(h) };
        return false;
    }
    *held = Some(h as isize);
    true
}
