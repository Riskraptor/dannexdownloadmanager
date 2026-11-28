//! Web seeds (BEP 19) end to end: torrents whose `url-list` is the lab server download with no
//! peer at all, from the web seed alone, which DDM serves to librqbit as a peer of its own
//! (`mdm_torrent::webseed`); librqbit checks every piece. A web seed's damaged copy never
//! finishes the torrent.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use mdm_labserver::{Config as Lab, LabServer, content};
use mdm_torrent::{Config, Phase, Torrents, librqbit};

/// File `seed`'s first `len` bytes (the lab's generator).
fn data(seed: u64, len: usize) -> Vec<u8> {
    let mut v = vec![0; len];
    content::fill(seed, 0, &mut v);
    v
}

/// A torrent of `path` (a file or a folder), made by librqbit.
async fn make_torrent(path: &Path, name: &str, piece_length: u32) -> Vec<u8> {
    let spawner = librqbit::spawn_utils::BlockingSpawner::new(1);
    let made = librqbit::create_torrent(
        path,
        librqbit::CreateTorrentOptions {
            name: Some(name),
            trackers: Vec::new(),
            piece_length: Some(piece_length),
        },
        &spawner,
    )
    .await
    .unwrap();
    made.as_bytes().unwrap().to_vec()
}

/// A session of its own: no DHT, no trackers, everything on this computer (web seeds there
/// too, which a real session never asks).
async fn session(state: PathBuf) -> Torrents {
    Torrents::start(Config {
        dht: false,
        offline: true,
        ..Config::new(state)
    })
    .await
    .unwrap()
}

