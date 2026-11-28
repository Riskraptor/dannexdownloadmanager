//! Link grabber (#37): the links of a page (the browser's "Download all links"), of a list file
//! (Tasks → Import), dropped or pasted, gathered into packages to tick before they are added.
//! It grew out of the Import window and keeps its ways (IDM's "Download all links": every link
//! looked up for its name and size, ticking all or none, "Hide web pages", the ticked ones added
//! to the queue); the packages come from JDownloader's link grabber: a split archive's volumes
//! are one package, the other links of one page another, the rest "Various files". A package has
//! its folder, its category, an archive password and "Extract when done".
//!
//! Beyond both: each link's text on the page, its site, size and type in columns sorted by any
//! of them; the links filtered by kind of file (with counts, and a tick for each kind) and by a
//! search; the same file linked twice grouped under its first link; links already in the
//! download list or already on the disk marked and left unticked. Built for thousands of links:
//! the rows are made again only when something changed, looked up a few at a time in parallel
//! (answers come back on a channel), and the disk is asked beside the window (#743).
//!
//! This file is the model (what the window shows and does, tested without a window);
//! `grabview.rs` draws it.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};

use url::Url;

use crate::dialogs::Outcome;
use crate::model::{Category, category_folder};
use crate::transfer::Imported;

/// Links looked up at once, and at once from one host.
const CHECKS: usize = 16;
const CHECKS_PER_HOST: usize = 6;
/// How far one frame looks for the next links to look up, from the first one waiting.
const SCAN: usize = 4096;

/// What looking a link up found.
#[derive(Clone, Debug, PartialEq)]
pub enum LinkState {
    Unchecked,
    Checking,
    Online,
    /// The server said it isn't there (404, 410).
    Offline(u16),
    /// 401 or 403: a login or the page's cookies are needed.
    NeedsLogin,
    /// The server asked to wait (429, 503).
    Busy,
    /// Anything else, in the interface language.
    Failed(String),
}

impl LinkState {
    /// Its place when the list is sorted by status: the good ones first.
    fn rank(&self) -> u8 {
        match self {
            LinkState::Online => 0,
            LinkState::Checking => 1,
            LinkState::Unchecked => 2,
            LinkState::Busy => 3,
            LinkState::NeedsLogin => 4,
            LinkState::Offline(_) => 5,
            LinkState::Failed(_) => 6,
        }
    }
}

/// A look-up's answer: the server's name and size, or why the link is no good.
pub type Answer = Result<mdm_engine::ProbeInfo, LinkState>;

/// The look-ups' answers' channel: each link's index and its answer.
type Answers = (Sender<(usize, Answer)>, Receiver<(usize, Answer)>);

/// Which package a link goes in: a split archive's volumes together, else the links of one page,
/// else "Various files".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// Volumes of one archive on one host (`movie.part1.rar`, `movie.part2.rar`).
    Volumes {
        host: String,
        base: String,
    },
    /// The page the links were on (its host and path).
    Page(String),
    Various,
    /// In the download list already.
    Known,
}

impl Origin {
    /// Where the link `url` named `name` (found on page `referer`) belongs.
    pub fn of(url: &Url, name: &str, referer: Option<&str>) -> Origin {
        let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
        if let Some(v) = mdm_extract::volume_key(name) {
            return Origin::Volumes { host, base: v.base };
        }
        if let Some(page) = referer.and_then(|r| Url::parse(r).ok())
            && let Some(h) = page.host_str()
        {
            let path = page.path().trim_end_matches('/');
            return Origin::Page(format!("{}{path}", h.to_ascii_lowercase()));
        }
        Origin::Various
    }

    /// Volumes of one archive are one set whatever case their names are in.
    fn same(&self, other: &Origin) -> bool {
        match (self, other) {
            (Origin::Volumes { host: a, base: x }, Origin::Volumes { host: b, base: y }) => {
                a == b && x.to_lowercase() == y.to_lowercase()
            }
            _ => self == other,
        }
    }

    fn name(&self) -> String {
        match self {
            Origin::Volumes { base, .. } => base.clone(),
            Origin::Page(p) => p.clone(),
            // i18n: Link grabber: the package of links that belong to no other
            Origin::Various => crate::i18n::t("Various files").to_owned(),
            // i18n: Link grabber: the package of links the download list has already
            Origin::Known => crate::i18n::t("Already in the list").to_owned(),
        }
    }
}

/// A web page's name rather than a file's (IDM's "Hide HTML files").
pub fn web_page(name: &str) -> bool {
    matches!(
        extension(name).as_str(),
        "htm" | "html" | "shtml" | "xhtml" | "php" | "asp" | "aspx" | "jsp" | "cgi"
    )
}

/// A name's extension, lowercase ("" without one).
fn extension(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, e)| e)
        .filter(|e| !e.is_empty() && e.len() <= 12 && !e.contains(['/', '\\', ' ']))
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// The kinds of file the window filters and ticks by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    Video,
    Audio,
    Archive,
    Document,
    Program,
    Image,
    Other,
}

impl Kind {
    pub const ALL: [Kind; 7] = [
        Kind::Video,
        Kind::Audio,
        Kind::Archive,
        Kind::Document,
        Kind::Program,
        Kind::Image,
        Kind::Other,
    ];

    /// A file's kind, by its extension (a split archive's volumes are archives).
    pub fn of(name: &str) -> Kind {
        if mdm_extract::volume_key(name).is_some() {
            return Kind::Archive;
        }
        let ext = extension(name);
        let is = |list: &[&str]| list.contains(&ext.as_str());
        if is(&[
            "mp4", "mkv", "avi", "webm", "mov", "m4v", "flv", "wmv", "mpg", "mpeg", "mpe", "ts",
            "m2ts", "mts", "3gp", "ogv", "asf", "rm", "rmvb", "vob", "divx",
        ]) {
            Kind::Video
        } else if is(&[
            "mp3", "m4a", "aac", "flac", "wav", "ogg", "oga", "opus", "wma", "aif", "aiff", "alac",
            "ape", "mka", "mid", "midi", "ra", "mpa",
        ]) {
            Kind::Audio
        } else if is(&[
            "zip", "rar", "7z", "tar", "gz", "tgz", "bz2", "tbz", "xz", "txz", "zst", "lz", "lzma",
            "cab", "arj", "ace", "iso", "img", "001", "sit", "sitx",
        ]) || ext.starts_with('r')
            && ext.len() == 3
            && ext[1..].bytes().all(|b| b.is_ascii_digit())
        {
            Kind::Archive
        } else if is(&[
            "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "pps", "ppsx", "odt", "ods", "odp",
            "rtf", "txt", "csv", "epub", "mobi", "azw3", "djvu", "md", "xps", "chm",
        ]) {
            Kind::Document
        } else if is(&[
            "exe",
            "msi",
            "msix",
            "msixbundle",
            "appx",
            "apk",
            "dmg",
            "pkg",
            "deb",
            "rpm",
            "appimage",
            "flatpak",
            "snap",
            "jar",
            "bat",
            "cmd",
            "sh",
            "run",
            "bin",
        ]) {
            Kind::Program
        } else if is(&[
            "jpg", "jpeg", "png", "gif", "webp", "bmp", "tif", "tiff", "svg", "heic", "heif",
            "avif", "ico", "psd", "raw", "cr2", "nef", "jxl",
        ]) {
            Kind::Image
        } else {
            Kind::Other
        }
    }

