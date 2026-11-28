//! #743, the paths the first part left: what the window's thread asked the disk, asked beside it.

use super::*;
use crate::app::watch::clipboard_catches;
use mdm_engine::{Begin, Destination};
use std::path::Path;
use std::time::Duration;

fn row(id: u64, dest: &Path, status: Status) -> Item {
    let url = format!("http://files.test/{id}/file.zip").parse().unwrap();
    let mut i = Item::new(id, url, dest.to_owned(), Category::General);
    i.status = status;
    i
}

/// A resume state at `dest`, as a download stopped part-way leaves it.
fn resume_state(i: &Item) {
    mdm_engine::state::ResumeState::new(
        i.url.clone(),
        i.url.clone(),
        i.dest.clone(),
        9,
        Default::default(),
    )
    .write(&state_path(&i.dest))
    .unwrap();
}

fn look(me: &Item, clearing: Vec<std::sync::mpsc::Receiver<()>>) -> StartLook {
    StartLook {
        me: me.place_copy(),
        guessed: me.name_from_url,
        fresh: me.number_at_start,
        others: Vec::new(),
        running: Default::default(),
        clearing,
    }
}

/// Delete with its files: what goes is known on the window's thread without the disk
/// (`files_of`: a finished file too, else only its part and state files), and removed beside
/// the window (`delete_files`), the row having left the list at once (`remove_download`
/// takes it off after handing the files over). A download added again under the same name
/// and started while they go waits for them (`Removing`, `StartLook::clearing`): the old
/// resume state isn't resumed from, it starts afresh there; then nothing is left to wait for.
#[test]
fn a_delete_with_files_goes_beside_the_window_and_a_readd_waits_for_it() {
    let dir = tempfile::tempdir().unwrap();
    let at = dir.path().join("file.zip");
    let old = row(1, &at, Status::Complete);
    resume_state(&old);
    std::fs::write(&at, b"old").unwrap();
    std::fs::write(mdm_engine::naming::part_path(&at), b"old").unwrap();
    let files = files_of(&at, true);
    assert_eq!(
        files,
        [
            at.clone(),
            state_path(&at),
            mdm_engine::naming::part_path(&at)
        ]
        .into_iter()
        .chain(mdm_engine::state::legacy_files(&at))
        .collect::<Vec<_>>()
    );
    assert_eq!(files_of(&at, false), files[1..], "an unfinished file stays");
    let removing = Removing::of(&files, None);
    assert!(removing.under_way());
    let done = removing.done();
    // The same address added again, its name free on the list: it starts while they go.
    let again = row(2, &at, Status::Stopped);
    let waits = waits_for(
        std::slice::from_ref(&removing),
        at.parent(),
        &Writes::of(&again),
    );
    assert_eq!(waits.len(), 1, "its start waits");
    let far = dir.path().join("elsewhere").join("file.zip");
    for (folder, writes) in [
        (at.parent(), Writes::Named("other.zip".into())),
        (far.parent(), Writes::Named("file.zip".into())),
    ] {
        assert!(waits_for(std::slice::from_ref(&removing), folder, &writes).is_empty());
    }
    let gone = files.clone();
    let remover = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        delete_files(&gone, gone.first().map(|f| f.as_path()), "file.zip");
        drop(done);
    });
    match look(&again, waits).decide() {
        Begin::Fresh(Destination::File(p)) => assert_eq!(p, at),
        other => panic!("afresh at its place once they are gone, not {other:?}"),
    }
    remover.join().unwrap();
    assert!(files.iter().all(|f| !f.exists()), "every file gone");
    assert!(!removing.under_way());
    assert!(
        removing.wait(dir.path(), &Writes::Folder).is_none(),
        "gone: nothing to wait for"
    );
    // One that can't go (here a folder in its place) is said, its row gone all the same.
    std::fs::create_dir_all(at.join("inside")).unwrap();
    let _ = crate::dialogs::told();
    delete_files(&files_of(&at, true), Some(&at), "file.zip");
    let told = crate::dialogs::told();
    assert!(
        told.iter()
            .any(|(t, m)| *t == "Delete" && m.contains("file.zip")),
        "{told:?}"
    );
}

