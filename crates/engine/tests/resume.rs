//! Resume, made robust (#4): the network pulled out mid-download and put back, a server that
//! stops sending parts of the file. Every case ends in the right file, byte for byte against the
//! lab server's generator, and never comes again from the start while a resume is possible.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mdm_engine::download::State;
use mdm_engine::{Destination, Download, Options, Request};
use mdm_labserver::{Config, Faults, LabServer, content};
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

const SETTLE: Duration = Duration::from_secs(20);

/// A network cable between the engine and the lab server: a proxy on a port of its own that
/// can be pulled out (every connection through it broken at once, new ones refused, as when the
/// network goes) and put back (on the same port).
struct Cable {
    addr: SocketAddr,
    to: SocketAddr,
    inner: Arc<Mutex<Plugged>>,
}

#[derive(Default)]
struct Plugged {
    accept: Option<JoinHandle<()>>,
    pipes: Vec<JoinHandle<()>>,
}

impl Cable {
    async fn new(to: SocketAddr) -> Cable {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let cable = Cable {
            addr,
            to,
            inner: Default::default(),
        };
        cable.serve(listener);
        cable
    }

    fn serve(&self, listener: TcpListener) {
        let (to, inner) = (self.to, self.inner.clone());
        let pipes = self.inner.clone();
        let accept = tokio::spawn(async move {
            loop {
                let Ok((mut client, _)) = listener.accept().await else {
                    return;
                };
                let pipe = tokio::spawn(async move {
                    if let Ok(mut server) = TcpStream::connect(to).await {
                        let _ = copy_bidirectional(&mut client, &mut server).await;
                    }
                });
                pipes.lock().unwrap().pipes.push(pipe);
            }
        });
        inner.lock().unwrap().accept = Some(accept);
    }

    /// Pulled out: every connection broken, the port closed (connections refused).
    fn pull(&self) {
        let mut p = self.inner.lock().unwrap();
        if let Some(a) = p.accept.take() {
            a.abort();
        }
        for pipe in p.pipes.drain(..) {
            pipe.abort();
        }
    }

