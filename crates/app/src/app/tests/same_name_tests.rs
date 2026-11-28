use super::{Item, place_to_take_by};
use crate::model::{Category, Status};
use std::path::{Path, PathBuf};

/// `place_to_take_by` as the window's rows say it (what a start's look sees, copied).
fn place_to_take(item: &Item, items: &[Item]) -> Option<PathBuf> {
    place_to_take_by(item, items, Item::holds_place)
}

fn row(id: u64, dest: &Path, status: Status, downloaded: u64) -> Item {
    let url = format!("http://files.test/{id}/master.mp4")
        .parse()
        .unwrap();
    let mut i = Item::new(id, url, dest.to_owned(), Category::Video);
    i.status = status;
    i.downloaded = downloaded;
    i
}

fn name(p: Option<std::path::PathBuf>) -> Option<String> {
    p.map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
}

/// #442: of two rows waiting under one name the first to start keeps it; the other goes to
/// the next free one, not to the same file. A row with its own data never moves.
#[test]
fn a_row_leaves_a_name_another_row_holds() {
    let dir = tempfile::tempdir().unwrap();
    let at = dir.path().join("master.mp4");
    let waiting = |id| row(id, &at, Status::Queued, 0);
    let free = Some("master (1).mp4");

    // Both only waiting: nobody has it yet, the one starting stays.
    let items = [waiting(1), waiting(2)];
    assert_eq!(place_to_take(&items[0], &items), None);

    // The other is part-way (its pieces and resume state are at that name).
    let items = [row(1, &at, Status::Stopped, 500), waiting(2)];
    assert_eq!(name(place_to_take(&items[1], &items)).as_deref(), free);
    // And the part-way one resumes where it is, whoever else waits for the name.
    assert_eq!(place_to_take(&items[0], &items), None);

    // The other finished, its file there.
    std::fs::write(&at, b"x").unwrap();
    let items = [row(1, &at, Status::Complete, 1), waiting(2)];
    assert_eq!(name(place_to_take(&items[1], &items)).as_deref(), free);
    // Finished, but its file is gone: the name is free again.
    std::fs::remove_file(&at).unwrap();
    assert_eq!(place_to_take(&items[1], &items), None);

    // A crash left the list's count at nothing, but the resume state says whose the pieces
    // are: that row stays and resumes, the other leaves.
    let state = mdm_engine::state::state_path(&at);
    let mut items = [waiting(1), waiting(2)];
    let url = items[0].url.clone();
    mdm_engine::state::ResumeState::new(url.clone(), url, at.clone(), 9, Default::default())
        .write(&state)
        .unwrap();
    assert_eq!(place_to_take(&items[0], &items), None);
    assert_eq!(name(place_to_take(&items[1], &items)).as_deref(), free);
    items[0].status = Status::Stopped;
    assert_eq!(name(place_to_take(&items[1], &items)).as_deref(), free);
    std::fs::remove_file(&state).unwrap();

    // Another folder's row of that name is no clash.
    let other = tempfile::tempdir().unwrap();
    let there = other.path().join("master.mp4");
    let items = [row(1, &there, Status::Stopped, 5), waiting(2)];
    assert_eq!(place_to_take(&items[1], &items), None);
}

/// A video's stem has to be free whatever the extension: yt-dlp would take another file of
/// that stem for its own and call the video downloaded.
#[test]
fn a_video_leaves_a_stem_another_row_holds() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let mut video = row(2, &d.join("master.mp4"), Status::Queued, 0);
    video.video = Some(crate::model::VideoSpec {
        selector: "b".into(),
        subs: Default::default(),
    });
    let items = [row(1, &d.join("Master.webm"), Status::Stopped, 9), video];
    assert_eq!(
        name(place_to_take(&items[1], &items)).as_deref(),
        Some("master (1).mp4")
    );
}
