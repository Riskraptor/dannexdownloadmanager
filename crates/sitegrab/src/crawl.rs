//! A grab running: pages fetched breadth-first a few at a time, their links followed or
//! collected, then the files' sizes asked. The window reads its [`Status`] and, once it has
//! ended, its files; a grab that ended early leaves a [`Frontier`] another can go on from.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqwest::header::{CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE, COOKIE, HeaderMap};
use serde::{Deserialize, Serialize};
use tokio::task::JoinSet;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::Spec;
use crate::extract::{self, LinkKind};
use crate::filter::{self, Group, Patterns, Types};
use crate::robots::{self, Robots, Rules};

/// The most of a page read (the rest of a longer one is left: its links are mostly read).
const MAX_PAGE: usize = 8 * 1024 * 1024;
/// The most of a style sheet read.
const MAX_STYLE: usize = 2 * 1024 * 1024;
/// How long one request may take, its answer read too.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
/// How long a size is asked for.
const HEAD_TIMEOUT: Duration = Duration::from_secs(20);
/// The longest pause a robots.txt's `Crawl-delay` makes DDM keep (a site asking for an hour
/// would stall the grab for good).
const MAX_ROBOTS_DELAY: Duration = Duration::from_secs(10);
/// The most cookies kept from one site's answers during a grab.
const MAX_JAR: usize = 50;

/// How the grab connects: the engine's options (proxy, DNS, adapter, User-Agent, timeouts), and
/// the login and cookies for the start address's site, if any (sent there only).
#[derive(Clone, Debug, Default)]
pub struct Net {
    pub opts: mdm_engine::Options,
    pub login: Option<(String, String)>,
    /// As a browser sends them: `name=value; name2=value2`.
    pub cookies: Option<String>,
}

/// What a file found is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FoundKind {
    /// Any file (a picture, a document…); what a project from before the kinds knew.
    #[default]
    File,
    /// A page, read as one.
    Page,
    /// A style sheet.
    Style,
}

/// A file found: its address, the page it was found on (sent as the Referer when it is
/// downloaded), its name and, when known, its size.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Found {
    pub url: String,
    #[serde(default)]
    pub page: String,
    pub name: String,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "is_file")]
    pub kind: FoundKind,
    /// Other addresses that led to it: a page's address before the server sent it on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub also: Vec<String>,
}

fn is_file(k: &FoundKind) -> bool {
    *k == FoundKind::File
}

/// Why the start page gave nothing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Fault {
    /// The server answered with this status.
    Status(u16),
    /// No answer (the error as the network library says it: for the log).
    Network(String),
    /// Its robots.txt asks robots not to visit it.
    Robots,
    /// It is a file, not a page, and not of a type being collected.
    NotPage,
}

/// How a grab ended.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum End {
    /// Every page within reach was explored.
    Finished,
    /// It stopped at the most pages it may explore (there were more).
    MaxPages,
    /// It stopped at the most files it may collect.
    MaxFiles,
    /// Stop was clicked.
    Stopped,
    /// The start page couldn't be read.
    Failed(Fault),
}

impl End {
    /// It ended before every page within reach was explored: another grab can go on from where
    /// it stopped ([`Crawl::frontier`]).
    pub fn early(&self) -> bool {
        matches!(self, End::MaxPages | End::MaxFiles | End::Stopped)
    }
}

/// What it is doing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    Exploring,
    /// Asking the files' sizes: `done` of `total`.
    Sizing {
        done: usize,
        total: usize,
    },
    Ended(End),
}

/// How far it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub phase: Phase,
    /// Pages explored (by the grabs it goes on from too).
    pub pages: u32,
    /// Pages waiting to be.
    pub waiting: u32,
    pub files: usize,
    /// The address asked last.
    pub current: Option<String>,
    /// Addresses not visited because robots.txt asks so.
    pub robots_skipped: u32,
    /// Addresses that failed (an error status, no answer).
    pub failed: u32,
}

impl Default for Status {
    fn default() -> Self {
        Status {
            phase: Phase::Exploring,
            pages: 0,
            waiting: 0,
            files: 0,
            current: None,
            robots_skipped: 0,
            failed: 0,
        }
    }
}

