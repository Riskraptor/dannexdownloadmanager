use super::next_trim;
use std::time::Duration;

/// The tray gives memory back 2 s after hiding, 2 minutes after hiding, then every
/// 10 minutes (#72).
#[test]
fn hidden_memory_is_given_back_at_2_s_2_min_then_every_10_min() {
    let mut at = Duration::ZERO;
    let times: Vec<u64> = (0..4)
        .map(|done| {
            at += next_trim(done);
            at.as_secs()
        })
        .collect();
    assert_eq!(times, [2, 120, 720, 1320]);
}
