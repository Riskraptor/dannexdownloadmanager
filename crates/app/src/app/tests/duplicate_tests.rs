use super::*;
use mdm_tools::video::{InfoSlot, Preloaded};

const SELECTOR: &str = "bv*[height=720]+ba/b[height=720]";

fn page() -> url::Url {
    url::Url::parse("https://videos.test/watch?v=1").unwrap()
}

/// A choice from the quality menu, holding (a clone of) `slot`, the menu's look-up.
fn choice(slot: &InfoSlot) -> crate::videodlg::VideoAdd {
    crate::videodlg::VideoAdd {
        page: page(),
        selector: SELECTOR.into(),
        dest: PathBuf::from("Clip.mp4"),
        category: Category::Video,
        size: None,
        size_approx: false,
        start_now: true,
        context: Default::default(),
        subs: Default::default(),
        remember: None,
        preloaded: Some(Preloaded {
            slot: slot.clone(),
            selector: SELECTOR.into(),
            label: "720p".into(),
            merge: true,
        }),
    }
}

/// Options' four answers to a duplicate, for any download; a finished file that is gone
/// (#342) leaves nothing to duplicate, whatever the answer.
#[test]
fn options_choice_decides_what_a_duplicate_does() {
    for action in 0..=4 {
        assert_eq!(duplicate_step(action, None), DupStep::Add);
        assert_eq!(
            duplicate_step(action, Some((7, true))),
            DupStep::Replace(7, false)
        );
    }
    let dup = Some((7, false));
    assert_eq!(duplicate_step(0, dup), DupStep::Ask(7));
    assert_eq!(duplicate_step(1, dup), DupStep::Add);
    assert_eq!(duplicate_step(2, dup), DupStep::Replace(7, true));
    assert_eq!(duplicate_step(3, dup), DupStep::Existing(7));
    assert_eq!(duplicate_step(9, dup), DupStep::Ask(7), "unknown: asked");
}

/// A video is the same download only on the same page in the same quality; a plain download
/// of that address is not a video's duplicate.
#[test]
fn a_video_is_a_duplicate_in_the_same_quality_only() {
    let mut item = Item::new(1, page(), PathBuf::from("Clip.mp4"), Category::Video);
    assert!(!same_video(&item, &page(), SELECTOR), "not a video");
    item.video = Some(model::VideoSpec {
        selector: SELECTOR.into(),
        subs: Default::default(),
    });
    assert!(same_video(&item, &page(), SELECTOR));
    assert!(!same_video(
        &item,
        &page(),
        "bv*[height=360]+ba/b[height=360]"
    ));
    let other = url::Url::parse("https://videos.test/watch?v=2").unwrap();
    assert!(!same_video(&item, &other, SELECTOR));
}

/// Each Options answer for a video (the quality menu's look-up with it), where it goes:
/// the look-up goes with the new download only, dropped when the one already there is
/// opened or resumed. Asked about, or over the one it overwrites (added once that one's
/// files are removed beside the window, #743), it waits, look-up and all.
#[test]
fn each_answer_for_a_video() {
    let slot = InfoSlot::new(Instant::now());
    for action in 0..=3u8 {
        let step = duplicate_step(action, Some((7, false)));
        let route = video_route(step, choice(&slot));
        match (action, &route) {
            (1, VideoRoute::Add(add)) => {
                assert!(add.preloaded.is_some());
                assert_eq!(slot.holders(), 2, "kept for the new download");
            }
            (0 | 2, VideoRoute::Wait(Pending::Video(add))) => {
                assert!(add.preloaded.is_some());
                assert_eq!(slot.holders(), 2, "kept for the answer");
            }
            (3, VideoRoute::Dropped) => {
                assert_eq!(slot.holders(), 1, "the look-up went with the choice");
            }
            _ => panic!("action {action}: unexpected route"),
        }
        drop(route);
        assert_eq!(slot.holders(), 1);
    }
}

