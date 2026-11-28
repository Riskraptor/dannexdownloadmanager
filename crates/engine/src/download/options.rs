//! What a download is asked to do (`Options`): its headers, proxy, connections, checksum and pieces,
//! mirrors, the slow disk of tests; where it goes (`Destination`), how it begins (`Begin`), and the
//! connection limits.

use super::*;

/// The most connections a download offers to use (IDM's ceiling too).
pub const MAX_CONNECTIONS: u32 = 32;

/// The most connections once the user allows more than `MAX_CONNECTIONS` (issue #12); the
/// engine never opens more, whatever it is given.
pub const MAX_CONNECTIONS_UNLOCKED: u32 = 128;

#[derive(Clone, Debug)]
pub struct Options {
    /// Max simultaneous connections. IDM's default is 8 (notes/ENGINE.md §4). Read through
    /// [`Options::connections`], which keeps it in `1..=MAX_CONNECTIONS_UNLOCKED`.
    pub max_connections: u32,
    /// Smallest half a split may leave. IDM split pieces down to ≤ 12.6 KB (lab S4, D-007).
    pub min_split: u64,
    pub connect_timeout: Duration,
    /// Max silence on a connection before it is considered dead.
    pub read_timeout: Duration,
    /// Consecutive failures after which a connection gives up (it retires if others are
    /// still working, otherwise the download fails).
    pub max_retries: u32,
    /// The first retry is immediate (as IDM, lab S5); then this delay, doubling, capped at 30 s.
    pub retry_delay: Duration,
    /// The download's User-Agent: by default the automatic one (`agent::automatic`, the default
    /// browser's), else the user's or the one a browser handed over.
    pub user_agent: String,
    /// `user_agent` is DDM's automatic one, not one the user set or a browser handed over: a
    /// site that wants an agent of its own (Google Drive's hosts, `sites::user_agent`) gets it.
    /// [`Options::speak_as`] sets another agent and clears this.
    pub automatic_agent: bool,
    /// Asked with instead of `user_agent` when the server refuses the first request with 403
    /// (#372): a browser's User-Agent on a connection that isn't a browser's trips some sites'
    /// bot checks (Cloudflare, seen on a public PDF), while DDM's own passes. Set when
    /// `user_agent` is the browser's. A later 403 is left to the download.
    pub agent_fallback: Option<String>,
    /// From the browser: the address its own download of this file ended at, its redirects
    /// followed. When the address asked for is refused before any byte (`link_refused`: a
    /// one-time link the browser already spent, a token bound to its session, a site that
    /// turns away whatever isn't that browser), the download asks there once instead (#630),
    /// the address's cookies and logins going only if it is the same site (`scoped_headers`).
    /// Never for a form's POST (its body goes where it was given) or through a service account.
    pub browser_final: Option<Url>,
    /// How often resume state is saved (IDM: "several times per minute").
    pub save_interval: Duration,
    /// Speed limit for this download, bytes/s (IDM's limiter is per file). None = unlimited.
    pub speed_limit: Option<u64>,
    /// Before continuing a segment that already has data on disk, re-fetch up to this many of
    /// its last written bytes and compare them (IDM's resume overlap check; catches changed
    /// files on servers without validators). 0 turns it off.
    pub verify_overlap: u64,
    /// Fixed addresses for host names, like hosts-file entries (tests).
    pub hosts: Vec<(String, Vec<std::net::IpAddr>)>,
    /// Options → Connection → DNS: who answers the names (#36).
    pub dns: crate::dns::DnsChoice,
    /// Options → Proxy / Socks → Network adapter (#35): the adapter every connection goes
    /// through, or none at all.
    pub route: crate::net::Route,
    /// A service account that unlocks this download's link first (#44, `crate::unlock`): asked
    /// before the first request and when the direct link dies. Not in the resume state (like
    /// the headers): whoever resumes gives it again.
    pub unlocker: Option<Arc<dyn crate::unlock::Unlocker>>,
    /// Options → Proxy / Socks.
    pub proxy: Proxy,
    /// A limit shared by every download using the same limiter (a global speed cap), on top
    /// of the download's own.
    pub global: Option<Arc<Limiter>>,
    /// A limit shared by a group of downloads that the list shows as one (a gallery's files,
    /// #769: its speed limit is the gallery's, however many of its files come at once), on top
    /// of the download's own and `global`.
    pub group: Option<Arc<Limiter>>,
    /// The finished file must have this hash (else the server's announced one, if any).
    pub checksum: Option<crate::verify::Hash>,
    /// Other copies of this exact file, normally from a Metalink document.  They are used for
    /// parallel ranged requests only after a whole-file checksum or piece hashes are known, so
    /// an untrusted or stale mirror can never silently corrupt the finished file.  Request
    /// credentials are scoped to each mirror's origin before they are sent.
    pub mirrors: Vec<Url>,
    /// The hashes of the file's pieces (a Metalink's `<pieces>`): the finished file's pieces
    /// are checked against them, and a bad one is downloaded again, from another source than
    /// the one that sent it when there is one (`repair_pieces`).
    pub pieces: Option<Arc<crate::verify::Pieces>>,
    /// Reconnect a connection far slower than the others (see `SHED_*`). Beyond IDM's rule.
    pub shed_slow: bool,
    /// Connection caps learned per host, shared by the downloads that use the same table.
    pub host_limits: Option<Arc<crate::hosts::HostLimits>>,
    /// Resume across a changed modification time (IDM: "Ignore file modification time changes
    /// when resuming a download"): servers that stamp every answer with a new date would
    /// otherwise make each resume start over. Only an ETag then tells a changed file.
    pub ignore_mtime: bool,
    /// Give the finished file the server's date (IDM: "Set file creation date as provided by
    /// the server"): created and modified times from `Last-Modified`.
    pub server_time: bool,
    /// FTP data connections: passive (the client connects; IDM's "Use passive mode for FTP",
    /// on by default), else active (the server connects back).
    pub ftp_passive: bool,
    /// Certificates (DER) trusted besides the system's for FTPS and HTTPS (tests' own CA).
    pub extra_roots: Vec<Vec<u8>>,
    /// Send every request as POST with this body, the Content-Type coming from the headers (a
    /// browser form's download). Redirects go as in browsers: 301, 302 and 303 turn it into a
    /// GET without the body (and without its Content-Type), 307 and 308 keep both. A POST may
    /// have side effects, so further connections repeat it only once an answer has shown that
    /// the server takes ranges (206 or `Accept-Ranges: bytes`); until then the first
    /// connection takes the whole file alone. Not stored in the resume state (like the
    /// headers): whoever resumes gives it again. HTTP only.
    pub post: Option<Bytes>,
    /// Called with the part file's path right after the engine creates it, before any data
    /// is written: once per new download (`Download::start`, `fetch`), never by a resume, which
    /// writes into the part file it finds. The part file is renamed (never copied) to the final
    /// name, so whatever the hook attaches to it — Windows' Mark of the Web stream, macOS'
    /// quarantine attribute — stays with the finished file.
    pub on_file_created: Option<FileHook>,
    /// From the browser: the MIME type its own download had. A web page where it had another
    /// type is the server refusing DDM (a firewall's "The requested URL was rejected"), an error
    /// and never saved as the file (#533, `expects_other_file`).
    pub expected_type: Option<String>,
    /// The most one request asks for (bytes): every request, the first one too, is a range of
    /// at most this many, the next one following on the same connection. For servers that slow
    /// a long answer down (YouTube's media servers serve 10 MB ranges at full speed and a whole
    /// file at a fraction of it: yt-dlp's `http_chunk_size`, #769). None: each connection asks
    /// for its whole part at once. HTTP only, never with a POST.
    pub request_cap: Option<u64>,
    /// Tests: a disk whose flush takes this long and holds every write to the file meanwhile
    /// (as NTFS does, #361). None: the real disk.
    #[doc(hidden)]
    pub slow_flush: Option<SlowFlush>,
}

