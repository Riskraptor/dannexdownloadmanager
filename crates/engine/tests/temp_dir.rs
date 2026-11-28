//! Options → Save to → Temporary directory (`naming::set_temp_dir`): a download keeps its part
//! file there until it is complete, then the file lands in its folder and nothing is left behind;
//! a download paused before the setting changed brings its part there when it resumes. Its own
//! test program: the setting is the process's, and the other tests' parts stay beside their
//! files.

use mdm_engine::{Destination, Download, Options, Request};
use mdm_labserver::{Config, Faults, LabServer, content};

const SETTLE: std::time::Duration = std::time::Duration::from_secs(20);

fn opts() -> Options {
    Options {
        retry_delay: std::time::Duration::from_millis(50),
        save_interval: std::time::Duration::from_millis(100),
        ..Options::default()
    }
}

fn names(dir: &std::path::Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// One test, in steps: the setting is process-wide.
#[tokio::test(flavor = "multi_thread")]
async fn parts_live_in_the_temporary_directory_until_the_end() {
    let s = LabServer::start(Config {
        faults: Faults::parse("rate=1M").unwrap(),
        ..Config::default()
    })
    .await
    .unwrap();
    let root = tempfile::tempdir().unwrap();
    let (dl_dir, temp) = (
        root.path().join("downloads"),
        root.path().join("temp parts"),
    );
    std::fs::create_dir_all(&dl_dir).unwrap();

    // 1. A new download: its part in the temporary directory while it runs, none beside it.
    mdm_engine::naming::set_temp_dir(Some(temp.clone()));
    let dl = Download::start(
        Request {
            url: s.url("/gen/3M/first.bin?seed=31").parse().unwrap(),
            headers: Default::default(),
            dest: Destination::Dir(dl_dir.clone()),
        },
        opts(),
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let snap = dl.snapshot();
        if snap.downloaded >= 256 << 10 {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "never started");
    }
    let parts = names(&temp);
    assert!(
        parts.len() == 1 && parts[0].starts_with("first.bin.") && parts[0].ends_with(".ddm.part"),
        "{parts:?}"
    );
    assert!(
        !names(&dl_dir).iter().any(|n| n.ends_with(".ddm.part")),
        "{:?}",
        names(&dl_dir)
    );
    let out = dl.wait().await.unwrap();
    let mut want = vec![0; 3 << 20];
    content::fill(31, 0, &mut want);
    assert!(std::fs::read(&out.path).unwrap() == want, "content");
    assert_eq!(names(&dl_dir), ["first.bin"], "only the finished file");
    assert!(names(&temp).is_empty(), "{:?}", names(&temp));

    // 2. Paused beside its file (no temporary directory then), resumed with one: the part moves
    //    there, the data on disk is kept, and nothing is left anywhere but the file.
    mdm_engine::naming::set_temp_dir(None);
    let faults = s.faults();
    s.set_faults(Faults {
        stall_at: Some(2 << 20),
        ..faults.clone()
    });
    let dl = Download::start(
        Request {
            url: s.url("/gen/6M/second.bin?seed=32").parse().unwrap(),
            headers: Default::default(),
            dest: Destination::Dir(dl_dir.clone()),
        },
        opts(),
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let file = loop {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let snap = dl.snapshot();
        if let Some(f) = snap.file.filter(|_| snap.downloaded >= 1 << 20) {
            break f;
        }
        assert!(std::time::Instant::now() < deadline, "never started");
    };
    dl.cancel();
    assert!(dl.wait().await.is_err());
    s.set_faults(faults);
    tokio::time::timeout(SETTLE, s.settled()).await.unwrap();
    let beside = mdm_engine::naming::beside_part_path(&file);
    assert!(beside.exists(), "paused beside its file");
    let state = mdm_engine::state::state_path(&file);
    let had = mdm_engine::state::ResumeState::load(&state)
        .unwrap()
        .downloaded();

    mdm_engine::naming::set_temp_dir(Some(temp.clone()));
    s.clear_log();
    let out = Download::resume(state, Default::default(), opts())
        .unwrap()
        .wait()
        .await
        .unwrap();
    let mut want = vec![0; 6 << 20];
    content::fill(32, 0, &mut want);
    assert!(std::fs::read(&out.path).unwrap() == want, "content");
    tokio::time::timeout(SETTLE, s.settled()).await.unwrap();
    let asked: u64 = s.log().iter().map(|e| e.bytes_sent).sum();
    assert!(
        asked < (6 << 20) - had / 2,
        "the data on disk was kept: {asked} bytes asked for, {had} were there"
    );
    assert_eq!(names(&dl_dir), ["first.bin", "second.bin"]);
    assert!(names(&temp).is_empty(), "{:?}", names(&temp));

    // 3. A temporary directory on another drive (DDM_TEST_OTHER_DRIVE names a folder there): the
    //    finished file is copied over, and its part removed.
    if let Some(other) = std::env::var_os("DDM_TEST_OTHER_DRIVE") {
        let other = std::path::PathBuf::from(other).join("ddm-temp-dir-test");
        let _ = std::fs::remove_dir_all(&other);
        mdm_engine::naming::set_temp_dir(Some(other.clone()));
        let out = Download::start(
            Request {
                url: s.url("/gen/2M/third.bin?seed=33").parse().unwrap(),
                headers: Default::default(),
                dest: Destination::Dir(dl_dir.clone()),
            },
            opts(),
        )
        .wait()
        .await
        .unwrap();
        let mut want = vec![0; 2 << 20];
        content::fill(33, 0, &mut want);
        assert!(std::fs::read(&out.path).unwrap() == want, "content");
        assert!(names(&other).is_empty(), "{:?}", names(&other));
        let _ = std::fs::remove_dir_all(&other);
    }
    mdm_engine::naming::set_temp_dir(None);
}