/// The question's answer: opening or resuming the one already there (3) drops the new
/// choice and its look-up; a numbered name (1) or overwrite (2, of that one) adds it,
/// look-up and all; whatever the answer, a question about a download that has left the list
/// meanwhile is checked afresh (never acted on a row that isn't there).
#[test]
fn the_answer_open_or_resume_drops_the_new_choices_look_up() {
    let slot = InfoSlot::new(Instant::now());
    for answer in 1..=3 {
        let pending = Pending::Video(Box::new(choice(&slot)));
        match (answer, answered(7, true, answer, pending)) {
            (3, Answer::Existing(7)) => assert_eq!(slot.holders(), 1),
            (1, Answer::Add(None, Pending::Video(add)))
            | (2, Answer::Add(Some(7), Pending::Video(add))) => {
                assert!(add.preloaded.is_some());
                assert_eq!(slot.holders(), 2);
            }
            _ => panic!("answer {answer}: unexpected"),
        }
        assert_eq!(slot.holders(), 1);
        let pending = Pending::Video(Box::new(choice(&slot)));
        match answered(7, false, answer, pending) {
            Answer::Again(Pending::Video(add)) => {
                assert!(add.preloaded.is_some(), "checked afresh, look-up and all");
            }
            _ => panic!("answer {answer} about a row gone: not checked afresh"),
        }
    }
}

/// The menu's look-up goes with a download started now, while fresh: not with one queued,
/// nor one added on a duplicate question answered after the look-ups are forgotten (its
/// links will have expired; it looks the page up itself, as at its start).
#[test]
fn a_late_answer_drops_the_menus_look_up() {
    let born = Instant::now();
    let slot = InfoSlot::new(born);
    let ttl = crate::videolookup::TTL;
    let pre = || choice(&slot).preloaded;
    assert!(kept_lookup(pre(), VideoGoes::Start, born).is_some());
    let just = born + ttl - Duration::from_secs(1);
    assert!(kept_lookup(pre(), VideoGoes::Start, just).is_some());
    assert!(kept_lookup(pre(), VideoGoes::Start, born + ttl).is_none());
    let hours = born + Duration::from_secs(3 * 3600);
    assert!(kept_lookup(pre(), VideoGoes::Start, hours).is_none());
    assert!(kept_lookup(pre(), VideoGoes::MainQueue, born).is_none());
    assert!(kept_lookup(pre(), VideoGoes::AskQueue, born).is_none());
    assert_eq!(slot.holders(), 1, "every one dropped");
}

/// What replacing `id` takes from the disk: found without it (`replacement`), then read
/// beside the window (`Removal::files`, #743).
fn replaced_files(items: &[Item], id: u64, yt_dlp: bool) -> Option<Vec<PathBuf>> {
    replacement(items, id, yt_dlp).map(Removal::files)
}

/// A video row in `dir` named `name`, finished or not.
fn video_row(id: u64, dir: &std::path::Path, name: &str, status: Status) -> Item {
    let mut i = Item::new(id, page(), dir.join(name), Category::Video);
    i.status = status;
    i.video = Some(model::VideoSpec {
        selector: SELECTOR.into(),
        subs: Default::default(),
    });
    i
}

