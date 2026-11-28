//! Web seeds (BEP 19, "GetRight style"): a .torrent's `url-list` names HTTP servers that have
//! its files. librqbit 9.1 doesn't download from them (upstream's ikatson/rqbit#739 adds that,
//! not merged). DDM serves each one to librqbit as a peer of its own: a connection from this
//! computer to the session's listening port that speaks the peer wire protocol (BEP 3) as a
//! seed, and answers each block librqbit asks for with bytes fetched from the web seed by HTTP
//! ranges. librqbit picks the pieces, checks each piece's SHA-1, writes it and counts it under
//! its limits, as for any peer: nothing of librqbit is changed or bypassed, and a web seed's
//! damaged copy never reaches a finished file (its piece fails librqbit's check and is asked
//! for again; a web seed that keeps failing is left alone for a while).
//!
//! What it takes: the session listening (Torrent settings → accept connections from peers, on
//! by default; never through a proxy, where librqbit doesn't listen). A web seed on this
//! computer or the local network is never asked (`trackers::allowed`, as for trackers). Not
//! served: BEP 17 (`httpseeds`, Hoffman-style server scripts), and a magnet link's `ws=`.

use std::collections::{HashMap, VecDeque};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use librqbit::Session;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{OnceCell, Semaphore, mpsc};
use tokio_util::sync::CancellationToken;
use url::Url;

/// Web seeds read from one torrent (`url-list`): more is no publisher's list.
const MAX_LISTED: usize = 16;
/// Web seeds served to one torrent at once.
const PER_TORRENT: usize = 2;
/// Pieces one connection fetches at once, and the bytes of the pieces it keeps fetched.
const FETCHES: usize = 4;
const CACHE_BYTES: u64 = 64 << 20;
/// The largest block a peer may ask for (librqbit asks 16 KiB at a time).
const MAX_BLOCK: u32 = 128 << 10;
/// How often the session's torrents are looked at.
const EVERY: Duration = Duration::from_secs(3);
/// librqbit drops a peer silent for 10 s: a keep-alive this often while nothing else is sent.
const KEEP_ALIVE: Duration = Duration::from_secs(4);
/// After a web seed fails: how long it is left alone, doubled each time.
const BACKOFF_MIN: Duration = Duration::from_secs(15);
const BACKOFF_MAX: Duration = Duration::from_secs(600);

/// A .torrent's web seeds as it lists them (`url-list`: one address, or a list), each once.
pub fn listed(torrent: &[u8]) -> Vec<String> {
    let Some(entries) = crate::trackers::top_level(torrent) else {
        return Vec::new();
    };
    let Some((_, value)) = entries.iter().find(|(k, _)| *k == b"url-list") else {
        return Vec::new();
    };
    let mut raw: Vec<&[u8]> = Vec::new();
    if value.first() == Some(&b'l') {
        let mut at = 1;
        while value.get(at).is_some_and(|c| *c != b'e') && raw.len() < MAX_LISTED * 4 {
            let Some((s, next)) = crate::trackers::string(value, at) else {
                break;
            };
            raw.push(s);
            at = next;
        }
    } else if let Some((s, end)) = crate::trackers::string(value, 0)
        && end == value.len()
    {
        raw.push(s);
    }
    let mut out: Vec<String> = Vec::new();
    for u in raw {
        let u = String::from_utf8_lossy(u).trim().to_owned();
        if !u.is_empty() && !out.contains(&u) && out.len() < MAX_LISTED {
            out.push(u);
        }
    }
    out
}

/// A .torrent with `urls` as its web seeds (`url-list`), everything else byte for byte (the
/// info dictionary too: its hash stays). None when it isn't a bencoded dictionary.
pub fn with_web_seeds(torrent: &[u8], urls: &[String]) -> Option<Vec<u8>> {
    let entries = crate::trackers::top_level(torrent)?;
    let mut list = b"l".to_vec();
    for u in urls {
        crate::trackers::push_string(&mut list, u.as_bytes());
    }
    list.push(b'e');
    let mut out: Vec<(&[u8], Vec<u8>)> = entries
        .into_iter()
        .filter(|(k, _)| *k != b"url-list")
        .map(|(k, v)| (k, v.to_vec()))
        .collect();
    out.push((b"url-list", list));
    // A dictionary's keys are sorted (bencode).
    out.sort_by(|a, b| a.0.cmp(b.0));
    let mut done = b"d".to_vec();
    for (k, v) in out {
        crate::trackers::push_string(&mut done, k);
        done.extend_from_slice(&v);
    }
    done.push(b'e');
    Some(done)
}