/// Why a grab can't start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Invalid {
    /// The start address isn't a web address.
    Address,
    /// No file type is chosen.
    NoTypes,
}

/// A page or style sheet still to read when a grab ended early.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Waiting {
    pub url: String,
    pub depth: u32,
    /// A style sheet (else a page).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub style: bool,
    /// The page it was found on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
}

/// Where a grab that ended early stood: what it had still to read (pages that got no answer
/// too, to be asked again) and every address it had queued or read. A grab started from it
/// ([`Crawl::resume`]) goes on there, reading nothing twice.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Frontier {
    pub waiting: Vec<Waiting>,
    pub seen: Vec<String>,
    /// Pages explored so far.
    #[serde(default)]
    pub pages: u32,
}

#[derive(Default)]
struct Inner {
    status: Status,
    files: Vec<Found>,
    frontier: Option<Frontier>,
    /// The cookies the start address's site set, once it has ended.
    cookies: Option<String>,
}

/// A grab under way (or ended). Dropping it stops it.
pub struct Crawl {
    shared: Arc<Mutex<Inner>>,
    cancel: CancellationToken,
}

impl Crawl {
    /// Start grabbing with `spec` on `rt`.
    pub fn start(rt: &tokio::runtime::Handle, spec: &Spec, net: Net) -> Result<Crawl, Invalid> {
        Self::begin(rt, spec, net, None)
    }

    /// Go on from where an earlier grab with the same settings stopped: the files it found and
    /// its `frontier`. Most pages and Most files count afresh from there.
    pub fn resume(
        rt: &tokio::runtime::Handle,
        spec: &Spec,
        net: Net,
        found: Vec<Found>,
        frontier: Frontier,
    ) -> Result<Crawl, Invalid> {
        Self::begin(rt, spec, net, Some((found, frontier)))
    }

    fn begin(
        rt: &tokio::runtime::Handle,
        spec: &Spec,
        net: Net,
        from: Option<(Vec<Found>, Frontier)>,
    ) -> Result<Crawl, Invalid> {
        let spec = spec.clamped();
        let start = crate::start_url(&spec.start).ok_or(Invalid::Address)?;
        if spec.types().is_empty() {
            return Err(Invalid::NoTypes);
        }
        let shared = Arc::new(Mutex::new(Inner::default()));
        let cancel = CancellationToken::new();
        let (s, c) = (shared.clone(), cancel.clone());
        rt.spawn(async move {
            let ctx = Arc::new(Ctx::new(&spec, start, net));
            let (end, frontier) = explore(&spec, &ctx, &s, &c, from).await;
            let end = if spec.sizes && !matches!(end, End::Stopped | End::Failed(_)) {
                ask_sizes(&spec, &ctx, &s, &c).await.unwrap_or(end)
            } else {
                end
            };
            let cookies = ctx.site_cookies();
            let mut inner = s.lock().unwrap_or_else(|e| e.into_inner());
            inner.cookies = cookies;
            inner.frontier = end.early().then_some(frontier);
            inner.status.phase = Phase::Ended(end);
            inner.status.current = None;
            inner.status.waiting = 0;
        });
        Ok(Crawl { shared, cancel })
    }

    pub fn status(&self) -> Status {
        self.lock().status.clone()
    }

    /// The files found so far (all of them once it has ended).
    pub fn files(&self) -> Vec<Found> {
        self.lock().files.clone()
    }

    /// Where it stood, once it has ended early (Stop, Most pages, Most files): what
    /// [`Crawl::resume`] goes on from.
    pub fn frontier(&self) -> Option<Frontier> {
        self.lock().frontier.clone()
    }

    /// Once it has ended: the cookies the downloads of the start address's site's files need
    /// (the user's, and those the site set while it was explored: a session, a consent), as a
    /// browser sends them.
    pub fn site_cookies(&self) -> Option<String> {
        self.lock().cookies.clone()
    }

    pub fn ended(&self) -> bool {
        matches!(self.lock().status.phase, Phase::Ended(_))
    }

