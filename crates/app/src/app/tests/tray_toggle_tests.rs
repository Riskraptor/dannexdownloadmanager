use super::{TRAY_TOGGLE_GRACE, tray_toggle_hide};
use std::time::Duration;

/// Left click toggles: hidden or minimized shows; frontmost hides; behind
/// shows; a click that just took the focus (Windows) still hides (#520).
#[test]
fn only_a_frontmost_window_hides() {
    let ms = Duration::from_millis;
    // Hidden or minimized: always shows.
    assert!(!tray_toggle_hide(true, false, Some(false), None));
    assert!(!tray_toggle_hide(false, true, Some(true), None));
    // Frontmost: hides.
    assert!(tray_toggle_hide(false, false, Some(true), None));
    // Behind other windows: shows.
    assert!(!tray_toggle_hide(
        false,
        false,
        Some(false),
        Some(TRAY_TOGGLE_GRACE + ms(1))
    ));
    assert!(!tray_toggle_hide(false, false, Some(false), None));
    // The click stole the focus just now (Windows): still hides.
    assert!(tray_toggle_hide(
        false,
        false,
        Some(false),
        Some(TRAY_TOGGLE_GRACE - ms(1))
    ));
    // Unknown focus: unsure means show, never hide.
    assert!(!tray_toggle_hide(false, false, None, None));
    assert!(!tray_toggle_hide(false, false, None, Some(Duration::ZERO)));
}