/// The web seeds DDM may ask: http and https servers on the internet (`local`: on this
/// computer and the local network too, for tests).
fn usable(urls: &[String], local: bool) -> Vec<Url> {
    urls.iter()
        .filter_map(|u| Url::parse(u).ok())
        .filter(|u| matches!(u.scheme(), "http" | "https"))
        .filter(|u| local || crate::trackers::allowed(u.as_str(), true))
        .collect()
}

/// Where a torrent's bytes are: its pieces, its files in order (offset, length, path, BEP 47
/// padding), its name, and whether it is a single file (BEP 19 names it differently).
struct Layout {
    info_hash: [u8; 20],
    name: String,
    piece_length: u64,
    total: u64,
    pieces: u32,
    single: bool,
    files: Vec<Span>,
}

struct Span {
    offset: u64,
    len: u64,
    path: Vec<String>,
    padding: bool,
}

impl Layout {
    fn of(torrent: &[u8]) -> Option<Layout> {
        let meta = librqbit::torrent_from_bytes(torrent).ok()?;
        let info_hash = meta.info_hash.0;
        let info = meta.info.data.validate().ok()?;
        let lengths = info.lengths();
        let files = info
            .iter_file_details_ext()
            .map(|f| Span {
                offset: f.offset,
                len: f.details.len,
                path: f.details.filename.to_vec(),
                padding: f.details.attrs().padding,
            })
            .collect();
        Some(Layout {
            info_hash,
            name: info.name().map(|n| n.into_owned()).unwrap_or_default(),
            piece_length: u64::from(lengths.default_piece_length()),
            total: lengths.total_length(),
            pieces: lengths.total_pieces(),
            single: info.info().files.is_none(),
            files,
        })
    }

    /// Bytes `[from, to)` of piece `i`.
    fn piece(&self, i: u32) -> (u64, u64) {
        let from = u64::from(i) * self.piece_length;
        (from, (from + self.piece_length).min(self.total))
    }

    /// A file's address on web seed `seed` (BEP 19): a single file's is the seed's address, its
    /// name added when that ends in `/`; a multi-file torrent's files are under the seed's
    /// address, in a folder of the torrent's name.
    fn url(&self, seed: &Url, file: &Span) -> Option<Url> {
        let mut url = seed.clone();
        if self.single {
            if seed.path().ends_with('/') {
                url.path_segments_mut()
                    .ok()?
                    .pop_if_empty()
                    .push(&self.name);
            }
            return Some(url);
        }
        {
            let mut segments = url.path_segments_mut().ok()?;
            segments.pop_if_empty().push(&self.name);
            segments.extend(file.path.iter());
        }
        Some(url)
    }
}

/// Serves the session's torrents their web seeds while it runs, when it listens for peers (its
/// stop ends it); `cfg`'s adapter is the web seeds' too, and with `cfg.offline` (tests) web
/// seeds on this computer and the local network are asked as well.
pub(crate) fn start(session: &Arc<Session>, cfg: &crate::Config) {
    let Some(listen) = session.listen_addr() else {
        tracing::info!("torrents: not listening for peers, so web seeds (url-list) are not used");
        return;
    };
    // The session's own port, reached from this computer.
    let ip = match listen.ip() {
        IpAddr::V4(a) if a.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(a) if a.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
        ip => ip,
    };
    let peer = SocketAddr::new(ip, listen.port());
    let Some(client) = client(cfg, cfg.offline) else {
        tracing::warn!("torrents: no HTTP client for web seeds; they are not used");
        return;
    };
    let local = cfg.offline;
    let stop = session.cancellation_token().child_token();
    let session = Arc::downgrade(session);
    tokio::spawn(async move {
        let mut served = Served::default();
        loop {
            tokio::select! {
                () = stop.cancelled() => return,
                () = tokio::time::sleep(EVERY) => {}
            }
            let Some(session) = session.upgrade() else {
                return;
            };
            // A torrent's numbers wait while its state is held (a pause's writes, #693): read
            // on a thread that may wait.
            let (client, stop) = (client.clone(), stop.clone());
            let looked = tokio::task::spawn_blocking(move || {
                served.tick(&session, peer, local, &client, &stop);
                served
            });
            match looked.await {
                Ok(s) => served = s,
                Err(_) => return,
            }
        }
    });
}