    /// Stop: what was found stays.
    pub fn stop(&self) {
        self.cancel.cancel();
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.shared.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Drop for Crawl {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

/// What a job fetches a page for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Html,
    Style,
}

#[derive(Clone)]
struct Job {
    url: Url,
    depth: u32,
    kind: Kind,
    page: Option<Url>,
}

impl Job {
    fn waiting(&self) -> Waiting {
        Waiting {
            url: self.url.to_string(),
            depth: self.depth,
            style: self.kind == Kind::Style,
            page: self.page.as_ref().map(Url::to_string),
        }
    }

    fn of(w: &Waiting) -> Option<Job> {
        Some(Job {
            url: Url::parse(&w.url).ok()?,
            depth: w.depth,
            kind: if w.style { Kind::Style } else { Kind::Html },
            page: w.page.as_deref().and_then(|p| Url::parse(p).ok()),
        })
    }

    /// The start page (no page before it).
    fn first(&self) -> bool {
        self.depth == 0 && self.page.is_none()
    }
}

/// What a job brought back.
enum Fetched {
    /// A page or a style sheet: where it ended (redirects followed) and its text.
    Page {
        job: Job,
        at: Url,
        body: String,
    },
    /// Not a page after all: a file, named by the server or its address.
    File {
        job: Job,
        name: String,
        size: Option<u64>,
    },
    Skipped(Job),
    Failed(Job, Fault),
}

/// What the requests of one grab share.
struct Ctx {
    net: Net,
    start: Url,
    obey_robots: bool,
    delay: Duration,
    clients: Mutex<HashMap<String, reqwest::Client>>,
    /// When each site may be asked again.
    gate: Mutex<HashMap<String, Instant>>,
    robots: Mutex<HashMap<String, RobotsCell>>,
    /// The cookies each site set in its answers (its host's), sent back to it for the rest of
    /// the grab, as a browser keeps a session's.
    jar: Mutex<HashMap<String, Vec<(String, String)>>>,
}

/// One site's robots.txt rules, asked once however many requests wait for them.
type RobotsCell = Arc<tokio::sync::OnceCell<Arc<Rules>>>;

impl Ctx {
    fn new(spec: &Spec, start: Url, net: Net) -> Ctx {
        Ctx {
            net,
            start,
            obey_robots: spec.robots,
            delay: Duration::from_millis(u64::from(spec.delay_ms)),
            clients: Mutex::default(),
            gate: Mutex::default(),
            robots: Mutex::default(),
            jar: Mutex::default(),
        }
    }

    /// The engine's client for `url`'s site (one per site: the proxy may differ by site).
    fn client(&self, url: &Url) -> Result<reqwest::Client, Fault> {
        let origin = url.origin().ascii_serialization();
        let mut clients = self.clients.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(c) = clients.get(&origin) {
            return Ok(c.clone());
        }
        let c = mdm_engine::http_client(&self.net.opts, url)
            .map_err(|e| Fault::Network(e.to_string()))?;
        clients.insert(origin, c.clone());
        Ok(c)
    }

    /// A request to `url`: with the login and cookies on the start address's site, and the
    /// cookies `url`'s site set during the grab.
    fn request(
        &self,
        method: reqwest::Method,
        url: &Url,
    ) -> Result<reqwest::RequestBuilder, Fault> {
        let mut req = self.client(url)?.request(method, url.clone());
        if let Some((user, pass)) = &self.net.login
            && crate::Scope::SameHost.allows(&self.start, url)
        {
            req = req.basic_auth(user, Some(pass));
        }
        if let Some(cookies) = self.cookies_for(url) {
            req = req.header(COOKIE, cookies);
        }
        Ok(req)
    }

    /// The start address's site's cookies (`site_cookies`), as a `Cookie` header.
    fn site_cookies(&self) -> Option<String> {
        self.cookies_for(&self.start)
    }

    /// The cookies sent to `url`: the user's on the start address's site, and those `url`'s
    /// site set during the grab.
    fn cookies_for(&self, url: &Url) -> Option<String> {
        let home = crate::Scope::SameHost.allows(&self.start, url);
        let mut cookies: Vec<String> = Vec::new();
        if home && let Some(given) = self.net.cookies.as_deref().map(str::trim) {
            cookies.extend(
                given
                    .split(';')
                    .map(str::trim)
                    .filter(|c| !c.is_empty())
                    .map(str::to_owned),
            );
        }
        if let Some(host) = url.host_str() {
            let jar = self.jar.lock().unwrap_or_else(|e| e.into_inner());
            for (name, value) in jar.get(host).into_iter().flatten() {
                // One the user gave goes first, the site's own of that name is left out.
                if !cookies
                    .iter()
                    .any(|c| c.split('=').next() == Some(name.as_str()))
                {
                    cookies.push(format!("{name}={value}"));
                }
            }
        }
        (!cookies.is_empty()).then(|| cookies.join("; "))
    }

    /// The cookies an answer from `url` sets, kept for the rest of the grab (an expired one,
    /// `Max-Age=0`, taken out).
    fn keep_cookies(&self, url: &Url, headers: &HeaderMap) {
        let Some(host) = url.host_str() else { return };
        let mut jar = self.jar.lock().unwrap_or_else(|e| e.into_inner());
        for v in headers.get_all(reqwest::header::SET_COOKIE) {
            let Ok(v) = v.to_str() else { continue };
            let mut parts = v.split(';');
            let Some((name, value)) = parts.next().and_then(|p| p.split_once('=')) else {
                continue;
            };
            let (name, value) = (name.trim().to_owned(), value.trim().to_owned());
            if name.is_empty() {
                continue;
            }
            let expired = parts.any(|p| {
                let p = p.trim().to_ascii_lowercase();
                p == "max-age=0" || p.starts_with("max-age=-")
            });
            let site = jar.entry(host.to_owned()).or_default();
            site.retain(|(n, _)| *n != name);
            if !expired && site.len() < MAX_JAR {
                site.push((name, value));
            }
        }
    }

    /// The robots.txt rules of `url`'s site (asked once per site; every rule open when not
    /// followed).
    async fn rules(&self, url: &Url, cancel: &CancellationToken) -> Arc<Rules> {
        if !self.obey_robots {
            return Arc::new(Rules::open());
        }
        let origin = url.origin().ascii_serialization();
        let cell = self
            .robots
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(origin)
            .or_default()
            .clone();
        cell.get_or_init(|| async {
            let Ok(robots) = url.join("/robots.txt") else {
                return Arc::new(Rules::open());
            };
            Arc::new(self.fetch_robots(&robots, cancel).await)
        })
        .await
        .clone()
    }

    async fn fetch_robots(&self, robots: &Url, cancel: &CancellationToken) -> Rules {
        self.wait_turn(robots, None, cancel).await;
        let Ok(req) = self.request(reqwest::Method::GET, robots) else {
            return Rules::open();
        };
        let resp = match tokio::time::timeout(HEAD_TIMEOUT, req.send()).await {
            Ok(Ok(r)) => r,
            // No answer at all: as a server error (RFC 9309 2.3.1.4).
            _ => return Rules::closed(),
        };
        let status = resp.status();
        if status.is_server_error() {
            return Rules::closed();
        }
        if !status.is_success() {
            // 4xx: no robots.txt, every path allowed.
            return Rules::open();
        }
        match tokio::time::timeout(HEAD_TIMEOUT, read_limited(resp, robots::MAX_SIZE)).await {
            Ok(Ok(body)) => {
                Robots::parse(&String::from_utf8_lossy(&body)).rules_for(robots::PRODUCT)
            }
            _ => Rules::closed(),
        }
    }

    /// Wait until `url`'s site may be asked again (the pause, or the robots.txt's longer one).
    async fn wait_turn(&self, url: &Url, rules: Option<&Rules>, cancel: &CancellationToken) {
        let delay = rules
            .and_then(|r| r.delay)
            .map_or(self.delay, |d| d.min(MAX_ROBOTS_DELAY).max(self.delay));
        let origin = url.origin().ascii_serialization();
        let at = {
            let mut gate = self.gate.lock().unwrap_or_else(|e| e.into_inner());
            let now = Instant::now();
            let at = gate.get(&origin).copied().unwrap_or(now).max(now);
            gate.insert(origin, at + delay);
            at
        };
        tokio::select! {
            _ = tokio::time::sleep_until(at) => {}
            _ = cancel.cancelled() => {}
        }
    }
}

/// `resp`'s body, at most `max` bytes of it.
async fn read_limited(mut resp: reqwest::Response, max: usize) -> Result<Vec<u8>, reqwest::Error> {
    let mut buf = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        let room = max - buf.len();
        buf.extend_from_slice(&chunk[..chunk.len().min(room)]);
        if buf.len() >= max {
            break;
        }
    }
    Ok(buf)
}

/// The size a response's `Content-Length` says (a HEAD's body is empty: the header is read).
fn length(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(CONTENT_LENGTH)?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// The path and query of `url`, as robots.txt rules are matched against them.
fn robots_path(url: &Url) -> String {
    match url.query() {
        Some(q) => format!("{}?{q}", url.path()),
        None => url.path().to_owned(),
    }
}

/// Fetch one page or style sheet.
async fn fetch(ctx: Arc<Ctx>, job: Job, cancel: CancellationToken) -> Fetched {
    let rules = ctx.rules(&job.url, &cancel).await;
    if !rules.allows(&robots_path(&job.url)) {
        return Fetched::Skipped(job);
    }
    ctx.wait_turn(&job.url, Some(&rules), &cancel).await;
    if cancel.is_cancelled() {
        return Fetched::Failed(job, Fault::Network("stopped".into()));
    }
    let req = match ctx.request(reqwest::Method::GET, &job.url) {
        Ok(r) => r,
        Err(f) => return Fetched::Failed(job, f),
    };
    let resp = match tokio::time::timeout(REQUEST_TIMEOUT, req.send()).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => return Fetched::Failed(job, Fault::Network(e.to_string())),
        Err(_) => return Fetched::Failed(job, Fault::Network("timed out".into())),
    };
    ctx.keep_cookies(resp.url(), resp.headers());
    if !resp.status().is_success() {
        return Fetched::Failed(job, Fault::Status(resp.status().as_u16()));
    }
    let at = resp.url().clone();
    let headers = resp.headers().clone();
    let mime = headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let is_html = mime.starts_with("text/html") || mime.starts_with("application/xhtml");
    let is_css = mime.starts_with("text/css");
    let readable = match job.kind {
        Kind::Html => is_html || (mime.is_empty() && filter::may_be_page(&at)),
        Kind::Style => is_css || (!is_html && at.path().to_ascii_lowercase().ends_with(".css")),
    };
    if !readable {
        // A file: its name from the server, else its address, with the extension its type
        // has when the name lacks one.
        let cd = headers
            .get(CONTENT_DISPOSITION)
            .and_then(|v| v.to_str().ok());
        let mut name = mdm_engine::naming::file_name(cd, &at);
        if filter::extension(&name).is_none()
            && let Some(ext) = filter::extension_for_mime(&mime)
        {
            name = format!("{name}.{ext}");
        }
        return Fetched::File {
            job,
            name,
            size: length(&headers),
        };
    }
    let max = match job.kind {
        Kind::Html => MAX_PAGE,
        Kind::Style => MAX_STYLE,
    };
    match tokio::time::timeout(REQUEST_TIMEOUT, read_limited(resp, max)).await {
        Ok(Ok(body)) => Fetched::Page {
            job,
            at,
            body: String::from_utf8_lossy(&body).into_owned(),
        },
        Ok(Err(e)) => Fetched::Failed(job, Fault::Network(e.to_string())),
        Err(_) => Fetched::Failed(job, Fault::Network("timed out".into())),
    }
}

/// What a grab collects, and the files it has.
struct Collector<'a> {
    spec: &'a Spec,
    start: &'a Url,
    types: Types,
    include: Patterns,
    exclude: Patterns,
    /// Each address collected (and each that led to one), and where its file is in the list.
    index: HashMap<String, usize>,
    /// The most files collected before this grab, and so the most in all.
    limit: usize,
    shared: &'a Mutex<Inner>,
}

