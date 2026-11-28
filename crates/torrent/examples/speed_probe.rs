//! How fast a real swarm downloads, as the app fetches it: a magnet's file list asked for (or a
//! .torrent read), then the torrent added with the peers met, then each second's peers and speed
//! for a while. The data is deleted at the end (never a whole ISO kept): only the numbers stay.
//! Each run starts a fresh session in a temporary folder, as the app's first torrent does, with
//! everything on (listening on every address: Windows Firewall asks about this program once).
//!
//!   cargo run --release -p mdm-torrent --example speed_probe -- [options] SOURCE
//!
//! SOURCE: a magnet link or a .torrent file. Options: `--secs N` (how long it downloads, 90 by
//! default), `--out DIR` (where its files go meanwhile; a temporary folder by default), `--label
//! TEXT` (the summary row's name), `--without-met-peers` (the download doesn't start from the
//! peers the file list came from, but finds peers again). Prints a line a second (time, peers connected / known, speed,
//! bytes), then a Markdown row: time to the file list, time to the first byte, the peers at the
//! end, and the mean speed over the whole run, its last 30 seconds and its best 10 seconds.
//! RUST_LOG (`librqbit=debug`, ...) adds librqbit's log on stderr.

use std::time::{Duration, Instant};

use mdm_torrent::{Config, Lookup, Phase, PieceOrder, Source, Torrents};

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut value = |name: &str| {
        let i = args.iter().position(|a| a == name)?;
        args.remove(i);
        Some(args.remove(i))
    };
    let secs: u64 = value("--secs").map_or(90, |s| s.parse().expect("a number of seconds"));
    let out = value("--out").map(std::path::PathBuf::from);
    let label = value("--label");
    let unmet = match args.iter().position(|a| a == "--without-met-peers") {
        Some(i) => {
            args.remove(i);
            true
        }
        None => false,
    };
    let Some(source) = args.first().cloned() else {
        eprintln!(
            "usage: speed_probe [--secs N] [--out DIR] [--label TEXT] [--without-met-peers] SOURCE"
        );
        std::process::exit(2);
    };
    let src = match Source::magnet(&source) {
        Some(m) => m,
        None => Source::File(mdm_torrent::read_file(source.as_ref()).expect(".torrent read")),
    };
    let state = tempfile::tempdir().expect("a temporary folder");
    let data = tempfile::tempdir().expect("a temporary folder");
    let folder = out.unwrap_or_else(|| data.path().to_owned());

    let t0 = Instant::now();
    let t = Torrents::start(Config::new(state.path().to_owned()))
        .await
        .expect("session started");
    eprintln!("speed_probe: listening on {:?}", t.listen_addr());
    let lookup = Lookup::default();
    let meta = t
        .metadata(&src, Duration::from_secs(180), Some(&lookup))
        .await
        .expect("the file list");
    let listed = t0.elapsed().as_secs_f64();
    let name = label.unwrap_or_else(|| meta.name.clone());
    eprintln!(
        "speed_probe: {} ({} files, {} MB) listed at {listed:.1} s, {} peers met",
        meta.name,
        meta.files.len(),
        meta.total() >> 20,
        meta.peers.len()
    );
    // The peers the file list came from start the download, as in the app (`--without-met-peers`:
    // it finds them again, from the trackers and the DHT).
    let met = if unmet {
        Vec::new()
    } else {
        meta.peers.clone()
    };
    let hash = t
        .add(&meta.torrent, &folder, None, met, false)
        .await
        .expect("added");
    // As the app's torrents go unless "Download in order" is ticked.
    t.set_order(&hash, PieceOrder::RarestFirst)
        .expect("its order");
    let added = Instant::now();
    let mut samples: Vec<u64> = Vec::new();
    let mut first_byte: Option<f64> = None;
    // Read every 100 ms (the first byte's time), a sample and a line every second.
    let mut tick = 0u64;
    let last = loop {
        tokio::time::sleep(Duration::from_millis(100)).await;
        tick += 1;
        let s = t.stats(&hash).expect("stats");
        if s.fetched > 0 && first_byte.is_none() {
            first_byte = Some(t0.elapsed().as_secs_f64());
        }
        let done = s.phase == Phase::Finished || tick >= secs * 10;
        if !tick.is_multiple_of(10) && !done {
            continue;
        }
        samples.push(s.fetched);
        println!(
            "{:6.1} s  peers {:3} / {:4}  {:8.1} KB/s  {:7.2} MB",
            added.elapsed().as_secs_f64(),
            s.peers_live,
            s.peers_seen,
            s.down_speed / 1024.0,
            s.fetched as f64 / 1048576.0
        );
        if done {
            break s;
        }
    };
    let kinds = t
        .peers(&hash)
        .iter()
        .fold((0, 0), |(tcp, utp), p| match p.kind.as_deref() {
            Some("utp") => (tcp, utp + 1),
            _ => (tcp + 1, utp),
        });
    // KB/s between two samples (each a second apart): over `n` seconds ending at `end`.
    let rate = |end: usize, n: usize| {
        let start = end.saturating_sub(n);
        let from = if start == 0 { 0 } else { samples[start - 1] };
        (samples[end - 1] - from) as f64 / 1024.0 / (end - start) as f64
    };
    let n = samples.len();
    let best = (10.min(n)..=n).map(|e| rate(e, 10)).fold(0.0, f64::max);
    println!(
        "| source | file list | 1st byte | peers at end (tcp / utp) | mean KB/s | last 30 s KB/s | best 10 s KB/s |"
    );
    println!("|---|---|---|---|---|---|---|");
    println!(
        "| {name} | {listed:.1} s | {} | {} / {} known ({} / {}) | {:.0} | {:.0} | {best:.0} |",
        first_byte.map_or("–".to_owned(), |s| format!("{s:.1} s")),
        last.peers_live,
        last.peers_seen,
        kinds.0,
        kinds.1,
        rate(n, n),
        rate(n, 30.min(n)),
    );
    // Its files go with it: the numbers are what this is for.
    let _ = t.remove(&hash, true).await;
    t.stop().await;
}