/// How web seeds are asked: through the session's adapter (`Config::bind`), with DDM's TLS
/// setup (`mdm_engine::tls`) and the session's client name. A redirect is followed only to an
/// address a web seed could have (`usable`: never into this computer or the local network,
/// unless `local`).
fn client(cfg: &crate::Config, local: bool) -> Option<reqwest::Client> {
    let redirects = reqwest::redirect::Policy::custom(move |attempt| {
        let to = attempt.url().as_str().to_owned();
        if attempt.previous().len() >= 10 || usable(&[to], local).is_empty() {
            attempt.stop()
        } else {
            attempt.follow()
        }
    });
    let mut b = reqwest::Client::builder()
        .user_agent(format!(
            "DannexDownloadManager/{}.{}",
            env!("CARGO_PKG_VERSION_MAJOR"),
            env!("CARGO_PKG_VERSION_MINOR")
        ))
        .redirect(redirects)
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(30));
    match &cfg.bind {
        Some(crate::Bind::Address(ip)) => b = b.local_address(*ip),
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        Some(crate::Bind::Device { name, .. }) => b = b.interface(name),
        _ => {}
    }
    mdm_engine::tls::configure(b).build().ok()
}

/// Each torrent's web seeds as last read (by info hash), and each web seed being served or
/// left alone for a while.
#[derive(Default)]
struct Served {
    layouts: HashMap<[u8; 20], Option<Seeded>>,
    seeds: HashMap<([u8; 20], Url), Seed>,
}

/// A torrent's layout and the web seeds DDM may ask for it.
type Seeded = (Arc<Layout>, Vec<Url>);

#[derive(Default)]
struct Seed {
    /// Its connection, and what ends it.
    running: Option<(tokio::task::JoinHandle<()>, CancellationToken)>,
    /// Shared with the connection: when it may be tried again, and how long it waits next.
    state: Arc<Mutex<(Option<Instant>, Duration)>>,
}