    /// Its name on the filter's buttons (English, translated where drawn).
    pub fn label(self) -> &'static str {
        match self {
            // i18n: Link grabber: a filter button: links to video files
            Kind::Video => crate::i18n::mark("Video"),
            // i18n: Link grabber: a filter button: links to sound files (music)
            Kind::Audio => crate::i18n::mark("Audio"),
            // i18n: Link grabber: a filter button: links to archives (zip, rar, 7z) and disc images
            Kind::Archive => crate::i18n::mark("Archives"),
            // i18n: Link grabber: a filter button: links to documents (pdf, doc, ebooks)
            Kind::Document => crate::i18n::mark("Documents"),
            // i18n: Link grabber: a filter button: links to programs and installers (exe, msi, apk)
            Kind::Program => crate::i18n::mark("Programs"),
            // i18n: Link grabber: a filter button: links to pictures (jpg, png, gif)
            Kind::Image => crate::i18n::mark("Images"),
            // i18n: Link grabber: a filter button: links to any other kind of file
            Kind::Other => crate::i18n::mark("Other"),
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// The list's columns, as it can be sorted by them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Col {
    Name,
    Text,
    Site,
    Size,
    Type,
    Status,
}

pub struct Package {
    origin: Origin,
    pub name: String,
    /// Where its files go (as typed).
    pub folder: String,
    /// Typed by the user: no longer follows the files' category.
    folder_set: bool,
    /// The category its downloads go in: None, each file's own (by its type).
    pub category: Option<Category>,
    pub password: String,
    pub extract: bool,
    extract_set: bool,
    pub delete: bool,
    open: bool,
}

pub struct Link {
    pub found: Imported,
    url: Url,
    pkg: usize,
    pub name: String,
    pub size: Option<u64>,
    pub state: LinkState,
    pub on: bool,
    /// Ticked or unticked by hand: what the window would tick by itself no longer applies.
    touched: bool,
    /// In the download list already.
    known: bool,
    /// Taken out of the window (Delete): neither shown nor added.
    removed: bool,
    /// The kind of file its name says, and where the search looks (name, text, address),
    /// lowercase: both made again when the name changes.
    kind: Kind,
    hay: String,
    /// The site, as shown: the host without "www.".
    site: String,
    /// The first link to the same file, when this one is a second link to it.
    dup_of: Option<usize>,
    /// Whether its file is on the disk already, at the path last asked about (None: not known
    /// yet).
    on_disk: Option<bool>,
    disk_path: Option<PathBuf>,
}

impl Link {
    /// The text the page shows for it (the link's words, a picture's description).
    pub fn text(&self) -> &str {
        &self.found.text
    }

    pub fn site(&self) -> &str {
        &self.site
    }

    pub fn known(&self) -> bool {
        self.known
    }

    pub fn on_disk(&self) -> bool {
        self.on_disk == Some(true)
    }

    pub fn dup_of(&self) -> Option<usize> {
        self.dup_of
    }

    /// The extension, as the Type column shows it (`PDF`).
    pub fn type_text(&self) -> String {
        extension(&self.name).to_ascii_uppercase()
    }

    pub fn address(&self) -> &Url {
        &self.url
    }

    /// The package it is in.
    pub fn pkg_index(&self) -> usize {
        self.pkg
    }

    /// Ticked unless it is in the list or on the disk already, offline, or a second link to
    /// the same file.
    fn wanted(&self) -> bool {
        !self.known
            && self.dup_of.is_none()
            && self.on_disk != Some(true)
            && !matches!(self.state, LinkState::Offline(_))
    }

    fn renamed(&mut self) {
        self.kind = Kind::of(&self.name);
        self.hay = format!(
            "{}\n{}\n{}",
            self.name.to_lowercase(),
            self.found.text.to_lowercase(),
            self.url.as_str().to_lowercase()
        );
    }
}

/// A package's ticked links, as the window hands them over.
pub struct Picked {
    pub name: String,
    pub folder: PathBuf,
    /// The category its downloads go in (None: each file's own).
    pub category: Option<Category>,
    pub password: Option<String>,
    pub extract: bool,
    pub delete: bool,
    /// Each link, the name to save it under (looked up, else the address's), and its size.
    pub links: Vec<(Imported, String, Option<u64>)>,
}

/// What a row of the list is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    Pkg(usize),
    Link(usize),
}

/// What the window shows, made again only when something changed (`Grabber::generation`).
#[derive(Default)]
struct Shown {
    /// The generation it was made for; None: never made.
    made: Option<u64>,
    rows: Vec<Row>,
    /// Per kind ([`Kind::index`]): the links the search finds (second links to a file left out),
    /// and how many of them are ticked.
    kinds: [(usize, usize); 7],
    /// Per package: its links shown (second links left out), how many are ticked, their size
    /// together (None while one is unknown).
    packages: Vec<(usize, usize, Option<u64>)>,
    /// Per first link: how many second links to its file there are.
    copies: HashMap<usize, usize>,
    /// What would be added: how many, of how many links, their size together, and how many of
    /// them have no size known.
    ticked: usize,
    total: usize,
    bytes: u64,
    unknown: usize,
}

/// The disk, asked beside the window (#743) whether files are there already.
struct Disk {
    ask: Sender<(usize, PathBuf)>,
    answers: Receiver<(usize, PathBuf, bool)>,
}

impl Disk {
    fn new(wake: Option<egui::Context>) -> Disk {
        let (ask, questions) = channel::<(usize, PathBuf)>();
        let (tell, answers) = channel();
        let started = std::thread::Builder::new()
            .name("ddm-grabber-disk".into())
            .spawn(move || {
                // Ends when the window is gone (its sender with it).
                while let Ok((i, path)) = questions.recv() {
                    let there = path.is_file();
                    if tell.send((i, path, there)).is_err() {
                        break;
                    }
                    if let Some(ctx) = &wake {
                        ctx.request_repaint();
                    }
                }
            });
        if let Err(e) = started {
            tracing::warn!("link grabber: the disk can't be asked: {e}");
        }
        Disk { ask, answers }
    }
}

pub struct Grabber {
    pub packages: Vec<Package>,
    pub links: Vec<Link>,
    source: String,
    pub hide_pages: bool,
    /// The default folders (Options → Save to), as category_folder reads them.
    downloads: PathBuf,
    dirs: Vec<(String, String)>,
    /// Delete archives after extracting, for a new package (Options).
    delete_default: bool,
    /// The addresses here (without a #fragment): a link is added once.
    addresses: HashSet<String>,
    /// The search box's text, the kind of file shown (None: all), the column sorted by and
    /// whether downward, and whether links are shown in their packages.
    pub search: String,
    pub filter: Option<Kind>,
    pub sort: Option<(Col, bool)>,
    pub grouped: bool,
    /// The first links whose second links to the same file are shown under them.
    copies_open: HashSet<usize>,
    /// Moves on with every change to what the list shows.
    generation: u64,
    shown: Shown,
    /// Look-ups: the answers' channel, how many are under way (and from each host), the first
    /// link that may still wait for one, and how many have answered.
    answers: Answers,
    in_flight: usize,
    per_host: HashMap<String, usize>,
    cursor: usize,
    pub looked: usize,
    /// Whether links are looked up at all (Options → Archives).
    pub look_up: bool,
    disk: Option<Disk>,
    /// Some link's file may be elsewhere than the disk was last asked about.
    disk_due: bool,
    /// The window's own state: the list's scroll and selection (the row selected, followed
    /// through changes), the package the box under it edits, a box with the keys, the search box
    /// to take the keys next frame.
    pub(crate) rows: crate::winui::RowList,
    pub(crate) selected_row: Option<Row>,
    pub(crate) current: usize,
    pub(crate) editing: bool,
    pub(crate) focus_search: bool,
    /// The list's row height (View → Density).
    pub row_h: f32,
}