/// What a start writes, and so which removals it waits for: a file its name and the names
/// after it and a dot (its part and state files), a video its stem and a dot (yt-dlp takes any
/// `<stem>.*`), a name the server gives anything in its folder; letter case aside, never
/// another folder's. A video replaced with its yt-dlp files (Overwrite) is waited for by a
/// file of its stem too.
#[test]
fn a_start_waits_only_for_removals_among_what_it_writes() {
    let d = Path::new("Downloads");
    let r = Removing::of(&files_of(&d.join("Clip.MP4"), true), None);
    let hits = |w: Writes| r.hits(d, &w);
    assert!(hits(Writes::Named("clip.mp4".into())));
    assert!(hits(Writes::Stem("clip".into())));
    assert!(hits(Writes::Folder));
    assert!(!hits(Writes::Named("clip.mp4.zip".into())));
    assert!(!hits(Writes::Named("clip (1).mp4".into())));
    assert!(!hits(Writes::Stem("clip (1)".into())));
    assert!(!r.hits(&d.join("sub"), &Writes::Folder), "another folder");
    let video = Removing::of(&files_of(&d.join("Clip.webm"), false), Some("Clip"));
    assert!(
        video.hits(d, &Writes::Named("clip.mp4".into())),
        "a yt-dlp file"
    );
    assert!(video.hits(d, &Writes::Stem("clip".into())));
    assert!(video.hits(d, &Writes::Stem("clip.part1".into())));
    assert!(!video.hits(d, &Writes::Named("clipper.mp4".into())));
    let mut v = row(1, &d.join("Clip.mp4"), Status::Stopped);
    assert_eq!(Writes::of(&v), Writes::Named("clip.mp4".into()));
    v.name_from_url = true;
    assert_eq!(Writes::of(&v), Writes::Folder, "the server names it");
    v.video = Some(crate::model::VideoSpec {
        selector: "b".into(),
        subs: Default::default(),
    });
    assert_eq!(Writes::of(&v), Writes::Stem("clip".into()));
}

/// A removal that ends without finishing its work (a panic on its thread) still tells the
/// starts waiting for it: none waits for ever.
#[test]
fn a_removal_that_panics_never_leaves_a_start_waiting() {
    let r = Removing::of(&[PathBuf::from("d").join("a.zip")], None);
    let wait = r.wait(Path::new("d"), &Writes::Folder).unwrap();
    let done = r.done();
    let t = std::thread::spawn(move || {
        let _done = done;
        panic!("the removal's thread");
    });
    assert!(t.join().is_err());
    assert_eq!(wait.recv_timeout(Duration::from_secs(5)), Ok(()));
    assert!(!r.under_way());
}

/// A playlist's folder is read beside the window (`naming::names_in`); its names are then
/// picked on the window's thread from what was read and the rows as they are by then
/// (`playlist_stems`), as `Stems::new` picked them on the window's thread: a file of the stem
/// there, or another row's, numbers it on. The pick never reads the disk: a file written, or
/// the folder gone, after the read changes nothing.
#[test]
fn a_playlists_names_come_from_its_folder_read_beside_the_window() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("Playlist");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("talk.webm"), b"x").unwrap();
    let there = mdm_engine::naming::names_in(&folder);
    assert_eq!(there, ["talk.webm"]);
    let mut intro = row(1, &folder.join("Intro.mkv"), Status::Queued);
    intro.video = Some(crate::model::VideoSpec {
        selector: "b".into(),
        subs: Default::default(),
    });
    let items = [intro];
    let names = ["Talk.mp4", "Talk.mp4", "Intro.mp4", "Outro.mp4"];
    let mut window = playlist_stems(&folder, &there, &items);
    let mut before = mdm_engine::naming::Stems::new(&folder, ["Intro".to_owned()]);
    let picked: Vec<PathBuf> = names.iter().map(|n| window.pick(n)).collect();
    let had: Vec<PathBuf> = names.iter().map(|n| before.pick(n)).collect();
    assert_eq!(picked, had);
    assert_eq!(
        picked,
        [
            folder.join("Talk (1).mp4"),
            folder.join("Talk (2).mp4"),
            folder.join("Intro (1).mp4"),
            folder.join("Outro.mp4"),
        ]
    );
    // After the read: the folder's files gone, a row elsewhere; the same names.
    std::fs::remove_dir_all(&folder).unwrap();
    let far = row(2, &dir.path().join("Outro.mkv"), Status::Queued);
    let mut again = playlist_stems(&folder, &there, &[items[0].place_copy(), far]);
    assert_eq!(again.pick("Talk.mp4"), folder.join("Talk (1).mp4"));
    assert_eq!(again.pick("Outro.mp4"), folder.join("Outro.mp4"));
}