fn names(files: &[PathBuf]) -> Vec<String> {
    let mut n: Vec<String> = files
        .iter()
        .filter(|f| f.exists())
        .map(|f| f.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    n.sort();
    n
}

/// Overwrite takes the old video's own files (its file, yt-dlp's leftovers, its subtitles),
/// never those of another download in that folder whose name or stem starts them
/// (`Clip.f137.mp4`'s parts, `Clip.v2.zip`'s), running or paused; one sharing its very stem
/// (`Clip.zip`) leaves every leftover where it is.
#[test]
fn overwrite_spares_every_other_downloads_files() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    for f in [
        "Clip.mp4",
        "Clip.en.srt",
        "Clip.fr.srt",
        "Clip.mp4.part-Frag3",
        "Clip.f140.m4a",
        "Clip.f137.mp4.part",
        "Clip.f137.mp4.ytdl",
        "CLIP.F137.mp4.part-Frag2",
        "Clip.v2.zip.ddm.part",
    ] {
        std::fs::write(d.join(f), b"x").unwrap();
    }
    let old = || video_row(1, d, "Clip.mp4", Status::Complete);
    // Another video named after a stream of the first, running: its parts are its own.
    let other = video_row(2, d, "Clip.f137.mp4", Status::Downloading);
    // A plain download, paused, named alike.
    let mut plain = Item::new(3, page(), d.join("Clip.v2.zip"), Category::General);
    plain.status = Status::Stopped;
    let files = replaced_files(&[old(), other, plain], 1, true).unwrap();
    assert_eq!(
        names(&files),
        [
            "Clip.en.srt",
            "Clip.f140.m4a",
            "Clip.fr.srt",
            "Clip.mp4",
            "Clip.mp4.part-Frag3"
        ]
    );
    // Another download of the same stem: only the old one's own file.
    let zip = Item::new(4, page(), d.join("clip.zip"), Category::General);
    let files = replaced_files(&[old(), zip], 1, true).unwrap();
    assert_eq!(names(&files), ["Clip.mp4"]);
    // One of that name in another folder changes nothing here.
    let elsewhere = tempfile::tempdir().unwrap();
    let far = video_row(5, elsewhere.path(), "Clip.f140.mp4", Status::Stopped);
    let files = replaced_files(&[old(), far], 1, true).unwrap();
    assert!(names(&files).contains(&"Clip.f140.m4a".to_owned()));
}

/// A running download is left alone; #342's automatic replacement (its finished file gone)
/// takes its row, state and part file only, never what yt-dlp left beside it.
#[test]
fn a_running_one_is_left_alone_and_a_gone_ones_leftovers_stay() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    for f in ["Clip.en.srt", "Clip.mp4.part", "Clip.f140.m4a"] {
        std::fs::write(d.join(f), b"x").unwrap();
    }
    let mut running = video_row(1, d, "Clip.mp4", Status::Downloading);
    // A run of a yt-dlp that isn't there: it fails at once, on its own thread (no network).
    let job = mdm_tools::video::VideoJob::start(
        &d.join("no-yt-dlp.exe"),
        page().as_str(),
        SELECTOR,
        d,
        "Clip",
        None,
        None,
        None,
        &Default::default(),
        &Default::default(),
        None,
    )
    .unwrap();
    running.live = Some(Live::new(model::Job::Video { job, url: page() }, 0));
    assert!(replaced_files(&[running], 1, true).is_none());
    // #342: finished, its file gone.
    let gone = video_row(1, d, "Clip.mp4", Status::Complete);
    let files = replaced_files(&[gone], 1, false).unwrap();
    assert!(names(&files).is_empty(), "{:?}", names(&files));
    assert_eq!(
        files,
        [
            d.join("Clip.mp4"),
            state_path(&d.join("Clip.mp4")),
            mdm_engine::naming::part_path(&d.join("Clip.mp4")),
        ]
        .into_iter()
        .chain(mdm_engine::state::legacy_files(&d.join("Clip.mp4")))
        .collect::<Vec<_>>()
    );
    assert!(replaced_files(&[], 1, true).is_none());
}