impl Grabber {
    pub fn new(
        downloads: PathBuf,
        dirs: Vec<(String, String)>,
        hide_pages: bool,
        delete_default: bool,
    ) -> Self {
        Grabber {
            packages: Vec::new(),
            links: Vec::new(),
            source: String::new(),
            hide_pages,
            downloads,
            dirs,
            delete_default,
            addresses: HashSet::new(),
            search: String::new(),
            filter: None,
            sort: None,
            grouped: true,
            copies_open: HashSet::new(),
            generation: 0,
            shown: Shown::default(),
            answers: channel(),
            in_flight: 0,
            per_host: HashMap::new(),
            cursor: 0,
            looked: 0,
            look_up: true,
            disk: None,
            disk_due: false,
            rows: Default::default(),
            selected_row: None,
            current: 0,
            editing: false,
            focus_search: false,
            row_h: 22.0,
        }
    }

    /// Something the list shows changed: made again before it is next drawn.
    fn changed(&mut self) {
        self.generation += 1;
    }

    /// More links, from `source` ("Links from the browser", a file's name): each in its
    /// package; one the download list has (`known`) comes unticked under "Already in the list";
    /// a link here already is not added again.
    pub fn add(&mut self, found: Vec<Imported>, source: String, known: impl Fn(&Url) -> bool) {
        let mut settle = HashSet::new();
        for d in found {
            let Ok(url) = Url::parse(&d.url) else {
                continue;
            };
            if !self.addresses.insert(address_key(&url)) {
                continue;
            }
            let name = mdm_engine::naming::sanitize(&mdm_engine::naming::file_name(None, &url));
            let known = known(&url);
            let origin = if known {
                Origin::Known
            } else {
                Origin::of(&url, &name, d.referer.as_deref())
            };
            let pkg = self.package_for(origin);
            let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
            let site = host.strip_prefix("www.").unwrap_or(&host).to_owned();
            let mut link = Link {
                found: d,
                url,
                pkg,
                name,
                size: None,
                state: LinkState::Unchecked,
                on: !known,
                touched: false,
                known,
                removed: false,
                kind: Kind::Other,
                hay: String::new(),
                site,
                dup_of: None,
                on_disk: None,
                disk_path: None,
            };
            link.renamed();
            self.links.push(link);
            settle.insert(pkg);
        }
        for k in settle {
            self.settle(k);
        }
        if !source.is_empty() && !self.source.split(", ").any(|s| s == source) {
            self.source = if self.source.is_empty() {
                source
            } else {
                format!("{}, {source}", self.source)
            };
        }
        self.disk_due = true;
        self.changed();
    }

    /// Where the links came from ("Links from the browser", a file's name, "Pasted").
    pub fn source(&self) -> &str {
        &self.source
    }

    /// `/show grabber`: a page's links as looked up (the docs' screenshots), nothing asked of
    /// the network.
    pub fn sample(&mut self) {
        let page = Some("https://example.com/files/".to_owned());
        let links: [(&str, &str, Option<u64>); 12] = [
            (
                "https://dl.example.com/holiday.part1.rar",
                "Holiday photos, part 1",
                Some(1_610_612_736),
            ),
            (
                "https://dl.example.com/holiday.part2.rar",
                "Part 2",
                Some(1_610_612_736),
            ),
            ("https://dl.example.com/holiday.part3.rar", "Part 3", None),
            (
                "https://example.com/files/manual.pdf",
                "User manual (PDF)",
                Some(2_516_582),
            ),
            (
                "https://mirror.example.net/files/manual.pdf",
                "Manual, mirror",
                Some(2_516_582),
            ),
            (
                "https://example.com/files/setup.exe",
                "Download the installer",
                Some(48_234_496),
            ),
            (
                "https://media.example.com/trailer-1080p.mp4",
                "Watch the trailer (1080p)",
                Some(187_695_104),
            ),
            (
                "https://media.example.com/theme-song.mp3",
                "Theme song",
                Some(6_291_456),
            ),
            (
                "https://media.example.com/poster.jpg",
                "Poster",
                Some(1_153_434),
            ),
            (
                "https://media.example.com/wallpaper-4k.png",
                "Wallpaper, 4K",
                Some(9_961_472),
            ),
            (
                "https://example.com/files/release-notes.txt",
                "What's new",
                Some(12_288),
            ),
            ("https://example.com/files/index.html", "Home", Some(18_432)),
        ];
        let found = links
            .iter()
            .map(|(u, text, _)| Imported {
                url: (*u).to_owned(),
                referer: page.clone(),
                text: (*text).to_owned(),
                ..Default::default()
            })
            .collect();
        // i18n: in Link grabber, where the addresses came from: a page in the browser
        let from = crate::i18n::t("Links from the browser").to_owned();
        let known = "https://example.com/files/release-notes.txt";
        self.add(found, from, |u| u.as_str() == known);
        for (l, (_, _, size)) in self.links.iter_mut().zip(links) {
            if l.known {
                continue;
            }
            l.size = size;
            l.state = if size.is_some() {
                LinkState::Online
            } else {
                LinkState::Offline(404)
            };
        }
        self.looked = self.links.iter().filter(|l| !l.known).count();
        self.changed();
    }

    /// `/show grabber-many:<n>`: a page's `n` links of every kind, as looked up (Link grabber's
    /// speed with thousands of links, and its screenshots), nothing asked of the network.
    pub fn many(&mut self, n: usize) {
        const KINDS: [(&str, &str, u64); 8] = [
            ("mp4", "Episode", 734_003_200),
            ("mp3", "Track", 7_340_032),
            ("jpg", "Photo", 2_202_009),
            ("pdf", "Chapter", 1_572_864),
            ("zip", "Bundle", 52_428_800),
            ("exe", "Setup", 98_566_144),
            ("png", "Scan", 4_194_304),
            ("epub", "Book", 3_145_728),
        ];
        let page = Some("https://archive.example.org/collection/".to_owned());
        let found = (0..n)
            .map(|k| {
                let (ext, word, _) = KINDS[k % KINDS.len()];
                let host = [
                    "archive.example.org",
                    "cdn.example.net",
                    "files.example.com",
                ][k % 3];
                Imported {
                    url: format!("https://{host}/items/{word}-{:04}.{ext}", k + 1).to_lowercase(),
                    referer: page.clone(),
                    text: format!("{word} {}", k + 1),
                    ..Default::default()
                }
            })
            .collect();
        // i18n: in Link grabber, where the addresses came from: a page in the browser
        let from = crate::i18n::t("Links from the browser").to_owned();
        self.add(found, from, |_| false);
        for (k, l) in self.links.iter_mut().enumerate() {
            let (_, _, size) = KINDS[k % KINDS.len()];
            l.size = Some(size + (k as u64 * 7919) % 100_000);
            l.state = if k % 97 == 13 {
                LinkState::Offline(404)
            } else {
                LinkState::Online
            };
        }
        self.looked = self.links.len();
        self.changed();
    }

    fn package_for(&mut self, origin: Origin) -> usize {
        if let Some(k) = self.packages.iter().position(|p| p.origin.same(&origin)) {
            return k;
        }
        self.packages.push(Package {
            name: origin.name(),
            origin,
            folder: String::new(),
            folder_set: false,
            category: None,
            password: String::new(),
            extract: false,
            extract_set: false,
            delete: self.delete_default,
            open: true,
        });
        self.packages.len() - 1
    }