/// A disk whose flush takes `takes` and, as NTFS's does, makes every write to the file wait for
/// it: the gate is held exclusively for the flush and shared for each write (`Options::slow_flush`).
#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct SlowFlush {
    pub takes: Duration,
    pub gate: Arc<std::sync::RwLock<()>>,
}

/// A callback given the path of a file the engine created (`Options::on_file_created`).
#[derive(Clone)]
pub struct FileHook(pub Arc<dyn Fn(&Path) + Send + Sync>);

impl FileHook {
    pub fn new(f: impl Fn(&Path) + Send + Sync + 'static) -> Self {
        FileHook(Arc::new(f))
    }
}

impl std::fmt::Debug for FileHook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FileHook")
    }
}

/// Which proxy downloads go through (Options → Proxy / Socks).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Proxy {
    /// "No proxy/socks" (IDM's default): direct connections.
    #[default]
    None,
    /// "Use system settings": Windows' proxy (and the environment's).
    System,
    /// "Manual proxy/socks configuration": `http://host:port` (or `socks5://…`), with an
    /// optional login, for the listed schemes.
    Manual {
        url: String,
        login: Option<(String, String)>,
        http: bool,
        https: bool,
    },
    /// "Use automatic configuration script": the script decides per address (`pac`); one
    /// that doesn't run leaves it to the system's settings.
    Pac { script: Arc<str> },
}