/// #743: the window's thread decides a duplicate without the disk. None, or one not finished
/// (its file isn't expected yet, never gone), at once as before; a finished one is looked
/// for beside the window first, its file there or gone alike (the window doesn't ask). The
/// look's answer (`finished_file_gone`, beside the window) then decides as the window's own
/// look did, every Options answer: #342's gone file replaced, else Options' rule.
#[test]
fn a_finished_duplicate_is_looked_for_beside_the_window() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    std::fs::write(d.join("there.zip"), b"x").unwrap();
    let row = |name: &str, status| {
        let mut i = Item::new(7, page(), d.join(name), Category::General);
        i.status = status;
        i
    };
    for action in 0..=4u8 {
        assert_eq!(duplicate_seen(action, None), DupStep::Add);
        for status in [
            Status::Queued,
            Status::Stopped,
            Status::Error,
            Status::Downloading,
        ] {
            for name in ["there.zip", "gone.zip"] {
                assert_eq!(
                    duplicate_seen(action, Some(&row(name, status))),
                    duplicate_step(action, Some((7, false))),
                    "not finished: decided at once"
                );
            }
        }
        for name in ["there.zip", "gone.zip"] {
            let finished = row(name, Status::Complete);
            assert_eq!(
                duplicate_seen(action, Some(&finished)),
                DupStep::Look(7),
                "finished: its file looked for first"
            );
            let gone = model::finished_file_gone(Status::Complete, &finished.dest);
            assert_eq!(gone, name == "gone.zip");
            let step = duplicate_step(action, Some((7, gone)));
            if gone {
                assert_eq!(step, DupStep::Replace(7, false), "#342");
            } else {
                assert_eq!(step, duplicate_step(action, Some((7, false))));
            }
        }
    }
    // Its folder gone too (a drive unplugged): not gone, Options' rule.
    let far = row("not there/x.zip", Status::Complete);
    assert!(!model::finished_file_gone(Status::Complete, &far.dest));
}

/// #743: the look's answer, once it comes, decides as Options say about the row as the look
/// found it; a row that left the list, or changed meanwhile (moved, started again, not
/// finished now), or a look that ended without an answer, has the new download checked
/// afresh, never acted on a row that isn't what was looked at.
#[test]
fn a_looks_answer_decides_on_the_row_it_looked_at() {
    let at = PathBuf::from("Downloads").join("file.zip");
    let mut done = Item::new(7, page(), at.clone(), Category::General);
    done.status = Status::Complete;
    for action in 0..=3u8 {
        for gone in [true, false] {
            assert_eq!(
                looked(action, std::slice::from_ref(&done), 7, &at, Some(gone)),
                Some(duplicate_step(action, Some((7, gone))))
            );
        }
        assert_eq!(
            looked(action, std::slice::from_ref(&done), 7, &at, None),
            None
        );
        assert_eq!(looked(action, &[], 7, &at, Some(true)), None, "left");
    }
    let mut moved = Item::new(7, page(), at.with_file_name("moved.zip"), Category::General);
    moved.status = Status::Complete;
    assert_eq!(looked(2, &[moved], 7, &at, Some(false)), None, "moved");
    let mut again = Item::new(7, page(), at.clone(), Category::General);
    again.status = Status::Queued;
    assert_eq!(
        looked(2, &[again], 7, &at, Some(true)),
        None,
        "redownloaded"
    );
}

/// #743: while a finished duplicate's file is looked for, the choice waits with its
/// look-up, which goes with the new download if it is added then.
#[test]
fn a_choice_waits_with_its_look_up_while_its_duplicate_is_looked_for() {
    let slot = InfoSlot::new(Instant::now());
    match video_route(DupStep::Look(7), choice(&slot)) {
        VideoRoute::Wait(Pending::Video(add)) => {
            assert!(add.preloaded.is_some());
            assert_eq!(slot.holders(), 2, "kept for the answer");
        }
        _ => panic!("waits for the look"),
    }
    assert_eq!(slot.holders(), 1);
}