    /// A package's defaults after its links (or its category) changed: its category's folder
    /// when one is chosen, else the folder of its files' category when they have one; else
    /// none, each file going to its own category's folder (as Import always did); "Extract
    /// when done" when it holds an archive.
    fn settle(&mut self, k: usize) {
        let links = || self.links.iter().filter(move |l| l.pkg == k && !l.removed);
        let folder = match self.packages[k].category {
            Some(cat) => Some(cat),
            None => {
                let mut cats = links().map(|l| Category::of(&l.name));
                let first = cats.next().unwrap_or(Category::General);
                cats.all(|c| c == first).then_some(first)
            }
        }
        .map(|cat| {
            category_folder(cat, &self.downloads, &self.dirs)
                .display()
                .to_string()
        })
        .unwrap_or_default();
        let archives = links().any(|l| crate::extractq::archive_name(&l.name));
        let p = &mut self.packages[k];
        if !p.folder_set {
            p.folder = folder;
        }
        if !p.extract_set {
            p.extract = archives;
        }
        self.disk_due = true;
    }

    /// Shown (not a hidden web page, not taken out)?
    fn shows(&self, l: &Link) -> bool {
        !l.removed && !(self.hide_pages && web_page(&l.name))
    }

    /// Found by the search box (every link while it is empty).
    fn found_by(&self, l: &Link, needle: &str) -> bool {
        needle.is_empty() || l.hay.contains(needle)
    }

    /// Second links to the same file: a name seen before (the same, whatever its case) of a
    /// file whose size, when both are known, is the same. A name without an extension (a page's
    /// address, `download`) is no file's to compare by.
    fn find_copies(&mut self) {
        let mut first: HashMap<String, usize> = HashMap::new();
        for i in 0..self.links.len() {
            let l = &self.links[i];
            let was = l.dup_of;
            let mut of = None;
            if !l.removed && !l.known && !extension(&l.name).is_empty() {
                let key = l.name.to_lowercase();
                match first.get(&key) {
                    Some(&j) => {
                        let (a, b) = (self.links[j].size, l.size);
                        if a.is_none() || b.is_none() || a == b {
                            of = Some(j);
                        }
                    }
                    None => {
                        first.insert(key, i);
                    }
                }
            }
            if of != was {
                self.links[i].dup_of = of;
            }
        }
    }

    /// The window's own ticks, on the links not ticked or unticked by hand: what it wants
    /// ([`Link::wanted`]).
    fn auto_ticks(&mut self) {
        for l in self.links.iter_mut().filter(|l| !l.touched) {
            l.on = l.wanted();
        }
    }

    /// What the list shows now: made again when something changed since.
    fn shown(&mut self) -> &Shown {
        if self.shown.made != Some(self.generation) {
            self.find_copies();
            self.auto_ticks();
            self.shown = self.make_shown();
        }
        &self.shown
    }

    fn make_shown(&self) -> Shown {
        let needle = self.search.trim().to_lowercase();
        let mut s = Shown {
            made: Some(self.generation),
            packages: vec![(0, 0, Some(0)); self.packages.len()],
            ..Default::default()
        };
        // What would be added, whatever is shown.
        for l in self.links.iter().filter(|l| self.shows(l)) {
            if l.dup_of.is_none() {
                s.total += 1;
            }
            if l.on {
                s.ticked += 1;
                match l.size {
                    Some(b) => s.bytes += b,
                    None => s.unknown += 1,
                }
            }
        }
        // The links the search finds, by kind; those the filter shows too.
        let mut mains = Vec::new();
        for (i, l) in self.links.iter().enumerate() {
            if !self.shows(l) || !self.found_by(l, &needle) {
                continue;
            }
            if let Some(j) = l.dup_of {
                *s.copies.entry(j).or_default() += 1;
                continue;
            }
            let k = &mut s.kinds[l.kind.index()];
            k.0 += 1;
            k.1 += usize::from(l.on);
            if self.filter.is_some_and(|f| f != l.kind) {
                continue;
            }
            let p = &mut s.packages[l.pkg];
            p.0 += 1;
            p.1 += usize::from(l.on);
            p.2 = match (p.2, l.size) {
                (Some(a), Some(b)) => Some(a + b),
                _ => None,
            };
            mains.push(i);
        }
        self.sort_links(&mut mains);
        // Each first link followed by its second links when they are open.
        let copies_of = |i: usize, rows: &mut Vec<Row>| {
            if self.copies_open.contains(&i) && s.copies.contains_key(&i) {
                rows.extend(
                    self.links
                        .iter()
                        .enumerate()
                        .filter(|(_, c)| {
                            c.dup_of == Some(i) && self.shows(c) && self.found_by(c, &needle)
                        })
                        .map(|(j, _)| Row::Link(j)),
                );
            }
        };
        let mut rows = Vec::with_capacity(mains.len() + self.packages.len());
        if self.grouped {
            let mut by_pkg: Vec<Vec<usize>> = vec![Vec::new(); self.packages.len()];
            for i in mains {
                by_pkg[self.links[i].pkg].push(i);
            }
            for (k, links) in by_pkg.into_iter().enumerate() {
                if links.is_empty() {
                    continue;
                }
                rows.push(Row::Pkg(k));
                if self.packages[k].open {
                    for i in links {
                        rows.push(Row::Link(i));
                        copies_of(i, &mut rows);
                    }
                }
            }
        } else {
            for i in mains {
                rows.push(Row::Link(i));
                copies_of(i, &mut rows);
            }
        }
        s.rows = rows;
        s
    }

    /// Links in the order of the column sorted by (their order on the page while none is).
    fn sort_links(&self, links: &mut [usize]) {
        let Some((col, down)) = self.sort else {
            return;
        };
        let l = &self.links;
        let cmp = |a: &usize, b: &usize| -> std::cmp::Ordering {
            let (x, y) = (&l[*a], &l[*b]);
            let by = match col {
                Col::Name => natural(&x.name, &y.name),
                Col::Text => natural(x.text(), y.text()),
                Col::Site => x.site.cmp(&y.site),
                // Unknown sizes last whichever way.
                Col::Size => match (x.size, y.size) {
                    (Some(a), Some(b)) => a.cmp(&b),
                    (Some(_), None) => return std::cmp::Ordering::Less,
                    (None, Some(_)) => return std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                },
                Col::Type => x
                    .kind
                    .cmp(&y.kind)
                    .then_with(|| extension(&x.name).cmp(&extension(&y.name))),
                Col::Status => x.state.rank().cmp(&y.state.rank()),
            };
            let by = if down { by.reverse() } else { by };
            by.then(a.cmp(b))
        };
        links.sort_by(cmp);
    }

    /// The rows shown now.
    pub fn rows(&mut self) -> &[Row] {
        &self.shown().rows
    }

    /// Package `k`'s links shown (how many, how many ticked, their size together), and how many
    /// second links to link `i`'s file there are, as last made ([`Grabber::rows`] makes them
    /// again first): for the rows' painter, which can't change the model.
    pub(crate) fn package_counts_now(&self, k: usize) -> (usize, usize, Option<u64>) {
        self.shown.packages.get(k).copied().unwrap_or((0, 0, None))
    }

    pub(crate) fn copies_now(&self, i: usize) -> usize {
        self.shown.copies.get(&i).copied().unwrap_or(0)
    }

    /// Per kind: the links the search finds and how many are ticked.
    pub fn kind_counts(&mut self) -> [(usize, usize); 7] {
        self.shown().kinds
    }

    /// Package `k`'s links shown: how many, how many ticked, and their size together.
    pub fn package_counts(&mut self, k: usize) -> (usize, usize, Option<u64>) {
        self.shown()
            .packages
            .get(k)
            .copied()
            .unwrap_or((0, 0, None))
    }