impl Served {
    /// Every downloading torrent with web seeds gets a connection to each of its first ones
    /// that isn't served or left alone; the others (paused, finished, gone) none.
    fn tick(
        &mut self,
        session: &Arc<Session>,
        peer: SocketAddr,
        local: bool,
        client: &reqwest::Client,
        stop: &CancellationToken,
    ) {
        let handles: Vec<crate::Handle> =
            session.with_torrents(|all| all.map(|(_, h)| h.clone()).collect());
        let mut wanted = std::collections::HashSet::new();
        for h in handles {
            let hash = h.info_hash().0;
            let stats = h.stats();
            if stats.finished || !matches!(stats.state, librqbit::TorrentStatsState::Live) {
                continue;
            }
            let entry = self.layouts.entry(hash).or_insert_with(|| {
                let bytes = h.with_metadata(|m| m.torrent_bytes.clone()).ok()?;
                let seeds = usable(&listed(&bytes), local);
                if seeds.is_empty() {
                    return None;
                }
                let layout = Layout::of(&bytes)?;
                tracing::info!("torrents: a torrent has {} web seed(s)", seeds.len());
                Some((Arc::new(layout), seeds))
            });
            let Some((layout, seeds)) = entry.clone() else {
                continue;
            };
            for url in seeds.into_iter().take(PER_TORRENT) {
                wanted.insert((hash, url.clone()));
                let seed = self.seeds.entry((hash, url.clone())).or_default();
                if seed.running.as_ref().is_some_and(|(t, _)| !t.is_finished()) {
                    continue;
                }
                if seed
                    .state
                    .lock()
                    .unwrap()
                    .0
                    .is_some_and(|t| t > Instant::now())
                {
                    continue;
                }
                let (layout, client, state) = (layout.clone(), client.clone(), seed.state.clone());
                let ends = stop.child_token();
                let end = ends.clone();
                let task = tokio::spawn(async move {
                    let res = serve(peer, &layout, &url, &client, &end).await;
                    let mut st = state.lock().unwrap();
                    match res {
                        // librqbit ended it (the torrent finished or paused): tried again at the
                        // next look if it still downloads.
                        Ok(()) => *st = (None, BACKOFF_MIN),
                        // Logged without the web seed's address or the torrent: DDM's log names
                        // no download.
                        Err(e) => {
                            let wait = st.1.max(BACKOFF_MIN);
                            tracing::warn!(
                                "torrents: a web seed failed ({e}); tried again in {} s",
                                wait.as_secs()
                            );
                            *st = (Some(Instant::now() + wait), (wait * 2).min(BACKOFF_MAX));
                        }
                    }
                });
                seed.running = Some((task, ends));
            }
        }
        // A torrent no longer downloading: its connections end.
        self.seeds.retain(|key, seed| {
            let keep = wanted.contains(key);
            if !keep && let Some((_, ends)) = seed.running.take() {
                ends.cancel();
            }
            keep
        });
    }
}

/// The peer protocol's messages DDM reads (BEP 3).
const REQUEST: u8 = 6;
const CANCEL: u8 = 8;
const UNCHOKE: u8 = 1;
const NOT_INTERESTED: u8 = 3;
const BITFIELD: u8 = 5;
const PIECE: u8 = 7;