impl Collector<'_> {
    /// Collect `url` named `name`, found on `page`, when it is a file to collect: of a type
    /// chosen, matching an include pattern (if any), on a site allowed, not collected yet, and
    /// room left for it.
    fn file(
        &mut self,
        url: &Url,
        name: &str,
        page: Option<&Url>,
        size: Option<u64>,
        kind: FoundKind,
    ) -> bool {
        if self.full()
            || !self.types.wants(name)
            || self.exclude.matches(url.as_str())
            || (!self.include.is_empty() && !self.include.matches(url.as_str()))
            || (!self.spec.files_anywhere && !self.spec.scope.allows(self.start, url))
            || self.index.contains_key(url.as_str())
        {
            return false;
        }
        let mut guard = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        let inner = &mut *guard;
        self.index.insert(url.to_string(), inner.files.len());
        inner.files.push(Found {
            url: url.to_string(),
            page: page.map(Url::to_string).unwrap_or_default(),
            name: mdm_engine::naming::sanitize(name),
            size,
            kind,
            also: Vec::new(),
        });
        inner.status.files = inner.files.len();
        true
    }

    /// Collect the page read at `at`, asked for as `asked` (the same, or the address the server
    /// sent on from): under the address it is at, `asked` noted as another way to it.
    fn page(&mut self, at: &Url, asked: &Url, page: Option<&Url>, size: u64) {
        let name = filter::page_name(at);
        let known = self.index.get(at.as_str()).copied();
        let added = known.is_none() && self.file(at, &name, page, Some(size), FoundKind::Page);
        let Some(i) = known.or_else(|| added.then(|| self.index[at.as_str()])) else {
            return;
        };
        if asked != at && !self.index.contains_key(asked.as_str()) {
            self.index.insert(asked.to_string(), i);
            let mut inner = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(f) = inner.files.get_mut(i) {
                f.also.push(asked.to_string());
            }
        }
    }

    fn full(&self) -> bool {
        self.shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .files
            .len()
            >= self.limit
    }
}