    /// Put back, on the same port.
    async fn plug(&self) {
        let until = Instant::now() + Duration::from_secs(5);
        let listener = loop {
            match TcpListener::bind(self.addr).await {
                Ok(l) => break l,
                Err(e) if Instant::now() < until => {
                    let _ = e;
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                Err(e) => panic!("the cable's port can't be taken again: {e}"),
            }
        };
        self.serve(listener);
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }
}

async fn server(faults: &str) -> LabServer {
    LabServer::start(Config {
        faults: Faults::parse(faults).unwrap(),
        ..Config::default()
    })
    .await
    .unwrap()
}

/// Retries as the app's downloads have them, quicker: 0, 0.3, 0.6, 1.2, 2.4 s.
fn opts() -> Options {
    Options {
        retry_delay: Duration::from_millis(300),
        save_interval: Duration::from_millis(100),
        max_connections: 4,
        ..Options::default()
    }
}

fn want(seed: u64, len: usize) -> Vec<u8> {
    let mut v = vec![0; len];
    content::fill(seed, 0, &mut v);
    v
}

/// Waits until `dl` has `bytes` in, failing after 20 s.
async fn until_in(dl: &Download, bytes: u64) {
    let deadline = Instant::now() + SETTLE;
    while dl.snapshot().downloaded < bytes {
        assert!(Instant::now() < deadline, "the download never got that far");
        assert!(
            !matches!(dl.snapshot().state, State::Failed | State::Done),
            "{:?}",
            dl.snapshot().error
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// The lowest figure the download showed from now until it ends, and how it ended.
async fn lowest_until_end(dl: Download) -> (u64, Result<mdm_engine::Outcome, mdm_engine::Error>) {
    let mut lowest = u64::MAX;
    loop {
        let s = dl.snapshot();
        lowest = lowest.min(s.downloaded);
        if matches!(s.state, State::Done | State::Failed | State::Cancelled) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    (lowest, dl.wait().await)
}

/// The network goes for a moment (a Wi-Fi hiccup, a server restarting): the connections' own
/// retries ride it out, the download goes on where it was (never back at 0) and ends whole.
#[tokio::test]
async fn a_network_gone_for_a_moment_is_ridden_out() {
    let s = server("rate=2M").await;
    let cable = Cable::new(s.addr()).await;
    let dir = tempfile::tempdir().unwrap();
    let dl = Download::start(
        Request {
            url: cable.url("/gen/6M/hiccup.bin?seed=41").parse().unwrap(),
            headers: Default::default(),
            dest: Destination::Dir(dir.path().to_owned()),
        },
        opts(),
    );
    until_in(&dl, 1 << 20).await;
    let had = dl.snapshot().downloaded;
    cable.pull();
    tokio::time::sleep(Duration::from_millis(1200)).await;
    cable.plug().await;
    let (lowest, out) = lowest_until_end(dl).await;
    let out = out.expect("the download ends whole");
    assert!(
        lowest + (1 << 20) >= had,
        "it went back from {had} to {lowest}"
    );
    assert!(std::fs::read(&out.path).unwrap() == want(41, 6 << 20));
}

/// The network goes for longer than the connections' retries: the download fails as one whose
/// network went (what the app starts again by itself, `autoresume.rs`), with its progress saved;
/// once it is back, the resume continues from there, asks nothing again it had (but the few bytes
/// each connection re-reads to check the file), and the file is whole.
#[tokio::test]
async fn a_network_gone_for_long_fails_as_such_and_resumes_where_it_was() {
    let s = server("rate=2M").await;
    let cable = Cable::new(s.addr()).await;
    let dir = tempfile::tempdir().unwrap();
    let dl = Download::start(
        Request {
            url: cable.url("/gen/6M/outage.bin?seed=42").parse().unwrap(),
            headers: Default::default(),
            dest: Destination::Dir(dir.path().to_owned()),
        },
        opts(),
    );
    until_in(&dl, 1 << 20).await;
    let file = dl.snapshot().file.expect("named");
    cable.pull();
    let failed = dl.wait().await;
    let err = failed.expect_err("no network: it fails");
    assert!(err.network_lost(), "{err:?} is a network failure");
    let state = mdm_engine::state::state_path(&file);
    let saved = mdm_engine::state::ResumeState::load(&state).expect("its progress is saved");
    let before = saved.downloaded();
    assert!(before >= 1 << 20, "saved: {before}");
    tokio::time::timeout(SETTLE, s.settled())
        .await
        .expect("the cut requests end");
    s.clear_log();
    cable.plug().await;
    let dl = Download::resume(state.clone(), Default::default(), opts()).unwrap();
    let (lowest, out) = lowest_until_end(dl).await;
    let out = out.expect("the resume ends whole");
    assert!(lowest >= before, "it went back from {before} to {lowest}");
    assert!(std::fs::read(&out.path).unwrap() == want(42, 6 << 20));
    assert!(!state.exists(), "its state is gone once whole");
    tokio::time::timeout(SETTLE, s.settled()).await.unwrap();
    let overlap = opts().verify_overlap;
    for e in s.log() {
        let (from, to) = e
            .range
            .as_deref()
            .and_then(|r| r.strip_prefix("bytes=")?.split_once('-'))
            .map(|(a, b)| (a.parse::<u64>().unwrap(), b.parse::<u64>().unwrap() + 1))
            .expect("every request of a resume asks for a part");
        let to_fetch: u64 = saved
            .segments
            .iter()
            .map(|g| to.min(g.end).saturating_sub(from.max(g.cursor)))
            .sum();
        assert!(
            (to - from) - to_fetch <= overlap,
            "asked again for data it had: {:?}",
            e.range
        );
    }
}

/// The server comes back without range support (restarted on another setup, a CDN that dropped
/// it): no resume can continue the file, and each one failed the same way (#4). The file comes
/// again from the start, on one connection, said in its log, and it is whole; nothing of the old
/// state is left to resume from.
#[tokio::test]
async fn a_server_that_stopped_sending_parts_gets_the_file_again_from_the_start() {
    let s = server("rate=2M").await;
    let dir = tempfile::tempdir().unwrap();
    let dl = Download::start(
        Request {
            url: s.url("/gen/6M/noranges.bin?seed=43").parse().unwrap(),
            headers: Default::default(),
            dest: Destination::Dir(dir.path().to_owned()),
        },
        opts(),
    );
    until_in(&dl, 1 << 20).await;
    let file = dl.snapshot().file.expect("named");
    dl.cancel();
    let _ = dl.wait().await;
    let state = mdm_engine::state::state_path(&file);
    assert!(state.exists(), "stopped part-way");
    tokio::time::timeout(SETTLE, s.settled()).await.unwrap();
    // From now on every answer is the whole file, the same one (its validators unchanged).
    s.set_faults(Faults {
        no_range: Some(true),
        ..s.faults()
    });
    let fast = Options {
        retry_delay: Duration::from_millis(30),
        ..opts()
    };
    let dl = Download::resume(state.clone(), Default::default(), fast).unwrap();
    let mut said = false;
    let out = loop {
        let v = dl.snapshot();
        said |= v
            .events
            .iter()
            .any(|e| e.text.contains("again from the start"));
        if matches!(v.state, State::Done | State::Failed | State::Cancelled) {
            break dl.wait().await;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    let out = out.expect("the file comes whole");
    assert!(said, "its log says it came again from the start");
    assert!(std::fs::read(&out.path).unwrap() == want(43, 6 << 20));
    assert!(!state.exists());
}

/// The same, but the file changed meanwhile (its validators): it is refused as changed, the part
/// on disk untouched, never mixed with another file's bytes.
#[tokio::test]
async fn a_changed_file_without_ranges_is_refused_not_mixed() {
    let s = server("rate=2M").await;
    let dir = tempfile::tempdir().unwrap();
    let dl = Download::start(
        Request {
            url: s.url("/gen/6M/changed.bin?seed=44").parse().unwrap(),
            headers: Default::default(),
            dest: Destination::Dir(dir.path().to_owned()),
        },
        opts(),
    );
    until_in(&dl, 1 << 20).await;
    let file = dl.snapshot().file.expect("named");
    dl.cancel();
    let _ = dl.wait().await;
    let state = mdm_engine::state::state_path(&file);
    tokio::time::timeout(SETTLE, s.settled()).await.unwrap();
    s.set_faults(Faults {
        no_range: Some(true),
        etag_change_every: Some(1),
        ..s.faults()
    });
    let fast = Options {
        retry_delay: Duration::from_millis(30),
        ..opts()
    };
    let err = Download::resume(state, Default::default(), fast)
        .unwrap()
        .wait()
        .await
        .expect_err("another file");
    assert!(
        matches!(err, mdm_engine::Error::FileChanged),
        "{err:?}: refused as changed"
    );
}

/// An unfinished download whose part file was deleted or moved meanwhile (#5): its resume state
/// alone can't be continued (each start failed on it). It starts again from the beginning, says
/// so in its log, and the file is whole.
#[tokio::test]
async fn a_vanished_part_file_starts_again_cleanly() {
    let s = server("rate=2M").await;
    let dir = tempfile::tempdir().unwrap();
    let url: url::Url = s.url("/gen/3M/vanished.bin?seed=45").parse().unwrap();
    let dl = Download::start(
        Request {
            url: url.clone(),
            headers: Default::default(),
            dest: Destination::Dir(dir.path().to_owned()),
        },
        opts(),
    );
    until_in(&dl, 1 << 20).await;
    let file = dl.snapshot().file.expect("named");
    dl.cancel();
    let _ = dl.wait().await;
    let state = mdm_engine::state::state_path(&file);
    assert!(state.exists());
    for part in mdm_engine::naming::part_paths(&file) {
        let _ = std::fs::remove_file(part);
    }
    let look_state = state.clone();
    let dl = Download::begin(url, Default::default(), opts(), None, 1 << 20, move || {
        mdm_engine::Begin::Resume(look_state)
    });
    let mut said = false;
    let out = loop {
        let v = dl.snapshot();
        said |= v.events.iter().any(|e| e.text.contains("starting again"));
        if matches!(v.state, State::Done | State::Failed | State::Cancelled) {
            break dl.wait().await;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    let out = out.expect("it starts again and ends whole");
    assert!(said, "its log says why it started again");
    assert_eq!(out.path, file);
    assert!(std::fs::read(&out.path).unwrap() == want(45, 3 << 20));
    assert!(!state.exists());
}

/// A network gone mid-download refuses every connection: that is no connection limit (#4). The
/// app's downloads share what they learn of each host (`Options::host_limits`): an outage taught
/// "127.0.0.1 allows 1 connection", and the resume, and every download from that host for ten
/// minutes, went on with one connection. Nothing is learned from it now, and the resume uses
/// several connections again.
#[tokio::test]
async fn an_outage_teaches_no_connection_limit() {
    let s = server("rate=1M").await;
    let cable = Cable::new(s.addr()).await;
    let limits = std::sync::Arc::new(mdm_engine::hosts::HostLimits::default());
    let shared = || Options {
        host_limits: Some(limits.clone()),
        ..opts()
    };
    let dir = tempfile::tempdir().unwrap();
    let dl = Download::start(
        Request {
            url: cable.url("/gen/8M/limits.bin?seed=46").parse().unwrap(),
            headers: Default::default(),
            dest: Destination::Dir(dir.path().to_owned()),
        },
        shared(),
    );
    until_in(&dl, 1 << 20).await;
    let file = dl.snapshot().file.expect("named");
    cable.pull();
    let err = dl.wait().await.expect_err("no network: it fails");
    assert!(err.network_lost(), "{err:?}");
    assert_eq!(limits.cap("127.0.0.1"), None, "the outage taught no cap");
    tokio::time::timeout(SETTLE, s.settled()).await.unwrap();
    s.clear_log();
    cable.plug().await;
    let state = mdm_engine::state::state_path(&file);
    let out = Download::resume(state, Default::default(), shared())
        .unwrap()
        .wait()
        .await
        .expect("the resume ends whole");
    assert!(std::fs::read(&out.path).unwrap() == want(46, 8 << 20));
    tokio::time::timeout(SETTLE, s.settled()).await.unwrap();
    // Several of the resume's requests ran at once.
    let log = s.log();
    let mut edges: Vec<(u64, i32)> = log
        .iter()
        .flat_map(|e| [(e.t_start_ms, 1), (e.t_end_ms, -1)])
        .collect();
    edges.sort();
    let most = edges
        .iter()
        .scan(0, |n, (_, d)| {
            *n += d;
            Some(*n)
        })
        .max()
        .unwrap_or(0);
    assert!(most > 1, "the resume went on with {most} connection(s)");
}
