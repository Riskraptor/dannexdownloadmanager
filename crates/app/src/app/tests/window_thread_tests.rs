use super::*;

/// #423: the files of a stopped download go as a group, off the window's thread.
#[test]
fn a_stopped_downloads_files_are_removed_together() {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("file.bin");
    let part = mdm_engine::naming::part_path(&dest);
    let state = mdm_engine::state::state_path(&dest);
    // And those under their former names (`.mdm.part`, Dannex up to 0.6.5).
    let legacy = mdm_engine::state::legacy_files(&dest);
    for f in [&dest, &part, &state].into_iter().chain(&legacy) {
        std::fs::write(f, b"x").unwrap();
    }
    remove_files_of(&dest, false);
    assert!(!part.exists() && !state.exists());
    assert!(legacy.iter().all(|f| !f.exists()));
    assert!(dest.exists(), "a file that wasn't complete stays");
    remove_files_of(&dest, true);
    assert!(!dest.exists());
}
