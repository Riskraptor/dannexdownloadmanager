/// #510: finished files are marked away from the window's thread; a file gone from its path
/// meanwhile (moved, deleted) isn't created again by the mark.
#[test]
fn a_finished_file_is_marked_and_a_gone_one_is_not_created() {
    let dir = tempfile::tempdir().unwrap();
    let here = dir.path().join("video.mp4");
    std::fs::write(&here, b"x").unwrap();
    let gone = dir.path().join("moved.mp4");
    super::mark_finished(
        vec![here.clone(), gone.clone()],
        "https://videos.test/v".into(),
        None,
    )
    .unwrap()
    .join()
    .unwrap();
    assert!(!gone.exists(), "a moved file's old path stays empty");
    #[cfg(windows)]
    {
        let mut stream = here.as_os_str().to_owned();
        stream.push(":Zone.Identifier");
        let zone = std::fs::read_to_string(stream).unwrap();
        assert!(zone.contains("ZoneId=3"), "{zone}");
        assert!(zone.contains("HostUrl=https://videos.test/v"), "{zone}");
    }
}
