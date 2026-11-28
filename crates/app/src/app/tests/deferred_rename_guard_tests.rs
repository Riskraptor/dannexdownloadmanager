use super::*;

fn test_item(id: u64, dest: PathBuf) -> Item {
    Item::new(
        id,
        "https://example.com/item".parse().unwrap(),
        dest,
        model::Category::General,
    )
}

#[test]
fn refuses_shared_category_and_save_folders_but_allows_exclusive_folder() {
    let dir = tempfile::tempdir().unwrap();
    let downloads = dir.path().join("Downloads");
    let video_cat = downloads.join("Video");
    let saved_dir = dir.path().join("SavedAlbums");
    let shared_gallery = downloads.join("pixiv");
    let exclusive_gallery = downloads.join("gallery-42");
    for d in [
        &downloads,
        &video_cat,
        &saved_dir,
        &shared_gallery,
        &exclusive_gallery,
    ] {
        std::fs::create_dir_all(d).unwrap();
    }

    let mut settings = Settings::default();
    settings
        .folder_history
        .push(saved_dir.display().to_string());
    let rss = crate::rss::Store::default();

    let mut g_root = test_item(1, downloads.clone());
    g_root.gallery = Some(Default::default());

    let mut g_cat = test_item(2, video_cat.clone());
    g_cat.gallery = Some(Default::default());

    let mut g_saved = test_item(3, saved_dir.clone());
    g_saved.gallery = Some(Default::default());

    let mut g_shared_a = test_item(4, shared_gallery.clone());
    g_shared_a.gallery = Some(Default::default());
    let g_shared_b = test_item(5, shared_gallery.join("other.jpg"));

    let mut g_exclusive = test_item(6, exclusive_gallery.clone());
    g_exclusive.gallery = Some(Default::default());

    let items = vec![g_root, g_cat, g_saved, g_shared_a, g_shared_b, g_exclusive];

    for idx in [0usize, 1, 2, 3] {
        let blocked = &items[idx];
        assert_eq!(
            deferred_rename_disabled_reason_with(blocked, &downloads, &settings, &rss, &items),
            Some("This folder is shared with another download or save folder"),
            "expected {} to be refused",
            blocked.dest.display()
        );
    }
    assert_eq!(
        deferred_rename_disabled_reason_with(&items[5], &downloads, &settings, &rss, &items),
        None,
        "an exclusive gallery subfolder can be renamed safely"
    );

    let reused_gallery = downloads.join("gallery-reused");
    std::fs::create_dir_all(&reused_gallery).unwrap();
    let mut g_preexisting = test_item(7, reused_gallery.clone());
    g_preexisting.gallery = Some(Default::default());
    g_preexisting.preexisting_folder = true;
    assert_eq!(
        deferred_rename_disabled_reason_with(&g_preexisting, &downloads, &settings, &rss, &items,),
        Some("This folder holds files from outside this download"),
        "a reused folder that already held untracked files is refused"
    );

    std::fs::write(reused_gallery.join("foreign.txt"), b"leftover").unwrap();
    let mut g_untracked = test_item(8, reused_gallery.clone());
    g_untracked.gallery = Some(Default::default());
    g_untracked.written_files = Some(Vec::new());
    assert_eq!(
        deferred_rename_disabled_reason_with(&g_untracked, &downloads, &settings, &rss, &items),
        Some("This folder holds files from outside this download"),
        "a folder containing files the download did not write is refused"
    );

    // Persisted across restart (`serde_json`) and updated when `take_place` renames the folder.
    let tracked = reused_gallery.join("01.jpg");
    std::fs::write(&tracked, b"ok").unwrap();
    let mut g_persisted = test_item(9, reused_gallery.clone());
    g_persisted.gallery = Some(Default::default());
    g_persisted.written_files = Some(vec![tracked]);
    let reloaded: Item =
        serde_json::from_str(&serde_json::to_string(&g_persisted).unwrap()).unwrap();
    assert_eq!(
        deferred_rename_disabled_reason_with(&reloaded, &downloads, &settings, &rss, &items),
        Some("This folder holds files from outside this download"),
        "persisted written_files still catches foreign.txt after restart"
    );

    std::fs::remove_file(reused_gallery.join("foreign.txt")).unwrap();
    assert_eq!(
        deferred_rename_disabled_reason_with(&reloaded, &downloads, &settings, &rss, &items),
        None
    );

    let moved_gallery = downloads.join("gallery-reused-renamed");
    std::fs::rename(&reused_gallery, &moved_gallery).unwrap();
    let mut moved_item = reloaded;
    take_place(&mut moved_item, moved_gallery.clone());
    assert_eq!(
        moved_item.written_files,
        Some(vec![moved_gallery.join("01.jpg")])
    );
    assert_eq!(
        deferred_rename_disabled_reason_with(&moved_item, &downloads, &settings, &rss, &items),
        None
    );
}