/// One web seed served to one torrent: a peer connection to the session at `peer` until
/// librqbit ends it (Ok) or the web seed fails (Err, why). Every piece is offered; each block
/// asked for is answered from its piece, fetched whole from the web seed (`Pieces`).
async fn serve(
    peer: SocketAddr,
    layout: &Arc<Layout>,
    seed: &Url,
    client: &reqwest::Client,
    stop: &CancellationToken,
) -> Result<(), String> {
    let stream = tokio::time::timeout(Duration::from_secs(5), tokio::net::TcpStream::connect(peer))
        .await
        .map_err(|_| "the session didn't answer".to_owned())?
        .map_err(|e| format!("the session: {e}"))?;
    stream.set_nodelay(true).ok();
    let (mut rd, mut wr) = stream.into_split();
    // The handshake: no extensions; a peer id of DDM's web seeds.
    let mut hs = Vec::with_capacity(68);
    hs.push(19);
    hs.extend_from_slice(b"BitTorrent protocol");
    hs.extend_from_slice(&[0; 8]);
    hs.extend_from_slice(&layout.info_hash);
    hs.extend_from_slice(&peer_id());
    wr.write_all(&hs).await.map_err(|e| e.to_string())?;
    let mut theirs = [0u8; 68];
    tokio::time::timeout(Duration::from_secs(10), rd.read_exact(&mut theirs))
        .await
        .map_err(|_| "the session sent no handshake".to_owned())?
        .map_err(|e| format!("the session closed: {e}"))?;
    if theirs[0] != 19
        || &theirs[1..20] != b"BitTorrent protocol"
        || theirs[28..48] != layout.info_hash
    {
        return Err("the session answered for another torrent".into());
    }
    // Every piece, and nothing wanted from it.
    let n = usize::try_from(layout.pieces).unwrap_or(0).div_ceil(8);
    let mut bits = vec![0xffu8; n];
    let spare = n * 8 - layout.pieces as usize;
    if let Some(last) = bits.last_mut() {
        *last &= 0xffu8 << spare;
    }
    let (tx, mut out) = mpsc::channel::<Vec<u8>>(256);
    let writer_stop = stop.clone();
    let writer = tokio::spawn(async move {
        let mut first = frame(BITFIELD, &bits);
        first.extend_from_slice(&frame(UNCHOKE, &[]));
        first.extend_from_slice(&frame(NOT_INTERESTED, &[]));
        if wr.write_all(&first).await.is_err() {
            return;
        }
        loop {
            let msg = tokio::select! {
                () = writer_stop.cancelled() => return,
                m = tokio::time::timeout(KEEP_ALIVE, out.recv()) => m,
            };
            let bytes = match msg {
                Ok(Some(m)) => m,
                Ok(None) => return,
                // Nothing to send for a while: a keep-alive.
                Err(_) => vec![0, 0, 0, 0],
            };
            if wr.write_all(&bytes).await.is_err() {
                return;
            }
        }
    });
    let pieces = Arc::new(Pieces::new(layout.clone(), seed.clone(), client.clone()));
    let failed = CancellationToken::new();
    let why: Arc<Mutex<Option<String>>> = Default::default();
    let cancelled: Arc<Mutex<std::collections::HashSet<(u32, u32)>>> = Default::default();
    let result = loop {
        let msg = tokio::select! {
            () = stop.cancelled() => break Ok(()),
            () = failed.cancelled() => {
                break Err(why.lock().unwrap().take().unwrap_or_else(|| "failed".into()));
            }
            m = read_message(&mut rd) => m,
        };
        let Some((id, body)) = (match msg {
            Ok(m) => m,
            // librqbit closed the connection.
            Err(_) => break Ok(()),
        }) else {
            continue; // a keep-alive
        };
        let triple = || -> Option<(u32, u32, u32)> {
            let n = |k: usize| Some(u32::from_be_bytes(body.get(k..k + 4)?.try_into().ok()?));
            Some((n(0)?, n(4)?, n(8)?))
        };
        match id {
            REQUEST => {
                let Some((index, begin, length)) = triple() else {
                    break Err("the session sent a bad request".into());
                };
                let (from, to) = layout.piece(index);
                if index >= layout.pieces
                    || length == 0
                    || length > MAX_BLOCK
                    || u64::from(begin) + u64::from(length) > to - from
                {
                    break Err("the session asked for a block outside the torrent".into());
                }
                cancelled.lock().unwrap().remove(&(index, begin));
                let (pieces, tx, failed, why, cancelled) = (
                    pieces.clone(),
                    tx.clone(),
                    failed.clone(),
                    why.clone(),
                    cancelled.clone(),
                );
                tokio::spawn(async move {
                    match pieces.get(index).await {
                        Ok(data) => {
                            if cancelled.lock().unwrap().remove(&(index, begin)) {
                                return;
                            }
                            let block = &data[begin as usize..(begin + length) as usize];
                            let mut body = Vec::with_capacity(8 + block.len());
                            body.extend_from_slice(&index.to_be_bytes());
                            body.extend_from_slice(&begin.to_be_bytes());
                            body.extend_from_slice(block);
                            let _ = tx.send(frame(PIECE, &body)).await;
                        }
                        Err(e) => {
                            why.lock().unwrap().get_or_insert(e);
                            failed.cancel();
                        }
                    }
                });
            }
            CANCEL => {
                if let Some((index, begin, _)) = triple() {
                    cancelled.lock().unwrap().insert((index, begin));
                }
            }
            // Choke, interest, have, bitfield, port, extensions: nothing to do for a seed.
            _ => {}
        }
    };
    writer.abort();
    result
}

/// A peer id of DDM's web seeds: `-DW0001-` and 12 random digits (never the session's own).
fn peer_id() -> [u8; 20] {
    let mut id = *b"-DW0001-000000000000";
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64)
        ^ (std::process::id() as u64).rotate_left(32);
    let mut x = seed | 1;
    for b in &mut id[8..] {
        // xorshift: distinct enough per connection, nothing secret in it.
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *b = b'0' + (x % 10) as u8;
    }
    id
}

