use super::place_in;

/// #333: a selection moved to one folder keeps each its own name, or the next free one when
/// several would land on the same name (not started: no file yet) or one is on the disk.
#[test]
fn a_selection_gets_a_free_name_each() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    std::fs::write(d.join("a.zip"), b"x").unwrap();
    let mut taken = std::collections::HashSet::new();
    let names: Vec<String> = ["a.zip", "a.zip", "b.bin", "b.bin"]
        .iter()
        .map(|n| {
            place_in(d, n, &mut taken)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(names, ["a (1).zip", "a (2).zip", "b.bin", "b (1).bin"]);
}