/// Waits until torrent `hash` is finished, or `wait` has passed: its last stats.
async fn wait_finished(t: &Torrents, hash: &str, wait: Duration) -> mdm_torrent::Stats {
    let until = Instant::now() + wait;
    loop {
        let s = t.stats(hash).unwrap();
        if s.phase == Phase::Finished || Instant::now() > until {
            return s;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// A multi-file torrent (its files under the web seed's address, in a folder of the torrent's
/// name, BEP 19) and a single-file one (the web seed's address is the file's): both download
/// whole from the web seed, with no peer, byte for byte.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn torrents_download_from_their_web_seed_alone() {
    let files = tempfile::tempdir().unwrap();
    let set = files.path().join("set");
    std::fs::create_dir_all(set.join("docs")).unwrap();
    let (a, b) = (data(7, 700_001), data(8, 123_457));
    std::fs::write(set.join("a.bin"), &a).unwrap();
    std::fs::write(set.join("docs").join("b.bin"), &b).unwrap();
    let one = data(9, 333_333);
    std::fs::write(files.path().join("one.iso"), &one).unwrap();
    let lab = LabServer::start(Lab {
        files: Some(files.path().to_owned()),
        ..Lab::default()
    })
    .await
    .unwrap();

    let multi = make_torrent(&set, "set", 32 * 1024).await;
    let multi = mdm_torrent::webseed::with_web_seeds(&multi, &[lab.url("/files/")]).unwrap();
    assert_eq!(mdm_torrent::webseed::listed(&multi), [lab.url("/files/")]);
    let single = make_torrent(&files.path().join("one.iso"), "one.iso", 16 * 1024).await;
    let single =
        mdm_torrent::webseed::with_web_seeds(&single, &[lab.url("/files/one.iso")]).unwrap();

    let tmp = tempfile::tempdir().unwrap();
    let t = session(tmp.path().join("state")).await;
    let out = tmp.path().join("out");
    let multi_hash = t.add(&multi, &out, None, Vec::new(), false).await.unwrap();
    let single_hash = t
        .add(&single, &out.join("single"), None, Vec::new(), false)
        .await
        .unwrap();
    for hash in [&multi_hash, &single_hash] {
        let s = wait_finished(&t, hash, Duration::from_secs(90)).await;
        assert_eq!(s.phase, Phase::Finished, "{s:?}");
    }
    assert!(std::fs::read(out.join("a.bin")).unwrap() == a);
    assert!(std::fs::read(out.join("docs").join("b.bin")).unwrap() == b);
    assert!(std::fs::read(out.join("single").join("one.iso")).unwrap() == one);
    // The web seed was asked in ranges, at the addresses BEP 19 makes.
    let log = lab.log();
    for path in [
        "/files/set/a.bin",
        "/files/set/docs/b.bin",
        "/files/one.iso",
    ] {
        assert!(
            log.iter()
                .any(|e| e.path == path && e.status == 206 && e.range.is_some()),
            "{path} asked in ranges"
        );
    }
    t.stop().await;
}

/// A real torrent with web seeds on the internet: a small public-domain book at the Internet
/// Archive, whose torrents list `https://archive.org/download/` and its storage servers in
/// `url-list`. No trackers, no DHT (the session's offline mode, listening on this computer
/// only): its web seeds are its only source. Needs the internet: `cargo test -p mdm-torrent
/// --test webseed -- --ignored`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "downloads from archive.org"]
async fn a_public_torrent_downloads_from_its_web_seeds() {
    const TORRENT: &str = "https://archive.org/download/pierreandhispeop06178gut/pierreandhispeop06178gut_archive.torrent";
    let bytes = reqwest::get(TORRENT)
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap()
        .to_vec();
    let seeds = mdm_torrent::webseed::listed(&bytes);
    assert!(
        seeds.iter().any(|s| s.starts_with("https://archive.org/")),
        "{seeds:?}"
    );
    let tmp = tempfile::tempdir().unwrap();
    let t = session(tmp.path().join("state")).await;
    let out = tmp.path().join("out");
    let hash = t.add(&bytes, &out, None, Vec::new(), false).await.unwrap();
    let s = wait_finished(&t, &hash, Duration::from_secs(180)).await;
    assert_eq!(s.phase, Phase::Finished, "{s:?}");
    assert!(s.done > 0 && s.done == s.total, "{s:?}");
    t.stop().await;
}

/// A web seed whose copy has one damaged byte: librqbit's check refuses that piece every time,
/// so the torrent never finishes from it (the other pieces do come).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_damaged_web_seed_never_finishes_a_torrent() {
    let files = tempfile::tempdir().unwrap();
    let good = data(11, 200_000);
    let src = files.path().join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("d.bin"), &good).unwrap();
    let torrent = make_torrent(&src.join("d.bin"), "d.bin", 16 * 1024).await;
    let mut bad = good.clone();
    bad[100_000] ^= 0xff;
    std::fs::write(files.path().join("d.bin"), &bad).unwrap();
    let lab = LabServer::start(Lab {
        files: Some(files.path().to_owned()),
        ..Lab::default()
    })
    .await
    .unwrap();
    let torrent =
        mdm_torrent::webseed::with_web_seeds(&torrent, &[lab.url("/files/d.bin")]).unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let t = session(tmp.path().join("state")).await;
    let hash = t
        .add(&torrent, &tmp.path().join("out"), None, Vec::new(), false)
        .await
        .unwrap();
    // Every piece but the damaged one (piece 6 of 16 KiB pieces) comes and checks out: waited
    // for as they arrive (a slow runner took over 20 s for the last good one), up to a minute.
    let damaged = [100_000 / 16_384];
    let until = Instant::now() + Duration::from_secs(60);
    let pieces = loop {
        let pieces = t.pieces(&hash);
        let missing: Vec<usize> = (0..pieces.len()).filter(|i| !pieces[*i]).collect();
        // (No pieces yet: the torrent is still starting.)
        if (pieces.len() == 13 && missing == damaged) || Instant::now() > until {
            break pieces;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert_eq!(pieces.len(), 13);
    let missing: Vec<usize> = (0..pieces.len()).filter(|i| !pieces[*i]).collect();
    assert_eq!(missing, damaged, "{pieces:?}");
    // And the damaged one never finishes the torrent, however long the seed is asked.
    let s = wait_finished(&t, &hash, Duration::from_secs(3)).await;
    assert_ne!(s.phase, Phase::Finished, "{s:?}");
    t.stop().await;
}