/// #743: what Overwrite takes is found on the window's thread without the disk
/// (`replacement`): a video's leftovers are read from its folder only beside the window
/// (`Removal::files`: files written after the window's part are found), and removed there
/// (`Removal::run`), another download's spared. One in a folder that isn't there needs
/// nothing of the disk on the window's thread either.
#[test]
fn overwrite_reads_and_removes_its_files_beside_the_window() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    std::fs::write(d.join("Clip.mp4"), b"x").unwrap();
    let old = video_row(1, d, "Clip.mp4", Status::Complete);
    let other = video_row(2, d, "Clip.f137.mp4", Status::Stopped);
    let removal = replacement(&[old, other], 1, true).unwrap();
    for f in ["Clip.en.srt", "Clip.f140.m4a", "Clip.f137.mp4.part"] {
        std::fs::write(d.join(f), b"x").unwrap();
    }
    removal.run();
    for f in ["Clip.mp4", "Clip.en.srt", "Clip.f140.m4a"] {
        assert!(!d.join(f).exists(), "{f} removed");
    }
    assert!(
        d.join("Clip.f137.mp4.part").exists(),
        "another's part stays"
    );

    let nowhere = d.join("not there");
    let removal = replacement(
        &[video_row(3, &nowhere, "Clip.mp4", Status::Complete)],
        3,
        true,
    )
    .unwrap();
    assert_eq!(
        removal.files,
        [
            nowhere.join("Clip.mp4"),
            state_path(&nowhere.join("Clip.mp4")),
            mdm_engine::naming::part_path(&nowhere.join("Clip.mp4")),
        ]
        .into_iter()
        .chain(mdm_engine::state::legacy_files(&nowhere.join("Clip.mp4")))
        .collect::<Vec<_>>()
    );
    assert!(removal.yt_dlp.is_some());
    removal.run();
    assert!(!nowhere.exists());
}

/// #743: a paired device's link over one already there (Overwrite): the old one's files go
/// beside the window first, then the new one's folder is looked at (its name free again, as
/// when they went on the window's thread) and its start, which waits for them, is told.
#[test]
fn a_replacements_name_is_looked_for_once_the_old_files_are_gone() {
    let dir = tempfile::tempdir().unwrap();
    let at = dir.path().join("file.zip");
    std::fs::write(&at, b"old").unwrap();
    std::fs::write(state_path(&at), b"old").unwrap();
    let mut old = Item::new(1, page(), at.clone(), Category::General);
    old.status = Status::Complete;
    let removal = replacement(&[old], 1, true);
    let (gone, clearing) = std::sync::mpsc::channel();
    assert!(clearing.try_recv().is_err());
    let found = numbered_after(removal, vec![gone], vec![(2, at.clone())]);
    assert_eq!(
        found,
        vec![(2, at.clone(), at.clone())],
        "its name free again"
    );
    assert!(!at.exists() && !state_path(&at).exists());
    assert!(clearing.try_recv().is_ok(), "its start told");
    // Nothing replaced: as `numbered` (a file there takes the name).
    std::fs::write(&at, b"another").unwrap();
    assert_eq!(
        numbered_after(None, Vec::new(), vec![(2, at.clone())]),
        vec![(2, at.clone(), dir.path().join("file (1).zip"))]
    );
}

/// #743: a video's folder is read beside the window (`video_folder`); its name is then picked on the
/// window's thread from what was read and the rows as they are by then (`video_place`): a
/// file of its stem there, or another row's, numbers it on, as `stem_destination` did on
/// the window's thread.
#[test]
fn a_videos_name_is_picked_from_its_folder_read_beside_the_window() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("Video");
    assert!(video_folder(&folder).is_empty());
    assert!(!folder.exists(), "not made here");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("clip.webm"), b"x").unwrap();
    let names = video_folder(&folder);
    assert_eq!(names, ["clip.webm"]);
    assert_eq!(
        video_place(&folder, "Clip.mp4", &names, &[]),
        folder.join("Clip (1).mp4")
    );
    assert_eq!(
        video_place(&folder, "Clip.mp4", &names, &[]),
        mdm_engine::naming::stem_destination(&folder, "Clip.mp4", |_| false)
    );
    // A row added while the folder was read has the next stem: the one after it.
    let row = video_row(1, &folder, "Clip (1).mkv", Status::Queued);
    assert_eq!(
        video_place(&folder, "Clip.mp4", &names, &[row]),
        folder.join("Clip (2).mp4")
    );
    // Another folder's row changes nothing here.
    let far = video_row(2, dir.path(), "Clip (1).mp4", Status::Queued);
    assert_eq!(
        video_place(&folder, "Clip.mp4", &names, &[far]),
        folder.join("Clip (1).mp4")
    );
}
