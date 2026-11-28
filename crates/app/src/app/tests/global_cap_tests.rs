use crate::app::speed::{helper_rate, share};

/// #708: a video's or a gallery's limit as it starts: its own (or the one for one file), and
/// an even share of the limit for all downloads with the other helpers running.
#[test]
fn a_helper_starts_under_both_limits() {
    const K: u64 = 1024;
    assert_eq!(helper_rate(None, None, 0), None);
    assert_eq!(helper_rate(Some(500 * K), None, 3), Some(500 * K));
    assert_eq!(helper_rate(None, Some(500 * K), 0), Some(500 * K));
    assert_eq!(
        helper_rate(None, Some(500 * K), 1),
        Some(250 * K),
        "shared by two"
    );
    assert_eq!(helper_rate(Some(200 * K), Some(500 * K), 1), Some(200 * K));
    assert_eq!(helper_rate(Some(400 * K), Some(500 * K), 1), Some(250 * K));
    assert_eq!(
        helper_rate(None, Some(2 * K), 9),
        Some(K),
        "never under 1 KB/s"
    );
}

#[test]
fn each_side_gets_half_and_what_the_other_leaves() {
    let cap = 800 * 1024;
    assert_eq!(share(cap, 0.0), cap, "the other idle: all of it");
    assert_eq!(share(cap, 100.0 * 1024.0), 700 * 1024, "a slow other side");
    assert_eq!(share(cap, 300.0 * 1024.0), 500 * 1024);
    assert_eq!(share(cap, 400.0 * 1024.0), 400 * 1024);
    assert_eq!(
        share(cap, 350.0 * 1024.0),
        400 * 1024,
        "a wobble under its half"
    );
    assert_eq!(share(cap, 800.0 * 1024.0), 400 * 1024, "half at least");
    assert_eq!(
        share(cap, 5e9),
        400 * 1024,
        "the other measured over the cap"
    );
    assert_eq!(share(1024, 0.0), 1024);
    assert_eq!(share(1500, 1500.0), 1024, "at least 1 KB/s");
}