/// A command's `.torrent` path is known as one without the disk (`torrent_path`): the same
/// answer whether the file is there or not, its quotes taken off; whether it is a file is
/// asked beside the window (`Beside::TorrentPath`). A magnet link, or any other address, is
/// no path.
#[test]
fn a_commands_torrent_path_is_looked_at_beside_the_window() {
    let dir = tempfile::tempdir().unwrap();
    let there = dir.path().join("Linux ISO.torrent");
    std::fs::write(&there, b"d4:infod4:name3:isoee").unwrap();
    let gone = dir.path().join("gone.TORRENT");
    for p in [&there, &gone] {
        let s = p.display().to_string();
        assert_eq!(torrent_path(&s), Some(p.clone()));
        assert_eq!(torrent_path(&format!("\"{s}")), Some(p.clone()));
    }
    assert!(
        there.is_file() && !gone.is_file(),
        "the look beside the window tells them apart"
    );
    assert_eq!(torrent_path("magnet:?xt=urn:btih:abc"), None);
    assert_eq!(torrent_path("https://files.test/a.zip"), None);
    // A web address of one is a path to look at too, as before (it isn't a file: the command
    // goes on as an address, Add torrent fetching it, #39).
    assert!(torrent_path("https://files.test/a.torrent").is_some_and(|p| !p.is_file()));
}

/// The clipboard monitor takes a copied magnet link as it takes a .torrent's address: while
/// torrents are among its types (they are by default), never otherwise.
#[test]
fn the_clipboard_takes_magnet_links_with_torrents() {
    let mut s = Settings::default();
    let magnet = "magnet:?xt=urn:btih:08ada5a7a6183aae1e09d831df6748d566095a10&dn=Sintel";
    assert!(clipboard_catches(&s, magnet));
    assert!(clipboard_catches(&s, "https://files.test/sintel.torrent"));
    assert!(clipboard_catches(&s, "https://files.test/a.zip"));
    assert!(!clipboard_catches(&s, "https://files.test/page.html"));
    assert!(
        !clipboard_catches(&s, "magnet:xt=urn:btih:abc"),
        "not a magnet link"
    );
    s.auto_types = "ZIP RAR".into();
    assert!(!clipboard_catches(&s, magnet));
    assert!(clipboard_catches(&s, "https://files.test/a.zip"));
}

