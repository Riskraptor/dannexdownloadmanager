use super::{TreeStep, tree_side_step};

/// Left and Right in the Categories tree, as a Windows tree view: Right opens a shut
/// branch and then goes to its first child; Left closes an open branch, else goes to the
/// parent; a header without a view of its own ("Queues") is never gone to.
#[test]
fn left_and_right_open_close_and_climb() {
    // All (open) > Music, Video; Unfinished (shut); Queues (open, no view) > Main.
    let rows = [
        (0, Some(true), true),
        (1, None, true),
        (1, None, true),
        (0, Some(false), true),
        (0, Some(true), false),
        (1, None, true),
    ];
    assert_eq!(tree_side_step(&rows, 0, true), Some(TreeStep::Select(1)));
    assert_eq!(tree_side_step(&rows, 0, false), Some(TreeStep::Toggle));
    assert_eq!(tree_side_step(&rows, 2, false), Some(TreeStep::Select(0)));
    assert_eq!(
        tree_side_step(&rows, 2, true),
        None,
        "a leaf has nothing under it"
    );
    assert_eq!(tree_side_step(&rows, 3, true), Some(TreeStep::Toggle));
    assert_eq!(
        tree_side_step(&rows, 3, false),
        None,
        "a shut root has no parent"
    );
    assert_eq!(tree_side_step(&rows, 5, false), None, "Queues has no view");
    assert_eq!(tree_side_step(&rows, 9, true), None);
    // An open branch whose first child has no view goes to the first that has one.
    let rows = [(0, Some(true), true), (1, None, false), (1, None, true)];
    assert_eq!(tree_side_step(&rows, 0, true), Some(TreeStep::Select(2)));
    // An open branch with no children (all filtered away) stays.
    let rows = [(0, Some(true), true), (0, None, true)];
    assert_eq!(tree_side_step(&rows, 0, true), None);
}