/// A message: its length, its id, its body.
fn frame(id: u8, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(5 + body.len());
    out.extend_from_slice(&(body.len() as u32 + 1).to_be_bytes());
    out.push(id);
    out.extend_from_slice(body);
    out
}

/// The next message from the session: None for a keep-alive. A message longer than any the
/// session sends (a bitfield of a huge torrent fits) is an error.
async fn read_message(
    rd: &mut tokio::net::tcp::OwnedReadHalf,
) -> std::io::Result<Option<(u8, Vec<u8>)>> {
    let mut len = [0u8; 4];
    rd.read_exact(&mut len).await?;
    let len = u32::from_be_bytes(len) as usize;
    if len == 0 {
        return Ok(None);
    }
    if len > (4 << 20) {
        return Err(std::io::Error::other("message too long"));
    }
    let mut buf = vec![0u8; len];
    rd.read_exact(&mut buf).await?;
    let id = buf[0];
    buf.remove(0);
    Ok(Some((id, buf)))
}

/// The pieces one connection fetched from its web seed, each fetched once however many of its
/// blocks are asked for, a few at once, the latest kept (`CACHE_BYTES`).
struct Pieces {
    layout: Arc<Layout>,
    seed: Url,
    client: reqwest::Client,
    fetching: Semaphore,
    /// Each piece asked for (fetched or being fetched), and their order, oldest first.
    kept: Mutex<(HashMap<u32, Fetched>, VecDeque<u32>)>,
}

/// A piece's bytes once fetched (or why not), shared by the blocks asked of it.
type Fetched = Arc<OnceCell<Result<Arc<Vec<u8>>, String>>>;

/// A web seed's failure in words, without its address: DDM's log names no download (a crash
/// report carries the log's end).
fn plain(e: reqwest::Error) -> String {
    e.without_url().to_string()
}

impl Pieces {
    fn new(layout: Arc<Layout>, seed: Url, client: reqwest::Client) -> Pieces {
        Pieces {
            layout,
            seed,
            client,
            fetching: Semaphore::new(FETCHES),
            kept: Default::default(),
        }
    }

    /// Piece `i`'s bytes, fetched once.
    async fn get(&self, i: u32) -> Result<Arc<Vec<u8>>, String> {
        let cell = {
            let mut kept = self.kept.lock().unwrap();
            let (map, order) = &mut *kept;
            let cell = map.entry(i).or_insert_with(|| {
                order.push_back(i);
                Default::default()
            });
            let cell = cell.clone();
            // The oldest go once the kept pieces pass the budget (two at least).
            let keep = (CACHE_BYTES / self.layout.piece_length.max(1)).max(2) as usize;
            while order.len() > keep {
                if let Some(old) = order.pop_front() {
                    map.remove(&old);
                }
            }
            cell
        };
        cell.get_or_init(|| async {
            let _slot = self
                .fetching
                .acquire()
                .await
                .map_err(|_| "stopped".to_owned())?;
            self.fetch(i).await.map(Arc::new)
        })
        .await
        .clone()
    }