#[test]
fn guards_split_archives_and_single_file_torrents() {
    let dir = tempfile::tempdir().unwrap();
    let downloads = dir.path().join("Downloads");
    std::fs::create_dir_all(&downloads).unwrap();
    let mut settings = Settings::default();
    let rss = crate::rss::Store::default();

    let part1 = test_item(1, downloads.join("backup.part1.rar"));
    let standalone_rar = test_item(2, downloads.join("standalone.rar"));

    let mut single_torrent = test_item(3, downloads.join("ubuntu.iso"));
    single_torrent.torrent = Some(crate::torrents::TorrentSpec {
        hash: "0123456789abcdef0123456789abcdef01234567".into(),
        folder: downloads.clone(),
        files: None,
        uploaded: 0,
        seeded_secs: 0,
        peers: 0,
        up_speed: 0.0,
        upload_limit: None,
        in_order: false,
        ends_first: false,
    });

    let pack_dir = downloads.join("PackFolder");
    std::fs::create_dir_all(&pack_dir).unwrap();
    let mut folder_torrent = test_item(4, pack_dir.clone());
    folder_torrent.torrent = Some(crate::torrents::TorrentSpec {
        hash: "fedcba9876543210fedcba9876543210fedcba98".into(),
        folder: pack_dir,
        files: None,
        uploaded: 0,
        seeded_secs: 0,
        peers: 0,
        up_speed: 0.0,
        upload_limit: None,
        in_order: false,
        ends_first: false,
    });

    let items = vec![part1, standalone_rar, single_torrent, folder_torrent];

    assert_eq!(
        deferred_rename_disabled_reason_with(&items[0], &downloads, &settings, &rss, &items),
        Some("Split archive volumes keep their names for extraction")
    );
    assert_eq!(
        deferred_rename_disabled_reason_with(&items[1], &downloads, &settings, &rss, &items),
        None
    );

    settings.torrent_keep_sharing = true;
    assert_eq!(
        deferred_rename_disabled_reason_with(&items[2], &downloads, &settings, &rss, &items),
        Some("A single-file torrent keeps its name")
    );
    assert_eq!(
        deferred_rename_disabled_reason_with(&items[3], &downloads, &settings, &rss, &items),
        None,
        "an exclusive multi-file torrent folder can be renamed and re-attached"
    );

    settings.torrent_keep_sharing = false;
    assert_eq!(
        deferred_rename_disabled_reason_with(&items[2], &downloads, &settings, &rss, &items),
        Some("A single-file torrent keeps its name"),
        "single-file torrent keeps its name even with Keep sharing off so a resume finds it"
    );
}

#[test]
fn pending_mark_updates_video_subtitles_and_gallery_paths_after_rename() {
    let dir = tempfile::tempdir().unwrap();
    let from_vid = dir.path().join("raw_id.mp4");
    let sub_en = dir.path().join("raw_id.en.srt");
    std::fs::write(&from_vid, b"v").unwrap();
    std::fs::write(&sub_en, b"s").unwrap();

    let mut vid_item = test_item(1, from_vid.clone());
    vid_item.pending_mark = Some(model::PendingMark {
        main_file: Some(from_vid.clone()),
        side_files: vec![sub_en.clone()],
        gallery_files: Vec::new(),
        url: "https://videos.test/watch".into(),
        referer: None,
    });

    // Done beside the window (#743): the video and its subtitles renamed there.
    let downloads = dir.path().join("Downloads");
    let Renamed::Done {
        to: actual_to,
        is_dir: false,
        side_files,
    } = rename_beside(&vid_item, "Talk 01", &downloads)
    else {
        panic!("the video is renamed");
    };
    apply_renamed_paths_to_pending_mark(&mut vid_item, &from_vid, &actual_to, false, side_files);
    let pm = vid_item.pending_mark.unwrap();
    assert_eq!(pm.main_file, Some(dir.path().join("Talk 01.mp4")));
    assert_eq!(pm.side_files, vec![dir.path().join("Talk 01.en.srt")]);
    assert!(dir.path().join("Talk 01.en.srt").exists());
    assert!(!sub_en.exists());

    let old_gal = dir.path().join("gal_raw");
    std::fs::create_dir_all(old_gal.join("sub")).unwrap();
    let img = old_gal.join("sub").join("01.png");
    std::fs::write(&img, b"img").unwrap();
    let mut gal_item = test_item(2, old_gal.clone());
    gal_item.gallery = Some(Default::default());
    gal_item.pending_mark = Some(model::PendingMark {
        main_file: None,
        side_files: Vec::new(),
        gallery_files: vec![img],
        url: "https://gallery.test/a".into(),
        referer: None,
    });
    let Renamed::Done {
        to: actual_gal,
        is_dir: true,
        side_files,
    } = rename_beside(&gal_item, "Album 01", &downloads)
    else {
        panic!("the gallery's folder is renamed");
    };
    apply_renamed_paths_to_pending_mark(&mut gal_item, &old_gal, &actual_gal, true, side_files);
    let gpm = gal_item.pending_mark.unwrap();
    assert_eq!(
        gpm.gallery_files,
        vec![dir.path().join("Album 01").join("sub").join("01.png")]
    );
    assert!(
        dir.path()
            .join("Album 01")
            .join("sub")
            .join("01.png")
            .exists()
    );
}

