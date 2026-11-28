//! Metalink downloads end to end: the lab server's documents (several files, sources with
//! faults of their own, whole-file and piece hashes) read by `mdm_engine::metalink`, and each
//! file downloaded from its sources by the engine, verified byte for byte.

use std::time::Duration;

use mdm_engine::download::State;
use mdm_engine::metalink::{self, File};
use mdm_engine::{Destination, Download, Error, Options, Request};
use mdm_labserver::{Config, LabServer, content};
use url::Url;

const SETTLE: Duration = Duration::from_secs(20);

async fn server() -> LabServer {
    LabServer::start(Config::default()).await.unwrap()
}

fn fast_opts() -> Options {
    Options {
        retry_delay: Duration::from_millis(50),
        ..Options::default()
    }
}

/// The lab's document at `path`, read as a person's Add URL would.
async fn manifest(s: &LabServer, path: &str) -> metalink::Manifest {
    let url: Url = s.url(path).parse().unwrap();
    let bytes = reqwest::get(url.clone())
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    metalink::parse_bytes(&bytes, &url).unwrap()
}

/// File `file` of a manifest downloaded under `dir` from its sources as the app hands them to
/// the engine (`Item::url` the first, the others `Options::mirrors`), with its hashes; the
/// result and the download's log.
async fn download(
    file: &File,
    dir: &std::path::Path,
    opts: Options,
) -> (Result<u64, Error>, Vec<String>) {
    let dest = dir.join(file.relative_path().unwrap());
    std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
    let opts = Options {
        mirrors: file.urls[1..].to_vec(),
        checksum: file.checksum.clone(),
        pieces: file.pieces.clone().map(std::sync::Arc::new),
        ..opts
    };
    let dl = Download::start(
        Request {
            url: file.urls[0].clone(),
            headers: Default::default(),
            dest: Destination::File(dest),
        },
        opts,
    );
    let events = loop {
        let snap = dl.snapshot();
        if matches!(snap.state, State::Done | State::Failed | State::Cancelled) {
            break snap.events.into_iter().map(|e| e.text).collect();
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    (dl.wait().await.map(|o| o.size), events)
}

/// The file at `path` is file `seed` of `size` bytes.
fn assert_content(path: &std::path::Path, seed: u64, size: u64) {
    let data = std::fs::read(path).unwrap();
    let mut want = vec![0; size as usize];
    content::fill(seed, 0, &mut want);
    assert!(data == want, "{} differs from seed {seed}", path.display());
}

/// A multi-file Metalink 4 and the same in Metalink 3: every file in its folder, from both of
/// its sources, its SHA-256 and piece hashes checked.
#[tokio::test]
async fn several_files_come_from_their_sources_with_their_hashes() {
    let s = server().await;
    for name in ["set.meta4", "set.metalink"] {
        s.clear_log();
        let doc = format!(
            "/metalink/{name}?files=3M/one.bin,1M/docs/two.bin,200K/three.bin\
             &seed=11&sources=2&fault2=latency_ms%3D1&pieces=256K&loc2=ke"
        );
        let m = manifest(&s, &doc).await;
        assert_eq!(m.files.len(), 3, "{name}");
        let dir = tempfile::tempdir().unwrap();
        for (i, f) in m.files.iter().enumerate() {
            assert_eq!(f.urls.len(), 2, "{name}: both sources");
            assert_eq!(f.sources[1].location.as_deref(), Some("KE"));
            assert!(f.checksum.is_some() && f.pieces.is_some(), "{name}: hashes");
            let (res, events) = download(f, dir.path(), fast_opts()).await;
            let size = res.unwrap_or_else(|e| panic!("{name} file {i}: {e}; {events:?}"));
            assert_eq!(Some(size), f.size);
            assert_content(
                &dir.path().join(f.relative_path().unwrap()),
                11 + i as u64,
                size,
            );
            assert!(
                events
                    .iter()
                    .any(|e| e.starts_with("Checksum verified (SHA-256)")),
                "{name}: {events:?}"
            );
        }
        assert!(dir.path().join("docs").join("two.bin").is_file());
        tokio::time::timeout(SETTLE, s.settled()).await.unwrap();
        // The 3 MiB file's ranges went to the mirror too (its own query), never with the main
        // address's validators.
        let log = s.log();
        let mirror: Vec<_> = log
            .iter()
            .filter(|e| e.query.as_deref().is_some_and(|q| q.contains("latency_ms")))
            .collect();
        assert!(!mirror.is_empty(), "{name}: the mirror served ranges");
        assert!(
            mirror
                .iter()
                .all(|e| e.status == 206 && e.if_range.is_none()),
            "{name}: ranges of the mirror, without If-Range"
        );
    }
}

/// One mirror refuses (404), another isn't there at all: both are left out and the download
/// finishes from the sources that work, instead of failing on a mirror's answer.
#[tokio::test]
async fn failing_mirrors_are_left_out() {
    let s = server().await;
    let m = manifest(
        &s,
        "/metalink/one.meta4?files=6M/big.bin&seed=21&sources=3\
         &fault2=refuse%3D404&fault3=latency_ms%3D1",
    )
    .await;
    let mut file = m.files[0].clone();
    // A mirror nobody answers at: a loopback port just released.
    let dead = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = dead.local_addr().unwrap().port();
    drop(dead);
    file.urls.insert(
        1,
        format!("http://127.0.0.1:{port}/big.bin").parse().unwrap(),
    );
    let dir = tempfile::tempdir().unwrap();
    let (res, events) = download(&file, dir.path(), fast_opts()).await;
    let size = res.unwrap_or_else(|e| panic!("{e}; {events:?}"));
    assert_content(&dir.path().join("big.bin"), 21, size);
    let dropped: Vec<_> = events
        .iter()
        .filter(|e| e.starts_with("Metalink mirror") && e.contains("not used"))
        .collect();
    assert!(
        dropped.iter().any(|e| e.contains("404")),
        "the refusing mirror: {events:?}"
    );
    assert!(
        dropped
            .iter()
            .any(|e| e.starts_with(&format!("Metalink mirror 127.0.0.1:{port}:"))),
        "the missing mirror: {events:?}"
    );
    assert_eq!(dropped.len(), 2, "the working mirror stays in: {events:?}");
}

/// The main address damages one byte (in the first piece, which only it sends): the piece
/// check finds that piece, leaves the main address out, and the mirror sends it again.
#[tokio::test]
async fn a_damaged_piece_is_fetched_again_from_another_source() {
    let s = server().await;
    let m = manifest(
        &s,
        "/metalink/one.meta4?files=8M/damaged.bin&seed=31&sources=2\
         &fault1=corrupt_at%3D1000&fault2=latency_ms%3D1&pieces=256K",
    )
    .await;
    let dir = tempfile::tempdir().unwrap();
    let (res, events) = download(&m.files[0], dir.path(), fast_opts()).await;
    let size = res.unwrap_or_else(|e| panic!("{e}; {events:?}"));
    assert_content(&dir.path().join("damaged.bin"), 31, size);
    assert!(
        events
            .iter()
            .any(|e| e == "1 of 32 pieces failed their SHA-256 check"),
        "{events:?}"
    );
    assert!(
        events
            .iter()
            .any(|e| e.starts_with("The main address sent data")),
        "{events:?}"
    );
    assert!(
        events
            .iter()
            .any(|e| e.starts_with("Checksum verified (SHA-256)")),
        "the whole file checks out at the end: {events:?}"
    );
    // The first piece came again, from the mirror.
    tokio::time::timeout(SETTLE, s.settled()).await.unwrap();
    assert!(s.log().iter().any(|e| {
        e.query.as_deref().is_some_and(|q| q.contains("latency_ms"))
            && e.range
                .as_deref()
                .is_some_and(|r| r.starts_with("bytes=0-"))
    }));
}

/// A file only one source has, damaged there: fetched again a few times, then the download
/// fails with a checksum mismatch and keeps its data.
#[tokio::test]
async fn a_piece_no_source_has_whole_fails_after_the_rounds() {
    let s = server().await;
    let m = manifest(
        &s,
        "/metalink/one.meta4?files=2M/bad.bin&seed=41&hash=0&pieces=256K&fault1=corrupt_at%3D600000",
    )
    .await;
    assert!(m.files[0].checksum.is_none());
    let dir = tempfile::tempdir().unwrap();
    let (res, events) = download(&m.files[0], dir.path(), fast_opts()).await;
    assert!(
        matches!(res, Err(Error::ChecksumMismatch("SHA-256"))),
        "{res:?}"
    );
    let failed = events
        .iter()
        .filter(|e| e.ends_with("pieces failed their SHA-256 check"))
        .count();
    assert_eq!(failed, 4, "the first check and 3 rounds: {events:?}");
    assert!(
        mdm_engine::naming::has_part(&dir.path().join("bad.bin")),
        "the data is kept"
    );
}

/// The pieces all check out but the whole file's hash doesn't: no piece can be blamed, the
/// download fails (and keeps its data).
#[tokio::test]
async fn good_pieces_and_a_wrong_whole_file_hash_fail() {
    let s = server().await;
    let m = manifest(
        &s,
        "/metalink/one.meta4?files=1M/w.bin&seed=51&pieces=256K&wrong=1",
    )
    .await;
    let dir = tempfile::tempdir().unwrap();
    let (res, _) = download(&m.files[0], dir.path(), fast_opts()).await;
    assert!(
        matches!(res, Err(Error::ChecksumMismatch("SHA-256"))),
        "{res:?}"
    );
}

/// Piece hashes alone are enough to use the mirrors: a main address that never answers is
/// replaced by the mirror from the first request on.
#[tokio::test]
async fn piece_hashes_alone_let_a_mirror_rescue_a_dead_main_address() {
    let s = server().await;
    let m = manifest(
        &s,
        "/metalink/one.meta4?files=2M/p.bin&seed=61&hash=0&pieces=512K",
    )
    .await;
    let mut file = m.files[0].clone();
    assert!(file.checksum.is_none() && file.pieces.is_some());
    let dead = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = dead.local_addr().unwrap().port();
    drop(dead);
    file.urls
        .insert(0, format!("http://127.0.0.1:{port}/p.bin").parse().unwrap());
    let dir = tempfile::tempdir().unwrap();
    let (res, events) = download(&file, dir.path(), fast_opts()).await;
    let size = res.unwrap_or_else(|e| panic!("{e}; {events:?}"));
    assert_content(&dir.path().join("p.bin"), 61, size);
    assert!(
        events
            .iter()
            .any(|e| e == "All 4 pieces verified (SHA-256)"),
        "{events:?}"
    );
}
