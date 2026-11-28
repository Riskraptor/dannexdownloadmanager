use super::{AtLogin, TRAY_GRACE, at_login};
use std::time::Duration;

/// Where a start with the session leaves the window (#226), in each order its tray icon and
/// the desktop's tray can come in.
#[test]
fn the_window_waits_for_the_tray_then_stays_there_or_shows_minimized() {
    let s = Duration::from_secs;
    let icon = |d| Some(s(d));
    // The tray first (Windows, KDE): DDM's icon registered, the window stays hidden.
    assert_eq!(at_login(s(0), icon(0), true), AtLogin::Tray);
    // The decision first (XFCE: DDM looked before its own icon had registered; GNOME: the
    // AppIndicator extension's tray came 11.5 s after DDM's icon): waited for, hidden, then
    // the tray.
    assert_eq!(at_login(s(0), None, false), AtLogin::Wait);
    assert_eq!(at_login(s(0), icon(0), false), AtLogin::Wait);
    assert_eq!(at_login(s(13), icon(12), true), AtLogin::Tray);
    assert_eq!(
        at_login(s(60), TRAY_GRACE.checked_sub(s(1)), true),
        AtLogin::Tray
    );
    // The grace counts from DDM's icon: a slow start of its own leaves the desktop its time.
    assert_eq!(at_login(TRAY_GRACE + s(5), icon(5), false), AtLogin::Wait);
    // No tray within the grace period (a desktop without one, #59; Windows' taskbar not
    // there to take the icon): shown, minimized.
    assert_eq!(
        at_login(TRAY_GRACE, Some(TRAY_GRACE), false),
        AtLogin::Minimize
    );
    // No icon at all (the tray couldn't be made): the grace from the start.
    assert_eq!(at_login(TRAY_GRACE, None, false), AtLogin::Minimize);
    assert_eq!(at_login(s(60), None, true), AtLogin::Minimize);
}
