use super::{StartLook, numbered};
use crate::model::{Category, Item, Status};
use mdm_engine::{Begin, Destination};
use std::path::Path;

fn row(id: u64, dest: &Path) -> Item {
    let url = format!("http://files.test/{id}/file.zip").parse().unwrap();
    let mut i = Item::new(id, url, dest.to_owned(), Category::General);
    i.status = Status::Stopped;
    i
}

fn look(me: &Item, others: &[Item]) -> StartLook {
    StartLook {
        me: me.place_copy(),
        guessed: me.name_from_url,
        fresh: me.number_at_start,
        others: others.iter().map(Item::place_copy).collect(),
        running: Default::default(),
        clearing: Vec::new(),
    }
}

fn file_of(b: &Begin) -> Option<String> {
    match b {
        Begin::Fresh(Destination::File(p)) => {
            Some(p.file_name().unwrap().to_string_lossy().into_owned())
        }
        _ => None,
    }
}

/// #743: what a start used to ask the disk on the window's thread is asked beside it, with
/// the same answers: a resume state there resumes, nothing there starts from scratch where
/// the row is, the address's guess is the server's to name, and a name given without a look
/// at its folder isn't looked at.
#[test]
fn a_start_asks_the_disk_beside_the_window() {
    let dir = tempfile::tempdir().unwrap();
    let at = dir.path().join("file.zip");
    let mut me = row(1, &at);

    // Nothing of it there: from scratch, at its place.
    assert_eq!(
        file_of(&look(&me, &[]).decide()).as_deref(),
        Some("file.zip")
    );

    // Its resume state there: it goes on from it.
    let state = mdm_engine::state::state_path(&at);
    mdm_engine::state::ResumeState::new(
        me.url.clone(),
        me.url.clone(),
        at.clone(),
        9,
        Default::default(),
    )
    .write(&state)
    .unwrap();
    me.downloaded = 4;
    match look(&me, &[]).decide() {
        Begin::Resume(s) => assert_eq!(s, state),
        other => panic!("resumes, not {other:?}"),
    }

    // A name given before its folder was looked at (Download File Info, or added without
    // it): the next free one as the engine starts, nothing read here.
    me.downloaded = 0;
    me.number_at_start = true;
    match look(&me, &[]).decide() {
        Begin::Fresh(Destination::Free(p)) => assert_eq!(p, at),
        other => panic!("numbered as it starts, not {other:?}"),
    }
    std::fs::remove_file(&state).unwrap();

    // The address's guess: into its folder, the server names it.
    me.number_at_start = false;
    me.name_from_url = true;
    match look(&me, &[]).decide() {
        Begin::Fresh(Destination::Dir(d)) => assert_eq!(d, dir.path()),
        other => panic!("into its folder, not {other:?}"),
    }
}

/// A row Dannex 0.6.5 or earlier stopped, its files `file.zip.mdm.json` and
/// `file.zip.mdm.part` (left under those names: its drive was away when the list loaded):
/// its start renames them to `.ddm.json` and `.ddm.part` and goes on from them.
#[test]
fn a_start_resumes_files_under_their_former_names() {
    let dir = tempfile::tempdir().unwrap();
    let at = dir.path().join("file.zip");
    let me = row(3, &at);
    let old_state = dir.path().join("file.zip.mdm.json");
    mdm_engine::state::ResumeState::new(
        me.url.clone(),
        me.url.clone(),
        at.clone(),
        9,
        Default::default(),
    )
    .write(&old_state)
    .unwrap();
    std::fs::write(dir.path().join("file.zip.mdm.part"), b"four").unwrap();
    match look(&me, &[]).decide() {
        Begin::Resume(s) => assert_eq!(s, dir.path().join("file.zip.ddm.json")),
        other => panic!("resumes, not {other:?}"),
    }
    assert_eq!(
        std::fs::read(dir.path().join("file.zip.ddm.part")).unwrap(),
        b"four"
    );
    assert!(!old_state.exists() && !dir.path().join("file.zip.mdm.part").exists());
}

/// #442 beside the window: of two rows of one name the one running keeps it, the one
/// starting takes the next free name; one only waiting doesn't hold it.
#[test]
fn a_start_beside_the_window_leaves_a_name_a_running_row_has() {
    let dir = tempfile::tempdir().unwrap();
    let at = dir.path().join("file.zip");
    let me = row(2, &at);
    let other = row(1, &at);
    let mut l = look(&me, std::slice::from_ref(&other));
    l.running.insert(1);
    assert_eq!(file_of(&l.decide()).as_deref(), Some("file (1).zip"));
    let l = look(&me, &[other]);
    assert_eq!(file_of(&l.decide()).as_deref(), Some("file.zip"));
}

