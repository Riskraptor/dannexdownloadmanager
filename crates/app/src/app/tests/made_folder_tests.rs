use super::{MADE_FOLDERS, note_made, prune_made};

/// A download writing in `a\b\c` where only the downloads folder is there: DDM made `a`.
#[test]
fn the_topmost_missing_folder_is_ddms() {
    let dir = tempfile::tempdir().unwrap();
    let id = u64::MAX - 7;
    note_made(id, Some(&dir.path().join("a").join("b").join("c")));
    note_made(id - 1, Some(dir.path()));
    let made: Vec<_> = MADE_FOLDERS
        .lock()
        .unwrap()
        .iter()
        .filter(|(i, _)| *i >= id - 1)
        .cloned()
        .collect();
    assert_eq!(made, [(id, dir.path().join("a"))]);
}

/// Only empty folders up to the one DDM made go; the user's file, a kept folder and the
/// folder above stay.
#[test]
fn only_empty_folders_ddm_made_go() {
    let dir = tempfile::tempdir().unwrap();
    let made = dir.path().join("made");
    let deep = made.join("x").join("y");
    std::fs::create_dir_all(&deep).unwrap();
    prune_made(&deep, &made, &[]);
    assert!(!made.exists());
    assert!(dir.path().exists());

    std::fs::create_dir_all(&deep).unwrap();
    std::fs::write(made.join("mine.txt"), b"x").unwrap();
    prune_made(&deep, &made, &[]);
    assert!(!made.join("x").exists());
    assert!(made.join("mine.txt").exists());

    std::fs::create_dir_all(&deep).unwrap();
    prune_made(&deep, &made, &[made.join("x")]);
    assert!(!deep.exists() && made.join("x").exists());

    let elsewhere = dir.path().join("other");
    std::fs::create_dir_all(&elsewhere).unwrap();
    prune_made(&elsewhere, &made, &[]);
    assert!(elsewhere.exists());
}