/// Whether a page at `url` may be explored: in the scope chosen, and under the start address's
/// folder when the grab keeps there.
fn explorable(spec: &Spec, start: &Url, url: &Url) -> bool {
    spec.scope.allows(start, url) && (!spec.below || crate::filter::below(start, url))
}

/// Explore from the start page (or from where an earlier grab stopped): every page within
/// reach, breadth-first, `spec.connections` at a time. Returns how it ended, and where it stood.
async fn explore(
    spec: &Spec,
    ctx: &Arc<Ctx>,
    shared: &Mutex<Inner>,
    cancel: &CancellationToken,
    from: Option<(Vec<Found>, Frontier)>,
) -> (End, Frontier) {
    let start = ctx.start.clone();
    let mut col = Collector {
        spec,
        start: &start,
        types: spec.types(),
        include: Patterns::parse(&spec.include),
        exclude: Patterns::parse(&spec.exclude),
        index: HashMap::new(),
        limit: spec.max_files as usize,
        shared,
    };
    let mut queue: VecDeque<Job> = VecDeque::new();
    // Addresses queued or explored already.
    let mut visited: HashSet<String> = HashSet::new();
    let mut pages_before = 0;
    match from {
        None => {
            visited.insert(start.to_string());
            queue.push_back(Job {
                url: start.clone(),
                depth: 0,
                kind: Kind::Html,
                page: None,
            });
        }
        Some((found, frontier)) => {
            for (i, f) in found.iter().enumerate() {
                col.index.insert(f.url.clone(), i);
                for also in &f.also {
                    col.index.insert(also.clone(), i);
                }
            }
            col.limit += found.len();
            pages_before = frontier.pages;
            {
                let mut inner = shared.lock().unwrap_or_else(|e| e.into_inner());
                inner.status.files = found.len();
                inner.status.pages = pages_before;
                inner.files = found;
            }
            visited.extend(frontier.seen);
            for w in &frontier.waiting {
                if let Some(job) = Job::of(w) {
                    visited.insert(job.url.to_string());
                    queue.push_back(job);
                }
            }
        }
    }
    let mut tasks: JoinSet<Fetched> = JoinSet::new();
    // The jobs under way, by address: put back in the frontier when it ends early.
    let mut flying: HashMap<String, Job> = HashMap::new();
    // Pages that got no answer (a timeout, a dropped connection): asked again by a grab that
    // goes on from this one.
    let mut retry: Vec<Waiting> = Vec::new();
    let mut started_pages = 0u32;
    let set =
        |f: &dyn Fn(&mut Status)| f(&mut shared.lock().unwrap_or_else(|e| e.into_inner()).status);
    let frontier = |queue: &VecDeque<Job>,
                    flying: &HashMap<String, Job>,
                    retry: &[Waiting],
                    visited: &HashSet<String>| {
        let mut waiting: Vec<Waiting> = flying.values().map(Job::waiting).collect();
        waiting.extend(queue.iter().map(Job::waiting));
        waiting.extend(retry.iter().cloned());
        let pages = shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .status
            .pages;
        Frontier {
            waiting,
            seen: visited.iter().cloned().collect(),
            pages,
        }
    };
    loop {
        while tasks.len() < spec.connections as usize {
            // Style sheets aren't pages: once Most pages is reached they are still read (the
            // fonts and pictures they name are the pages' own files); only pages wait.
            let next = if started_pages >= spec.max_pages {
                queue
                    .iter()
                    .position(|j| j.kind != Kind::Html)
                    .and_then(|i| queue.remove(i))
            } else {
                queue.pop_front()
            };
            let Some(job) = next else { break };
            if job.kind == Kind::Html {
                started_pages += 1;
            }
            let shown = job.url.to_string();
            set(&|s| s.current = Some(shown.clone()));
            flying.insert(shown, job.clone());
            tasks.spawn(fetch(ctx.clone(), job, cancel.clone()));
        }
        let waiting = queue.iter().filter(|j| j.kind == Kind::Html).count() as u32;
        set(&|s| s.waiting = waiting);
        if tasks.is_empty() {
            let end = if queue.is_empty() {
                End::Finished
            } else {
                End::MaxPages
            };
            return (end, frontier(&queue, &flying, &retry, &visited));
        }
        let next = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                return (End::Stopped, frontier(&queue, &flying, &retry, &visited));
            }
            r = tasks.join_next() => r,
        };
        let Some(Ok(fetched)) = next else { continue };
        let job_url = match &fetched {
            Fetched::Page { job, .. }
            | Fetched::File { job, .. }
            | Fetched::Skipped(job)
            | Fetched::Failed(job, _) => job.url.to_string(),
        };
        flying.remove(&job_url);
        match fetched {
            Fetched::Skipped(job) => {
                if job.first() && pages_before == 0 {
                    return (End::Failed(Fault::Robots), Frontier::default());
                }
                set(&|s| s.robots_skipped += 1);
            }
            Fetched::Failed(job, fault) => {
                if job.first() && pages_before == 0 {
                    return (End::Failed(fault), Frontier::default());
                }
                if matches!(fault, Fault::Network(_)) {
                    retry.push(job.waiting());
                }
                set(&|s| s.failed += 1);
            }
            Fetched::File { job, name, size } => {
                let first = job.first() && pages_before == 0;
                let taken = col.file(&job.url, &name, job.page.as_ref(), size, FoundKind::File);
                if first && !taken {
                    return (End::Failed(Fault::NotPage), Frontier::default());
                }
            }
            Fetched::Page { job, at, body } => match job.kind {
                Kind::Html => {
                    set(&|s| s.pages += 1);
                    visited.insert(at.to_string());
                    // Redirected off the sites explored (a sign-in page elsewhere): not read.
                    if !spec.scope.allows(&start, &at) {
                        continue;
                    }
                    if col.types.has(Group::Pages) {
                        col.page(&at, &job.url, job.page.as_ref(), body.len() as u64);
                    }
                    let links = extract::html_links(&body);
                    let base = links
                        .base
                        .as_deref()
                        .and_then(|b| extract::resolve(&at, b))
                        .unwrap_or_else(|| at.clone());
                    for link in links.links {
                        let Some(u) = extract::resolve(&base, &link.href) else {
                            continue;
                        };
                        if col.exclude.matches(u.as_str()) {
                            continue;
                        }
                        let name = filter::last_segment(&u);
                        match link.kind {
                            LinkKind::Page if filter::may_be_page(&u) => {
                                if job.depth < spec.depth
                                    && explorable(spec, &start, &u)
                                    && visited.insert(u.to_string())
                                {
                                    queue.push_back(Job {
                                        url: u,
                                        depth: job.depth + 1,
                                        kind: Kind::Html,
                                        page: Some(at.clone()),
                                    });
                                }
                            }
                            LinkKind::Page | LinkKind::Resource => {
                                col.file(&u, &name, Some(&at), None, FoundKind::File);
                            }
                            LinkKind::Style => {
                                col.file(&u, &style_name(name), Some(&at), None, FoundKind::Style);
                                if spec.scope.allows(&start, &u) && visited.insert(u.to_string()) {
                                    queue.push_back(Job {
                                        url: u,
                                        depth: job.depth,
                                        kind: Kind::Style,
                                        page: Some(at.clone()),
                                    });
                                }
                            }
                        }
                    }
                }
                Kind::Style => {
                    for (href, kind) in extract::css_links(&body) {
                        let Some(u) = extract::resolve(&at, &href) else {
                            continue;
                        };
                        if col.exclude.matches(u.as_str()) {
                            continue;
                        }
                        let name = filter::last_segment(&u);
                        if kind == LinkKind::Style {
                            col.file(
                                &u,
                                &style_name(name),
                                job.page.as_ref(),
                                None,
                                FoundKind::Style,
                            );
                            if spec.scope.allows(&start, &u) && visited.insert(u.to_string()) {
                                queue.push_back(Job {
                                    url: u,
                                    depth: job.depth,
                                    kind: Kind::Style,
                                    page: job.page.clone(),
                                });
                            }
                        } else {
                            col.file(&u, &name, job.page.as_ref(), None, FoundKind::File);
                        }
                    }
                }
            },
        }
        if col.full() {
            return (End::MaxFiles, frontier(&queue, &flying, &retry, &visited));
        }
    }
}

