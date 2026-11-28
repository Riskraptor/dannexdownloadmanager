use super::finished_windows;
use crate::progress::SHOWN_WHOLE;
use std::time::{Duration, Instant};

/// A window on screen shows its finished download whole for SHOWN_WHOLE, then closes; one
/// not on screen (hidden in the tray, or none) closes at once; one started again meanwhile
/// stays for the new run.
#[test]
fn a_finished_window_shows_it_whole_for_a_moment() {
    let t0 = Instant::now();
    let ms = Duration::from_millis;
    let mut finishing = Vec::new();
    let close = finished_windows(vec![1, 2], |id| id == 1, |_| true, &mut finishing, t0);
    assert_eq!(close, [2], "not on screen: at once");
    assert_eq!(finishing, [(1, t0)]);
    let soon = t0 + SHOWN_WHOLE - ms(1);
    let close = finished_windows(vec![], |_| true, |_| true, &mut finishing, soon);
    assert!(close.is_empty(), "held for the moment");
    let close = finished_windows(vec![3], |_| true, |_| true, &mut finishing, t0 + ms(100));
    assert!(close.is_empty());
    let close = finished_windows(vec![], |_| true, |_| true, &mut finishing, t0 + SHOWN_WHOLE);
    assert_eq!(close, [1], "then it closes");
    // 3, started again during its moment: not closed, and no longer waited for.
    let later = t0 + ms(100) + SHOWN_WHOLE;
    let close = finished_windows(vec![], |_| true, |id| id != 3, &mut finishing, later);
    assert!(close.is_empty());
    assert!(finishing.is_empty());
}