    /// Piece `i` from the web seed: one range request for each file it touches; a padding
    /// file's bytes are zeros, never asked for.
    async fn fetch(&self, i: u32) -> Result<Vec<u8>, String> {
        let (from, to) = self.layout.piece(i);
        let mut out = Vec::with_capacity((to - from) as usize);
        for f in &self.layout.files {
            let (start, end) = (f.offset.max(from), (f.offset + f.len).min(to));
            if start >= end {
                continue;
            }
            if f.padding {
                out.resize(out.len() + (end - start) as usize, 0);
                continue;
            }
            let url = self
                .layout
                .url(&self.seed, f)
                .ok_or_else(|| "its address can't name the file".to_owned())?;
            let (a, b) = (start - f.offset, end - f.offset);
            let resp = self
                .client
                .get(url)
                .header(reqwest::header::RANGE, format!("bytes={a}-{}", b - 1))
                .send()
                .await
                .map_err(plain)?;
            let whole_from_start = resp.status() == reqwest::StatusCode::OK && a == 0;
            if resp.status() != reqwest::StatusCode::PARTIAL_CONTENT && !whole_from_start {
                return Err(format!("it answered {}", resp.status()));
            }
            let want = (b - a) as usize;
            let mut got = 0usize;
            let mut resp = resp;
            while got < want {
                let Some(chunk) = resp.chunk().await.map_err(plain)? else {
                    break;
                };
                let take = chunk.len().min(want - got);
                out.extend_from_slice(&chunk[..take]);
                got += take;
            }
            if got < want {
                return Err("it sent less than asked".into());
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_list_is_read_and_written_without_touching_the_info() {
        let torrent = b"d8:announce3:abc4:infod4:name3:isoee";
        assert!(listed(torrent).is_empty());
        let with = with_web_seeds(
            torrent,
            &["https://a.example/iso".into(), "http://b.example/".into()],
        )
        .unwrap();
        assert_eq!(
            listed(&with),
            ["https://a.example/iso", "http://b.example/"]
        );
        // The info dictionary's bytes are the same (so is its hash).
        let info = |b: &[u8]| -> Vec<u8> {
            let e = crate::trackers::top_level(b).unwrap();
            e.iter().find(|(k, _)| *k == b"info").unwrap().1.to_vec()
        };
        assert_eq!(info(&with), info(torrent));
        // One address alone (a string, not a list).
        let one = b"d4:infod4:name3:isoe8:url-list18:https://a.example/e";
        assert_eq!(listed(one), ["https://a.example/"]);
    }

    #[test]
    fn only_web_seeds_on_the_internet_are_asked() {
        let urls: Vec<String> = [
            "https://archive.example/download/item/",
            "http://127.0.0.1:8000/x",
            "http://192.168.1.2/x",
            "ftp://ftp.example/x",
            "not a url",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
        let hosts = |v: Vec<Url>| -> Vec<String> {
            v.iter().map(|u| u.host_str().unwrap().to_owned()).collect()
        };
        assert_eq!(hosts(usable(&urls, false)), ["archive.example"]);
        assert_eq!(
            hosts(usable(&urls, true)),
            ["archive.example", "127.0.0.1", "192.168.1.2"]
        );
    }

    #[test]
    fn bep19_addresses_of_single_and_multi_file_torrents() {
        let layout = |single: bool, name: &str| Layout {
            info_hash: [0; 20],
            name: name.into(),
            piece_length: 16,
            total: 32,
            pieces: 2,
            single,
            files: Vec::new(),
        };
        let span = |path: &[&str]| Span {
            offset: 0,
            len: 1,
            path: path.iter().map(|s| (*s).to_owned()).collect(),
            padding: false,
        };
        let u = |s: &str| Url::parse(s).unwrap();
        let one = layout(true, "disc 1.iso");
        assert_eq!(
            one.url(&u("https://m.example/pub/"), &span(&["disc 1.iso"]))
                .unwrap()
                .as_str(),
            "https://m.example/pub/disc%201.iso"
        );
        assert_eq!(
            one.url(
                &u("https://m.example/pub/x.iso?k=1"),
                &span(&["disc 1.iso"])
            )
            .unwrap()
            .as_str(),
            "https://m.example/pub/x.iso?k=1"
        );
        let many = layout(false, "set");
        assert_eq!(
            many.url(&u("https://m.example/pub"), &span(&["docs", "a#b.txt"]))
                .unwrap()
                .as_str(),
            "https://m.example/pub/set/docs/a%23b.txt"
        );
        assert_eq!(
            many.url(&u("https://m.example/pub/"), &span(&["c.bin"]))
                .unwrap()
                .as_str(),
            "https://m.example/pub/set/c.bin"
        );
    }

    #[test]
    fn messages_are_framed() {
        assert_eq!(frame(UNCHOKE, &[]), [0, 0, 0, 1, 1]);
        assert_eq!(frame(PIECE, &[9, 9]), [0, 0, 0, 3, 7, 9, 9]);
        let id = peer_id();
        assert!(id.starts_with(b"-DW0001-") && id[8..].iter().all(u8::is_ascii_digit));
    }
}