/// A style sheet's name: with `.css` when its address names none (`/styles?v=3`), so it is
/// collected as one and opens as one.
fn style_name(name: String) -> String {
    if filter::extension(&name).is_some() {
        name
    } else if name.is_empty() {
        "style.css".to_owned()
    } else {
        format!("{name}.css")
    }
}

/// Ask each file without a size for it (HEAD), `spec.connections` at a time. None: it ran to
/// the end; Some(Stopped) when Stop cut it short.
async fn ask_sizes(
    spec: &Spec,
    ctx: &Arc<Ctx>,
    shared: &Mutex<Inner>,
    cancel: &CancellationToken,
) -> Option<End> {
    let todo: Vec<(usize, Url)> = {
        let inner = shared.lock().unwrap_or_else(|e| e.into_inner());
        inner
            .files
            .iter()
            .enumerate()
            .filter(|(_, f)| f.size.is_none())
            .filter_map(|(i, f)| Some((i, Url::parse(&f.url).ok()?)))
            .collect()
    };
    let total = todo.len();
    if total == 0 {
        return None;
    }
    let set_phase = |done| {
        let mut inner = shared.lock().unwrap_or_else(|e| e.into_inner());
        inner.status.phase = Phase::Sizing { done, total };
    };
    set_phase(0);
    let mut todo = todo.into_iter();
    let mut tasks: JoinSet<(usize, Option<u64>)> = JoinSet::new();
    let mut done = 0;
    loop {
        while tasks.len() < spec.connections as usize {
            let Some((i, url)) = todo.next() else { break };
            let (ctx, cancel) = (ctx.clone(), cancel.clone());
            tasks.spawn(async move {
                let rules = ctx.rules(&url, &cancel).await;
                if !rules.allows(&robots_path(&url)) {
                    return (i, None);
                }
                ctx.wait_turn(&url, Some(&rules), &cancel).await;
                let Ok(req) = ctx.request(reqwest::Method::HEAD, &url) else {
                    return (i, None);
                };
                let size = match tokio::time::timeout(HEAD_TIMEOUT, req.send()).await {
                    Ok(Ok(r)) if r.status().is_success() => length(r.headers()),
                    _ => None,
                };
                (i, size)
            });
        }
        if tasks.is_empty() {
            return None;
        }
        let next = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Some(End::Stopped),
            r = tasks.join_next() => r,
        };
        if let Some(Ok((i, size))) = next {
            done += 1;
            let mut guard = shared.lock().unwrap_or_else(|e| e.into_inner());
            let inner = &mut *guard;
            if let Some(f) = inner.files.get_mut(i) {
                f.size = size;
                inner.status.current = Some(f.url.clone());
            }
            inner.status.phase = Phase::Sizing { done, total };
        }
    }
}