/// `deferred_rename_beside` for `item` alone in the list, renamed to `desired`.
fn rename_beside(item: &Item, desired: &str, downloads: &std::path::Path) -> Renamed {
    deferred_rename_beside(RenameJob {
        me: item.place_copy(),
        items: vec![item.place_copy()],
        downloads: downloads.to_owned(),
        settings: Settings::default(),
        rss: crate::rss::Store::default(),
        desired: desired.into(),
        side_files: item
            .pending_mark
            .as_ref()
            .map(|pm| pm.side_files.clone())
            .unwrap_or_default(),
    })
}

/// #743: a deferred rename is decided and done beside the window, every way it can go as it
/// went on the window's thread: a name taken numbers on, a folder shared with another
/// download stays (and says why), the name it has or a blank one is nothing to do, a file
/// gone meanwhile fails with the system's reason, and a torrent only gets its place held
/// for `torrent_host`.
#[test]
fn a_deferred_rename_is_done_beside_the_window() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let downloads = d.join("Downloads");
    let file = d.join("raw.zip");
    std::fs::write(&file, b"z").unwrap();
    std::fs::write(d.join("Report.zip"), b"another").unwrap();
    let item = test_item(1, file.clone());

    // The name is taken: the next free one, the extension kept.
    let Renamed::Done { to, is_dir, .. } = rename_beside(&item, "Report", &downloads) else {
        panic!("renamed under a numbered name");
    };
    assert_eq!(to, d.join("Report (1).zip"));
    assert!(!is_dir);
    assert!(to.exists() && !file.exists());

    // The name it has, or none: nothing to do, nothing said.
    let item = test_item(1, to.clone());
    assert!(matches!(
        rename_beside(&item, "Report (1)", &downloads),
        Renamed::Refused(None)
    ));
    assert!(matches!(
        rename_beside(&item, "  ", &downloads),
        Renamed::Refused(None)
    ));

    // Gone from under it (moved by hand): the system's reason, to say.
    let gone = test_item(2, d.join("gone.zip"));
    assert!(matches!(
        rename_beside(&gone, "Kept", &downloads),
        Renamed::Failed(_)
    ));

    // A gallery in the downloads folder itself: shared, it stays, and says why.
    std::fs::create_dir_all(&downloads).unwrap();
    let mut shared = test_item(3, downloads.clone());
    shared.gallery = Some(Default::default());
    let Renamed::Refused(Some(why)) = rename_beside(&shared, "Album", &downloads) else {
        panic!("a shared folder keeps its name");
    };
    assert_eq!(
        why,
        "This folder is shared with another download or save folder"
    );
    assert!(downloads.exists());

    // A torrent's folder: its new place held, moved by `torrent_host`, not here.
    let folder = d.join("Show.S01");
    std::fs::create_dir_all(&folder).unwrap();
    let mut torrent = test_item(4, folder.clone());
    torrent.torrent = Some(crate::torrents::TorrentSpec {
        hash: "00".repeat(20),
        files: None,
        folder: folder.clone(),
        uploaded: 0,
        seeded_secs: 0,
        peers: 0,
        up_speed: 0.0,
        upload_limit: None,
        in_order: false,
        ends_first: true,
    });
    let Renamed::Reserved { to, hold, is_dir } = rename_beside(&torrent, "Show", &downloads) else {
        panic!("a torrent's place is held for torrent_host");
    };
    assert_eq!(to, d.join("Show"));
    assert!(is_dir && hold.is_some());
    assert!(folder.exists(), "torrent_host moves it, not the look");
}