    /// How many second links to the file of link `i` there are.
    pub fn copies(&mut self, i: usize) -> usize {
        self.shown().copies.get(&i).copied().unwrap_or(0)
    }

    pub fn copies_open(&self, i: usize) -> bool {
        self.copies_open.contains(&i)
    }

    /// What would be added: how many links, of how many, their size together, and how many of
    /// them have no size known.
    pub fn summary(&mut self) -> (usize, usize, u64, usize) {
        let s = self.shown();
        (s.ticked, s.total, s.bytes, s.unknown)
    }

    /// Links to look up now (at most `CHECKS` at once, `CHECKS_PER_HOST` from one host): the
    /// caller starts each ([`Grabber::checking`]) with the link's request and sends its answer
    /// on [`Grabber::answers`].
    pub fn next_checks(&mut self) -> Vec<(usize, Url, Imported)> {
        let mut room = CHECKS.saturating_sub(self.in_flight);
        let mut out = Vec::new();
        if room == 0 || !self.look_up {
            return out;
        }
        let waits = |l: &Link| l.state == LinkState::Unchecked && !l.known && !l.removed;
        while self.cursor < self.links.len() && !waits(&self.links[self.cursor]) {
            self.cursor += 1;
        }
        let mut hosts: HashMap<&str, usize> = HashMap::new();
        for (i, l) in self.links.iter().enumerate().skip(self.cursor).take(SCAN) {
            if room == 0 {
                break;
            }
            if !waits(l) {
                continue;
            }
            let host = l.url.host_str().unwrap_or_default();
            let busy = self.per_host.get(host).copied().unwrap_or(0);
            let now = hosts.entry(host).or_default();
            if busy + *now >= CHECKS_PER_HOST {
                continue;
            }
            *now += 1;
            room -= 1;
            out.push((i, l.url.clone(), l.found.clone()));
        }
        out
    }

    /// Link `i` is being looked up; its answer comes on [`Grabber::answers`].
    pub fn checking(&mut self, i: usize) {
        if let Some(l) = self.links.get_mut(i) {
            l.state = LinkState::Checking;
            let host = l.url.host_str().unwrap_or_default().to_owned();
            *self.per_host.entry(host).or_default() += 1;
            self.in_flight += 1;
            self.changed();
        }
    }

    /// Where a look-up sends its answer (the link's index, and what the server said).
    pub fn answers(&self) -> Sender<(usize, Answer)> {
        self.answers.0.clone()
    }

    /// How many links are looked up in all (none in the list already, none taken out).
    pub fn to_look(&self) -> usize {
        self.links.iter().filter(|l| !l.known && !l.removed).count()
    }

    /// Answers that came: the server's name and size, or why the link is no good.
    fn take_answers(&mut self) {
        let mut moved = HashSet::new();
        let mut any = false;
        while let Ok((i, answer)) = self.answers.1.try_recv() {
            let Some(l) = self.links.get_mut(i) else {
                continue;
            };
            any = true;
            if l.state == LinkState::Checking {
                let host = l.url.host_str().unwrap_or_default();
                if let Some(n) = self.per_host.get_mut(host) {
                    *n = n.saturating_sub(1);
                }
                self.in_flight = self.in_flight.saturating_sub(1);
            }
            self.looked += 1;
            match answer {
                Ok(info) => {
                    l.state = LinkState::Online;
                    l.size = info.size;
                    let named = mdm_engine::naming::sanitize(&info.file_name);
                    if !named.is_empty() && named != l.name {
                        l.name = named;
                        l.renamed();
                        moved.insert(l.pkg);
                    }
                }
                Err(state) => l.state = state,
            }
        }
        for k in moved {
            self.settle(k);
        }
        if any {
            self.changed();
        }
    }

    /// Asks the disk about each link whose file may now be elsewhere (its name or its folder
    /// changed), and takes what it answered: a file there already is marked, and unticked unless
    /// it was ticked by hand.
    fn ask_disk(&mut self, wake: Option<&egui::Context>) {
        if self.disk_due {
            self.disk_due = false;
            let disk = self.disk.get_or_insert_with(|| Disk::new(wake.cloned()));
            let mut folders: HashMap<Category, PathBuf> = HashMap::new();
            for (i, l) in self.links.iter_mut().enumerate() {
                if l.removed || l.known || l.name.is_empty() {
                    continue;
                }
                let pk = &self.packages[l.pkg];
                let dir = if pk.folder.trim().is_empty() {
                    let cat = pk.category.unwrap_or_else(|| Category::of(&l.name));
                    folders
                        .entry(cat)
                        .or_insert_with(|| category_folder(cat, &self.downloads, &self.dirs))
                        .clone()
                } else {
                    crate::dialogs::resolve_path(pk.folder.trim())
                };
                let path = dir.join(&l.name);
                if l.disk_path.as_ref() != Some(&path) {
                    l.on_disk = None;
                    l.disk_path = Some(path.clone());
                    let _ = disk.ask.send((i, path));
                }
            }
        }
        let Some(disk) = &self.disk else { return };
        let mut any = false;
        while let Ok((i, path, there)) = disk.answers.try_recv() {
            if let Some(l) = self.links.get_mut(i)
                && l.disk_path.as_ref() == Some(&path)
            {
                l.on_disk = Some(there);
                any = true;
            }
        }
        if any {
            self.changed();
        }
    }

    /// The window's work between frames: answers taken, the disk asked.
    pub fn tick(&mut self, wake: Option<&egui::Context>) {
        self.take_answers();
        self.ask_disk(wake);
    }