impl Default for Options {
    fn default() -> Self {
        Options {
            max_connections: 8,
            min_split: 12 * 1024,
            connect_timeout: Duration::from_secs(30),
            read_timeout: Duration::from_secs(12),
            max_retries: 5,
            retry_delay: Duration::from_secs(1),
            user_agent: crate::agent::automatic(),
            automatic_agent: true,
            agent_fallback: None,
            browser_final: None,
            save_interval: Duration::from_secs(2),
            speed_limit: None,
            verify_overlap: 64,
            hosts: Vec::new(),
            dns: Default::default(),
            route: Default::default(),
            unlocker: None,
            proxy: Proxy::None,
            global: None,
            group: None,
            checksum: None,
            mirrors: Vec::new(),
            pieces: None,
            shed_slow: true,
            host_limits: None,
            ignore_mtime: false,
            server_time: false,
            ftp_passive: true,
            extra_roots: Vec::new(),
            post: None,
            on_file_created: None,
            expected_type: None,
            request_cap: None,
            slow_flush: None,
        }
    }
}

impl Options {
    /// Speak as `agent` (the user's, a browser's) instead of the automatic User-Agent.
    pub fn speak_as(&mut self, agent: String) {
        self.user_agent = agent;
        self.automatic_agent = false;
    }

    /// A site's own User-Agent may replace this download's (`request_agent`): the agent is the
    /// automatic one, or DDM's former one (#372's fallback, D-007), as before #668.
    pub(super) fn site_agent(&self) -> bool {
        self.automatic_agent || self.user_agent == crate::agent::IE11_USER_AGENT
    }

    /// The connections this download may open: `max_connections` in
    /// `1..=MAX_CONNECTIONS_UNLOCKED` (a hand-edited setting can hold anything).
    pub fn connections(&self) -> u32 {
        self.max_connections.clamp(1, MAX_CONNECTIONS_UNLOCKED)
    }

    /// `request_cap`, when it applies: some bytes, without a form's POST.
    pub(super) fn cap(&self) -> Option<u64> {
        self.request_cap.filter(|c| *c > 0 && self.post.is_none())
    }

    /// The limits this download shares with others, beside its own: its group's (`group`),
    /// then the one for all downloads (`global`).
    pub(super) fn shared_limits(&self) -> impl Iterator<Item = &Limiter> {
        self.group
            .as_deref()
            .into_iter()
            .chain(self.global.as_deref())
    }
}

#[derive(Clone, Debug)]
pub enum Destination {
    /// Save into this folder under the name the server/URL suggests (numbered if taken).
    Dir(PathBuf),
    /// Save exactly here.
    File(PathBuf),
    /// Save here, or under the next free numbered name in its folder (`naming::unique_path`)
    /// when this one is taken as the download starts: a name given before its folder was looked
    /// at (#743: a window that waits for a sleeping drive freezes), decided here, beside it.
    Free(PathBuf),
}

impl Destination {
    /// The folder the file goes into.
    pub(super) fn folder(&self) -> &Path {
        match self {
            Destination::File(p) | Destination::Free(p) => p.parent().unwrap_or(Path::new("")),
            Destination::Dir(dir) => dir,
        }
    }
}

/// How a download goes on, decided beside it as it starts (`Download::begin`, #743): what a
/// look at the disk finds (a resume state there, a place of its own) that a window must not
/// wait for, a sleeping hard disk, a USB drive or a network folder taking seconds to answer.
#[derive(Clone, Debug)]
pub enum Begin {
    /// Continue from this state file (`state::state_path` of the file it names).
    Resume(PathBuf),
    /// Nothing to continue: start from scratch.
    Fresh(Destination),
}

#[derive(Clone, Debug)]
pub struct Request {
    pub url: Url,
    /// Extra headers sent on every request (Referer, Cookie, Authorization …).
    pub headers: HeaderMap,
    pub dest: Destination,
}

/// Whether the engine can download `url`: http, https, ftp, ftps (implicit TLS) and ftpes
/// (explicit TLS). Magnet links are the torrent engine's.
pub fn supported(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https") || crate::ftp::is_ftp(url)
}