/// Redownload's removal, under way beside the window, is waited for: the old resume state
/// isn't resumed from as it goes.
#[test]
fn a_start_waits_for_redownloads_removal() {
    let dir = tempfile::tempdir().unwrap();
    let at = dir.path().join("file.zip");
    let me = row(1, &at);
    let state = mdm_engine::state::state_path(&at);
    mdm_engine::state::ResumeState::new(
        me.url.clone(),
        me.url.clone(),
        at.clone(),
        9,
        Default::default(),
    )
    .write(&state)
    .unwrap();
    let (gone, clearing) = std::sync::mpsc::channel();
    let remover = std::thread::spawn({
        let state = state.clone();
        move || {
            std::thread::sleep(std::time::Duration::from_millis(150));
            std::fs::remove_file(state).unwrap();
            gone.send(()).unwrap();
        }
    });
    let mut l = look(&me, &[]);
    l.clearing = vec![clearing];
    assert_eq!(file_of(&l.decide()).as_deref(), Some("file.zip"));
    remover.join().unwrap();
}

/// A video row at `dest`, not started.
fn video(id: u64, dest: &Path) -> Item {
    let mut i = row(id, dest);
    i.video = Some(crate::model::VideoSpec {
        selector: "b".into(),
        subs: Default::default(),
    });
    i
}

/// #743: a video's place as it starts is decided off the window's thread
/// (`StartLook::video`), as `claim_place` decided it on the window's: its own, or a stem of
/// its own (#442) when another row there that holds its place (running, or finished with
/// its file there) has its stem, whatever the extension; a file there of the next stem
/// numbers it on.
#[test]
fn a_videos_place_is_decided_beside_the_window() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let me = video(2, &d.join("Clip.mp4"));
    let other = || video(1, &d.join("Clip.webm"));
    assert_eq!(look(&me, &[]).video(), d.join("Clip.mp4"));
    assert_eq!(
        look(&me, &[other()]).video(),
        d.join("Clip.mp4"),
        "one only waiting holds nothing"
    );
    let mut l = look(&me, &[other()]);
    l.running.insert(1);
    assert_eq!(l.video(), d.join("Clip (1).mp4"));
    let mut done = other();
    done.status = Status::Complete;
    std::fs::write(&done.dest, b"x").unwrap();
    std::fs::write(d.join("Clip (1).en.srt"), b"x").unwrap();
    assert_eq!(look(&me, &[done]).video(), d.join("Clip (2).mp4"));
}

/// #743: a video's run starts at once, its place decided and its folder made on the run's
/// own thread (`VideoJob::begin`), never the window's; the row follows the place it took
/// (#442), as a file download's follows the file its start made.
#[test]
fn a_video_row_follows_the_place_its_run_took() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("Video");
    let mut me = video(2, &folder.join("Clip.mp4"));
    let mut l = look(&me, &[video(1, &folder.join("Clip.mp4"))]);
    l.running.insert(1);
    // A yt-dlp that isn't there: the run fails at once, on its own thread (no network).
    let job = mdm_tools::video::VideoJob::begin(
        &dir.path().join("no-yt-dlp.exe"),
        "https://videos.test/watch?v=1",
        "b",
        move || l.video(),
        None,
        None,
        None,
        &Default::default(),
        &Default::default(),
        None,
        None,
    )
    .unwrap();
    me.live = Some(crate::model::Live::new(
        crate::model::Job::Video {
            job,
            url: me.url.clone(),
        },
        0,
    ));
    me.status = Status::Downloading;
    let end = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !me.poll() && std::time::Instant::now() < end {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(me.status, Status::Error, "{:?}", me.error);
    assert_eq!(me.dest, folder.join("Clip (1).mp4"));
    assert_eq!(me.file_name, "Clip (1).mp4");
    assert!(folder.is_dir(), "its folder made on the run's thread");
}

/// #743: a download added without Download File Info shows the numbered name its folder
/// would have given it, as found beside the window: a file there, or a part file, takes the
/// name.
#[test]
fn names_taken_are_found_beside_the_window() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    std::fs::write(d.join("a.zip"), b"x").unwrap();
    std::fs::write(mdm_engine::naming::part_path(&d.join("b.zip")), b"x").unwrap();
    let found = numbered(vec![
        (1, d.join("a.zip")),
        (2, d.join("b.zip")),
        (3, d.join("c.zip")),
    ]);
    assert_eq!(
        found,
        vec![
            (1, d.join("a.zip"), d.join("a (1).zip")),
            (2, d.join("b.zip"), d.join("b (1).zip")),
            (3, d.join("c.zip"), d.join("c.zip")),
        ]
    );
}