    /// The links the list shows now (second links under a closed first one left out).
    fn shown_links(&mut self) -> Vec<usize> {
        let needle = self.search.trim().to_lowercase();
        self.shown();
        self.links
            .iter()
            .enumerate()
            .filter(|(_, l)| {
                self.shows(l)
                    && l.dup_of.is_none()
                    && self.found_by(l, &needle)
                    && self.filter.is_none_or(|f| f == l.kind)
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Tick (or untick) every link shown: those of the kind shown, the search finds; second
    /// links to a file stay as they are.
    pub fn tick_shown(&mut self, on: bool) {
        for i in self.shown_links() {
            let l = &mut self.links[i];
            l.on = on;
            l.touched = true;
        }
        self.changed();
    }

    /// Every link shown the other way round.
    pub fn invert_shown(&mut self) {
        for i in self.shown_links() {
            let l = &mut self.links[i];
            l.on = !l.on;
            l.touched = true;
        }
        self.changed();
    }

    /// Tick every link of `kind` the search finds, or untick them when all are ticked.
    pub fn tick_kind(&mut self, kind: Kind) {
        let (n, on) = self.kind_counts()[kind.index()];
        let to = on < n;
        let needle = self.search.trim().to_lowercase();
        let picks: Vec<usize> = self
            .links
            .iter()
            .enumerate()
            .filter(|(_, l)| {
                self.shows(l) && l.dup_of.is_none() && l.kind == kind && self.found_by(l, &needle)
            })
            .map(|(i, _)| i)
            .collect();
        for i in picks {
            let l = &mut self.links[i];
            l.on = to;
            l.touched = true;
        }
        self.changed();
    }

    pub fn toggle_link(&mut self, i: usize) {
        if let Some(l) = self.links.get_mut(i) {
            l.on = !l.on;
            l.touched = true;
            self.changed();
        }
    }

    /// Package `k`'s links shown, all ticked, or all unticked when they all were.
    pub fn tick_package(&mut self, k: usize) {
        let (n, on, _) = self.package_counts(k);
        let to = on < n;
        let shown = self.shown_links();
        for i in shown {
            let l = &mut self.links[i];
            if l.pkg == k {
                l.on = to;
                l.touched = true;
            }
        }
        self.changed();
    }

    /// Open or close package `k`.
    pub fn flip_package(&mut self, k: usize) {
        if let Some(p) = self.packages.get_mut(k) {
            p.open = !p.open;
            self.changed();
        }
    }

    pub fn package_open(&self, k: usize) -> bool {
        self.packages.get(k).is_some_and(|p| p.open)
    }

    /// Show or hide the second links to link `i`'s file.
    pub fn flip_copies(&mut self, i: usize) {
        if !self.copies_open.remove(&i) {
            self.copies_open.insert(i);
        }
        self.changed();
    }

    /// Take a link (or a package's links) out of the window.
    pub fn remove(&mut self, row: Row) {
        match row {
            Row::Link(i) => {
                if let Some(l) = self.links.get_mut(i) {
                    l.removed = true;
                    l.on = false;
                }
            }
            Row::Pkg(k) => {
                for l in self.links.iter_mut().filter(|l| l.pkg == k) {
                    l.removed = true;
                    l.on = false;
                }
            }
        }
        self.changed();
    }

    /// Sort by `col`: upward, then downward, then back in the page's order.
    pub fn sort_by(&mut self, col: Col) {
        self.sort = match self.sort {
            Some((c, false)) if c == col => Some((col, true)),
            Some((c, true)) if c == col => None,
            _ => Some((col, false)),
        };
        self.changed();
    }

    pub fn set_filter(&mut self, kind: Option<Kind>) {
        if self.filter != kind {
            self.filter = kind;
            self.changed();
        }
    }

    pub fn set_search(&mut self, text: String) {
        if self.search != text {
            self.search = text;
            self.changed();
        }
    }

    pub fn set_grouped(&mut self, on: bool) {
        if self.grouped != on {
            self.grouped = on;
            self.changed();
        }
    }

    pub fn set_hide_pages(&mut self, on: bool) {
        if self.hide_pages != on {
            self.hide_pages = on;
            self.changed();
        }
    }

    /// Package `k`'s box edited: a folder typed or chosen (it no longer follows the files'
    /// category, and the disk is asked again), "Extract when done" changed by hand.
    pub(crate) fn package_edited(&mut self, k: usize, folder: bool, extract: bool) {
        let Some(p) = self.packages.get_mut(k) else {
            return;
        };
        p.folder_set |= folder;
        p.extract_set |= extract;
        if folder {
            self.disk_due = true;
            self.changed();
        }
    }

    /// Package `k`'s category chosen (None: each file's own): its folder follows unless one
    /// was typed.
    pub fn set_category(&mut self, k: usize, category: Option<Category>) {
        if let Some(p) = self.packages.get_mut(k)
            && p.category != category
        {
            p.category = category;
            self.settle(k);
            self.changed();
        }
    }

    /// The ticked links of each package, as Download now and Add to queue take them.
    pub fn picked(&self) -> Vec<Picked> {
        self.packages
            .iter()
            .enumerate()
            .filter_map(|(k, p)| {
                let links: Vec<(Imported, String, Option<u64>)> = self
                    .links
                    .iter()
                    .filter(|l| l.pkg == k && l.on && self.shows(l))
                    .map(|l| (l.found.clone(), l.name.clone(), l.size))
                    .collect();
                (!links.is_empty()).then(|| Picked {
                    name: p.name.clone(),
                    folder: PathBuf::from(p.folder.trim()),
                    category: p.category,
                    password: Some(p.password.clone()).filter(|s| !s.is_empty()),
                    extract: p.extract,
                    delete: p.extract && p.delete,
                    links,
                })
            })
            .collect()
    }

    /// `Done((start, packages))`: Download now (`true`) or Add to queue (`false`) with the
    /// ticked links of each package.
    pub fn show(&mut self, ctx: &egui::Context) -> Outcome<(bool, Vec<Picked>)> {
        self.tick(Some(ctx));
        if self.in_flight > 0 {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
        match crate::grabview::show(self, ctx) {
            Outcome::Open => Outcome::Open,
            Outcome::Closed => Outcome::Closed,
            Outcome::Done(start) => Outcome::Done((start, self.picked())),
        }
    }
}

/// Link grabber's row height for View → Density: roomy, roomier.
pub fn row_height(density: crate::look::Density) -> f32 {
    match density {
        crate::look::Density::Compact => 22.0,
        crate::look::Density::Comfortable => 26.0,
    }
}

/// An address as the window tells links apart: without its `#fragment` (one file whatever part
/// of a page it points at).
fn address_key(u: &Url) -> String {
    let mut u = u.clone();
    u.set_fragment(None);
    u.into()
}

/// Names compared as people read them: case aside, and numbers by their value (`part2` before
/// `part10`).
fn natural(a: &str, b: &str) -> std::cmp::Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let mut n = String::new();
                while let Some(c) = x.peek().copied().filter(char::is_ascii_digit) {
                    n.push(c);
                    x.next();
                }
                let mut m = String::new();
                while let Some(d) = y.peek().copied().filter(char::is_ascii_digit) {
                    m.push(d);
                    y.next();
                }
                let (n, m) = (n.trim_start_matches('0'), m.trim_start_matches('0'));
                let by = n.len().cmp(&m.len()).then_with(|| n.cmp(m));
                if by != std::cmp::Ordering::Equal {
                    return by;
                }
            }
            (Some(c), Some(d)) => {
                let by = c.to_lowercase().cmp(d.to_lowercase());
                if by != std::cmp::Ordering::Equal {
                    return by;
                }
                x.next();
                y.next();
            }
        }
    }
}

/// What looking a link up found, as its row says it.
pub fn state_text(s: &LinkState) -> String {
    match s {
        LinkState::Unchecked => String::new(),
        LinkState::Checking => crate::i18n::t("Checking...").to_owned(),
        // i18n: Link grabber, a link's status: the server has the file
        LinkState::Online => crate::i18n::t("Online").to_owned(),
        LinkState::Offline(code) => crate::i18n::fmt(
            // i18n: Link grabber, a link's status: the server hasn't the file; {status} is the HTTP code (404)
            crate::i18n::t("Offline ({status})"),
            &[("status", &code.to_string())],
        ),
        // i18n: Link grabber, a link's status: the server wants a login (401, 403)
        LinkState::NeedsLogin => crate::i18n::t("Needs a login").to_owned(),
        // i18n: Link grabber, a link's status: the server asked to try later (429, 503)
        LinkState::Busy => crate::i18n::t("Busy, try later").to_owned(),
        LinkState::Failed(why) => why.clone(),
    }
}