/// What is dropped is sorted beside the window (`sort_dropped`), as the window sorted it on
/// its own thread: lists of addresses read (one that can't be read says why), addresses,
/// then the rest in the order dropped, a magnet link or a .torrent file there marked to open
/// in Add torrent, anything else (a .torrent that isn't there, a picture) to be said.
#[test]
fn what_is_dropped_is_sorted_beside_the_window() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let list = d.join("links.TXT");
    std::fs::write(
        &list,
        "https://files.test/a.zip\r\nhttps://files.test/b.zip\r\n",
    )
    .unwrap();
    let torrent = d.join("iso.torrent");
    std::fs::write(&torrent, b"d4:infod4:name3:isoee").unwrap();
    // Metalink files: read (their files, or why not); one that isn't there is the rest's.
    let meta4 = d.join("set.meta4");
    std::fs::write(
        &meta4,
        r#"<metalink><file name="a.bin"><url>https://files.test/a.bin</url></file>
               <file name="b.bin"><url>https://files.test/b.bin</url></file></metalink>"#,
    )
    .unwrap();
    let broken = d.join("broken.metalink");
    std::fs::write(&broken, "<html>").unwrap();
    let s = |p: &Path| p.display().to_string();
    let sorted = sort_dropped(vec![
        s(&d.join("picture.png")),
        "https://files.test/c.zip".into(),
        s(&torrent),
        s(&list),
        "magnet:?xt=urn:btih:abc".into(),
        s(&d.join("not there.torrent")),
        s(&meta4),
        s(&broken),
        s(&d.join("gone.meta4")),
    ]);
    assert_eq!(sorted.metalinks.len(), 2);
    assert_eq!(sorted.metalinks[0].0, s(&meta4));
    assert_eq!(sorted.metalinks[0].1.as_ref().unwrap().files.len(), 2);
    assert!(sorted.metalinks[1].1.is_err());
    assert_eq!(sorted.links, ["https://files.test/c.zip"]);
    assert_eq!(sorted.lists.len(), 1);
    let (name, read) = &sorted.lists[0];
    assert_eq!(name, "links.TXT");
    let urls: Vec<&str> = read
        .as_ref()
        .unwrap()
        .iter()
        .map(|i| i.url.as_str())
        .collect();
    assert_eq!(
        urls,
        ["https://files.test/a.zip", "https://files.test/b.zip"]
    );
    assert_eq!(
        sorted.other,
        [
            (s(&d.join("picture.png")), false),
            (s(&torrent), true),
            ("magnet:?xt=urn:btih:abc".to_owned(), true),
            (s(&d.join("not there.torrent")), false),
            (s(&d.join("gone.meta4")), false),
        ]
    );
    // Tasks → Import's file, read beside the window too: one that isn't there says why.
    let (name, read) = read_list(&d.join("gone.txt"));
    assert_eq!(name, "gone.txt");
    assert!(read.is_err());
}

/// The browser's quality menu takes yt-dlp and ffmpeg as last found beside the window
/// (`videodlg::found_tools`): not looked at yet, or yt-dlp still coming (DDM gets it by
/// itself), the Download video window does it (`use-window`, an answer the extension already
/// follows); no data folder to put it in, `no-ytdlp`; else its program, and the merged
/// qualities with ffmpeg.
#[test]
fn the_quality_menu_takes_the_helpers_found_beside_the_window() {
    use ipc::api::lookup;
    let tool = |exe: &str| mdm_tools::Installed {
        version: "2026.10.26".into(),
        exe: PathBuf::from(exe),
    };
    let dir = Some(PathBuf::from("tools"));
    assert_eq!(lookup_with(None), Err(lookup::USE_WINDOW));
    assert_eq!(
        lookup_with(Some((dir.clone(), None, Some(tool("ffmpeg"))))),
        Err(lookup::USE_WINDOW)
    );
    assert_eq!(lookup_with(Some((None, None, None))), Err(lookup::NO_YTDLP));
    assert_eq!(
        lookup_with(Some((dir.clone(), Some(tool("yt-dlp")), None))),
        Ok((PathBuf::from("yt-dlp"), false))
    );
    assert_eq!(
        lookup_with(Some((dir, Some(tool("yt-dlp")), Some(tool("ffmpeg"))))),
        Ok((PathBuf::from("yt-dlp"), true))
    );
}

/// File Properties' new address and "Through…" go into the resume state beside the window
/// (`follow_state`, the row's next start waiting for it): the download continues from the new
/// address; none there is left alone.
#[test]
fn properties_rewrite_the_resume_state_beside_the_window() {
    let dir = tempfile::tempdir().unwrap();
    let at = dir.path().join("file.zip");
    let i = row(1, &at, Status::Stopped);
    resume_state(&i);
    let state = state_path(&at);
    let new: url::Url = "https://mirror.test/file.zip".parse().unwrap();
    follow_state(&state, false, Some(&new));
    let saved = mdm_engine::state::ResumeState::load(&state).unwrap();
    assert_eq!((saved.url.clone(), saved.final_url), (new.clone(), new));
    follow_state(&dir.path().join("none.ddm.json"), true, None);
    assert!(!dir.path().join("none.ddm.json").exists());
}
