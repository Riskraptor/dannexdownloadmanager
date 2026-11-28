//! How long a download takes from being handed over to its first bytes (the owner's list, #3):
//! each step's moment, logged once, in milliseconds, so the time from a browser's click to
//! Download File Info and to the file's first bytes can be read off `logs\ddm.log`
//! (`tools/lab/start-latency.ps1` reads them). The browser's host logs its own part (crates/nmhost:
//! "handed over in"). Never an address or a file name: the log outlives private windows.

use std::time::Instant;

/// What a command hands over, for the log: None for anything else (a setting, a window to show).
pub fn handed_over(cmd: &mdm_ipc::Command) -> Option<&'static str> {
    if cmd.video_page.is_some() {
        Some("a video")
    } else if !cmd.links.is_empty() {
        Some("links")
    } else if cmd.url.is_some() {
        Some(if cmd.from_browser.is_some() || cmd.browser_waits {
            "a browser's download"
        } else {
            "an address"
        })
    } else {
        None
    }
}

/// Command `cmd`, which the channel took at `at`, is handled now: how long it waited for the
/// window's pass (the main window may be in the notification area).
pub fn taken(cmd: &mdm_ipc::Command, at: Instant) {
    if let Some(what) = handed_over(cmd) {
        tracing::info!(
            "start time: {what} handed over, taken by the window {} ms after the channel had it",
            ms(at)
        );
    }
}

/// Milliseconds since `t`.
pub fn ms(t: Instant) -> u128 {
    t.elapsed().as_millis()
}

/// The main window's next pass at once, though it is hidden in the notification area or
/// minimized (`win32::nudge_main_window`; nothing elsewhere, see `unix.rs`): a command came, or a
/// window it opens needs its next pass. From any thread.
pub fn nudge() {
    // MDM_NO_NUDGE=1: as before (passes held 100 ms apart while hidden), to measure the
    // difference (tools/lab/start-latency.ps1 -NoNudge).
    static OFF: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *OFF.get_or_init(|| std::env::var_os("MDM_NO_NUDGE").is_some_and(|v| v == "1")) {
        return;
    }
    #[cfg(windows)]
    crate::win32::nudge_main_window();
    #[cfg(not(windows))]
    crate::unix::nudge_main_window();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only what hands something over is timed; a setting or a window to show isn't.
    #[test]
    fn only_hand_overs_are_timed() {
        let browser = mdm_ipc::Command {
            url: Some("https://x.test/a.zip".into()),
            from_browser: Some("Google Chrome".into()),
            browser_waits: true,
            ..Default::default()
        };
        assert_eq!(handed_over(&browser), Some("a browser's download"));
        let typed = mdm_ipc::Command {
            url: Some("https://x.test/a.zip".into()),
            ..Default::default()
        };
        assert_eq!(handed_over(&typed), Some("an address"));
        let video = mdm_ipc::Command {
            video_page: Some("https://x.test/watch".into()),
            ..Default::default()
        };
        assert_eq!(handed_over(&video), Some("a video"));
        let show = mdm_ipc::Command {
            show: Some("options".into()),
            ..Default::default()
        };
        assert_eq!(handed_over(&show), None);
        assert_eq!(handed_over(&mdm_ipc::Command::default()), None);
    }
}