/// A look-up's failure as a link's status.
pub fn state_of(e: &mdm_engine::Error) -> LinkState {
    match e {
        mdm_engine::Error::Status(s) => match s.as_u16() {
            404 | 410 => LinkState::Offline(s.as_u16()),
            401 | 403 => LinkState::NeedsLogin,
            429 | 503 => LinkState::Busy,
            _ => LinkState::Failed(crate::errors::describe(e)),
        },
        mdm_engine::Error::Throttled(..) => LinkState::Busy,
        _ => LinkState::Failed(crate::errors::describe(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(url: &str, referer: Option<&str>) -> Imported {
        Imported {
            url: url.into(),
            referer: referer.map(str::to_owned),
            ..Default::default()
        }
    }

    fn grabber() -> Grabber {
        Grabber::new(PathBuf::from("D:\\dl"), Vec::new(), true, false)
    }

    fn info(name: &str, size: Option<u64>) -> mdm_engine::ProbeInfo {
        mdm_engine::ProbeInfo {
            final_url: "https://x.org/".parse().unwrap(),
            size,
            file_name: name.into(),
            server: None,
        }
    }

    #[test]
    fn packages_by_volumes_then_page_then_various() {
        let page = Some("https://site.org/files/?sort=1");
        let mut g = grabber();
        g.add(
            vec![
                link("https://cdn.site.org/Movie.2024.part1.rar", page),
                link("https://cdn.site.org/movie.2024.PART2.rar", page),
                link("https://cdn.site.org/movie.2024.part3.rar#x", page),
                link("https://site.org/notes.txt", page),
                link("https://site.org/setup.exe", page),
                link("https://other.org/x.7z.001", None),
                link("https://other.org/x.7z.002", None),
                link("https://other.org/alone.zip", None),
                link("https://site.org/a.zip", None),
                // Again: added once.
                link("https://cdn.site.org/Movie.2024.part1.rar#again", page),
                link("https://known.org/k.zip", page),
            ],
            "Links from the browser".into(),
            |u| u.host_str() == Some("known.org"),
        );
        let names: Vec<(&str, usize)> = g
            .packages
            .iter()
            .enumerate()
            .map(|(k, p)| {
                (
                    p.name.as_str(),
                    g.links.iter().filter(|l| l.pkg == k).count(),
                )
            })
            .collect();
        assert_eq!(
            names,
            [
                ("Movie.2024", 3),
                ("site.org/files", 2),
                ("x", 2),
                ("Various files", 2),
                ("Already in the list", 1),
            ]
        );
        assert!(g.packages[0].extract, "a package of archives is extracted");
        assert!(!g.packages[1].extract);
        assert!(g.packages[3].extract, "Various holds two zips");
        let known = g.links.iter().find(|l| l.known).unwrap();
        assert!(!known.on);
        // The ticked links, each package to its folder.
        let picked = g.picked();
        assert_eq!(picked.len(), 4);
        assert_eq!(picked[0].links.len(), 3);
    }

    /// IDM's export file through Tasks → Import: every download keeps its referrer, cookies
    /// and User-Agent, and a list of several kinds of files goes each to its category's folder
    /// (the package has no folder of its own), as Import always did.
    #[test]
    fn an_idm_export_file_keeps_its_request_context() {
        let ef2 = "<\r\nhttps://files.example.org/setup.exe\r\nreferer: https://example.org/get\r\n\
                   cookie: sid=42; theme=dark\r\nUser-Agent: Mozilla/5.0 Test\r\n>\r\n\
                   <\r\nhttps://files.example.org/manual.pdf\r\nreferer: https://example.org/get\r\n\
                   cookie: sid=42\r\n>\r\n<\r\nhttps://other.org/song.mp3\r\n>\r\n";
        let found = crate::transfer::import(ef2);
        assert_eq!(found.len(), 3);
        let mut g = grabber();
        g.add(found, "list.ef2".into(), |_| false);
        let picked = g.picked();
        let all: Vec<&(Imported, String, Option<u64>)> =
            picked.iter().flat_map(|p| &p.links).collect();
        assert_eq!(all.len(), 3);
        let setup = &all.iter().find(|l| l.1 == "setup.exe").unwrap().0;
        assert_eq!(setup.referer.as_deref(), Some("https://example.org/get"));
        assert_eq!(setup.cookies.as_deref(), Some("sid=42; theme=dark"));
        assert_eq!(setup.user_agent.as_deref(), Some("Mozilla/5.0 Test"));
        let manual = &all.iter().find(|l| l.1 == "manual.pdf").unwrap().0;
        assert_eq!(manual.cookies.as_deref(), Some("sid=42"));
        // The page's package (a program and a document): no folder, each to its category's.
        let page = picked.iter().find(|p| p.name == "example.org/get").unwrap();
        assert_eq!(page.folder, PathBuf::new());
        assert!(!page.extract);
        // One kind of file: its category's folder.
        let various = picked.iter().find(|p| p.links[0].1 == "song.mp3").unwrap();
        assert_ne!(various.folder, PathBuf::new());
    }

    #[test]
    fn same_name_on_two_hosts_is_two_packages() {
        let a = Origin::of(
            &"https://a.org/x.part1.rar".parse().unwrap(),
            "x.part1.rar",
            None,
        );
        let b = Origin::of(
            &"https://b.org/x.part2.rar".parse().unwrap(),
            "x.part2.rar",
            None,
        );
        assert!(!a.same(&b));
        let c = Origin::of(
            &"https://A.org/X.part2.rar".parse().unwrap(),
            "X.part2.rar",
            None,
        );
        assert!(a.same(&c));
    }

    #[test]
    fn web_pages_hide_and_stay_out() {
        let mut g = grabber();
        g.add(
            vec![
                link("https://s.org/index.html", Some("https://s.org/")),
                link("https://s.org/a.zip", Some("https://s.org/")),
            ],
            String::new(),
            |_| false,
        );
        assert!(web_page("index.HTML") && !web_page("a.zip") && web_page("view.php"));
        assert_eq!(g.picked()[0].links.len(), 1);
        g.set_hide_pages(false);
        assert_eq!(g.picked()[0].links.len(), 2);
        assert_eq!(g.packages[0].name, "s.org");
    }

    #[test]
    fn checks_are_limited_and_answers_come_back() {
        let mut g = grabber();
        let many: Vec<Imported> = (0..20)
            .map(|k| link(&format!("https://one.org/f{k}.bin"), None))
            .chain((0..20).map(|k| link(&format!("https://h{k}.org/f.bin"), None)))
            .collect();
        g.add(many, String::new(), |_| false);
        let first = g.next_checks();
        assert_eq!(first.len(), CHECKS);
        assert_eq!(
            first
                .iter()
                .filter(|(_, u, _)| u.host_str() == Some("one.org"))
                .count(),
            CHECKS_PER_HOST
        );
        for (i, _, _) in &first {
            g.checking(*i);
        }
        assert!(g.next_checks().is_empty());
        // An answer frees its place: the next link of another host goes.
        let tx = g.answers();
        let (i, _, _) = &first[0];
        tx.send((*i, Ok(info("real-name.bin", Some(4096)))))
            .unwrap();
        g.tick(None);
        assert_eq!(g.links[*i].name, "real-name.bin");
        assert_eq!(g.links[*i].size, Some(4096));
        assert_eq!(g.looked, 1);
        assert_eq!(g.next_checks().len(), 1);
    }

    /// A link the server says isn't there is unticked; ticked by hand, it stays ticked.
    #[test]
    fn offline_links_are_unticked_unless_ticked_by_hand() {
        let mut g = grabber();
        g.add(
            vec![
                link("https://a.org/x.zip", None),
                link("https://a.org/y.zip", None),
            ],
            String::new(),
            |_| false,
        );
        g.toggle_link(1);
        g.toggle_link(1);
        let tx = g.answers();
        for i in 0..2 {
            g.checking(i);
            tx.send((i, Err(LinkState::Offline(404)))).unwrap();
        }
        g.tick(None);
        assert_eq!(g.summary().0, 1, "y.zip was ticked by hand");
        assert!(!g.links[0].on && g.links[1].on);
    }

    /// The same file linked twice (a mirror): grouped under its first link, unticked; a file
    /// of another size with the same name is no copy.
    #[test]
    fn second_links_to_a_file_are_grouped_and_unticked() {
        let mut g = grabber();
        g.add(
            vec![
                link("https://a.org/manual.pdf", None),
                link("https://mirror.org/Manual.PDF", None),
                link("https://b.org/manual.pdf?x=1", None),
                link("https://a.org/download", None),
                link("https://b.org/download", None),
            ],
            String::new(),
            |_| false,
        );
        let rows = g.rows().to_vec();
        assert_eq!(g.copies(0), 2);
        assert!(g.links[1].dup_of == Some(0) && g.links[2].dup_of == Some(0));
        assert!(!g.links[1].on && !g.links[2].on && g.links[0].on);
        // Names without an extension are compared by nothing.
        assert!(g.links[4].dup_of.is_none());
        assert!(!rows.contains(&Row::Link(1)), "closed under its first link");
        g.flip_copies(0);
        let rows = g.rows().to_vec();
        let at = rows.iter().position(|r| *r == Row::Link(0)).unwrap();
        assert_eq!(&rows[at + 1..at + 3], &[Row::Link(1), Row::Link(2)]);
        // Another size: not the same file after all.
        let tx = g.answers();
        g.checking(0);
        g.checking(2);
        tx.send((0, Ok(info("manual.pdf", Some(100))))).unwrap();
        tx.send((2, Ok(info("manual.pdf", Some(999))))).unwrap();
        g.tick(None);
        g.rows();
        assert!(g.links[2].dup_of.is_none() && g.links[2].on);
        assert_eq!(
            g.links[1].dup_of,
            Some(0),
            "its size isn't known: still a copy"
        );
    }

    /// Filters by kind and the search box; ticking by kind and what is shown.
    #[test]
    fn kinds_filter_search_and_tick() {
        let mut g = grabber();
        g.add(
            vec![
                Imported {
                    url: "https://a.org/clip.mp4".into(),
                    text: "Holiday clip".into(),
                    ..Default::default()
                },
                link("https://a.org/song.flac", None),
                link("https://a.org/pic.JPG", None),
                link("https://a.org/b.part1.rar", None),
                link("https://a.org/setup.msi", None),
                link("https://a.org/notes.pdf", None),
                link("https://a.org/data.bin2", None),
            ],
            String::new(),
            |_| false,
        );
        let kinds: Vec<Kind> = g.links.iter().map(|l| l.kind).collect();
        assert_eq!(
            kinds,
            [
                Kind::Video,
                Kind::Audio,
                Kind::Image,
                Kind::Archive,
                Kind::Program,
                Kind::Document,
                Kind::Other
            ]
        );
        assert!(
            Kind::ALL
                .iter()
                .all(|k| g.kind_counts()[k.index()] == (1, 1))
        );
        g.tick_kind(Kind::Image);
        assert_eq!(g.kind_counts()[Kind::Image.index()], (1, 0));
        assert_eq!(g.summary().0, 6);
        g.set_filter(Some(Kind::Video));
        assert_eq!(g.rows().len(), 2, "its package and the clip");
        g.tick_shown(false);
        assert_eq!(g.summary().0, 5, "only the clip was shown");
        g.set_filter(None);
        g.set_search("holiday".into());
        assert_eq!(
            g.rows()
                .iter()
                .filter(|r| matches!(r, Row::Link(_)))
                .count(),
            1,
            "found by its text"
        );
        g.set_search(String::new());
        g.invert_shown();
        assert_eq!(g.summary().0, 2);
    }

    #[test]
    fn sorted_by_any_column_and_back_to_the_page_order() {
        let mut g = grabber();
        g.add(
            vec![
                link("https://c.org/part10.zip", None),
                link("https://a.org/part2.zip", None),
                link("https://b.org/Part1.zip", None),
            ],
            String::new(),
            |_| false,
        );
        g.set_grouped(false);
        let order = |g: &mut Grabber| -> Vec<String> {
            g.rows()
                .to_vec()
                .into_iter()
                .filter_map(|r| match r {
                    Row::Link(i) => Some(g.links[i].name.clone()),
                    Row::Pkg(_) => None,
                })
                .collect()
        };
        g.sort_by(Col::Name);
        assert_eq!(order(&mut g), ["Part1.zip", "part2.zip", "part10.zip"]);
        g.sort_by(Col::Name);
        assert_eq!(order(&mut g), ["part10.zip", "part2.zip", "Part1.zip"]);
        g.sort_by(Col::Name);
        assert_eq!(order(&mut g), ["part10.zip", "part2.zip", "Part1.zip"]);
        assert_eq!(g.sort, None);
        g.sort_by(Col::Site);
        assert_eq!(order(&mut g), ["part2.zip", "Part1.zip", "part10.zip"]);
        let tx = g.answers();
        for (i, size) in [(0, 30), (1, 10)] {
            g.checking(i);
            tx.send((i, Ok(info(&g.links[i].name.clone(), Some(size)))))
                .unwrap();
        }
        g.tick(None);
        g.sort_by(Col::Size);
        assert_eq!(order(&mut g), ["part2.zip", "part10.zip", "Part1.zip"]);
        g.sort_by(Col::Size);
        assert_eq!(
            order(&mut g),
            ["part10.zip", "part2.zip", "Part1.zip"],
            "an unknown size stays last"
        );
    }

    /// A package's category: its downloads go in it, and its folder follows.
    #[test]
    fn a_package_takes_a_category_and_its_folder() {
        let mut g = grabber();
        g.add(
            vec![
                link("https://a.org/x.pdf", Some("https://a.org/p")),
                link("https://a.org/y.exe", Some("https://a.org/p")),
            ],
            String::new(),
            |_| false,
        );
        assert_eq!(g.picked()[0].folder, PathBuf::new());
        g.set_category(0, Some(Category::Programs));
        let p = &g.picked()[0];
        assert_eq!(p.category, Some(Category::Programs));
        assert!(p.folder.ends_with("Programs"), "{:?}", p.folder);
    }

    /// Thousands of links: added, shown and ticked without delay (the duplicate check, the
    /// packages' folders and the rows each go once over them).
    #[test]
    fn thousands_of_links_are_quick() {
        let mut g = grabber();
        let page = Some("https://big.org/list");
        let many: Vec<Imported> = (0..20_000)
            .map(|k| link(&format!("https://cdn{}.big.org/file{k}.zip", k % 7), page))
            .collect();
        let t = std::time::Instant::now();
        g.add(many, String::new(), |_| false);
        assert_eq!(g.rows().len(), 20_001);
        g.sort_by(Col::Name);
        g.rows();
        g.tick_shown(false);
        assert_eq!(g.summary().0, 0);
        g.set_search("file19".into());
        assert_eq!(g.rows().len(), 1 + 1 + 10 + 100 + 1000);
        let took = t.elapsed();
        assert!(took < std::time::Duration::from_secs(3), "{took:?}");
    }

    #[test]
    fn names_sort_naturally() {
        let mut names = ["b10", "B2", "a", "b1", "b02x"];
        names.sort_by(|a, b| natural(a, b));
        assert_eq!(names, ["a", "b1", "B2", "b02x", "b10"]);
    }

    #[test]
    fn links_are_taken_out() {
        let mut g = grabber();
        g.add(
            vec![
                link("https://a.org/x.zip", None),
                link("https://a.org/y.zip", None),
            ],
            String::new(),
            |_| false,
        );
        g.remove(Row::Link(0));
        assert_eq!(g.summary(), (1, 1, 0, 1));
        assert_eq!(g.picked()[0].links.len(), 1);
        g.remove(Row::Pkg(0));
        assert!(g.picked().is_empty());
        assert!(g.rows().is_empty());
    }
}
