//! What a grab takes: which sites its pages may be on (`Scope`), which files it collects (file
//! type groups and a custom list, `Types`), and the addresses it includes or leaves out
//! (wildcard `Patterns`).

use serde::{Deserialize, Serialize};
use url::Url;

/// Which pages are explored, measured from the start address.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scope {
    /// The start address's host only (`www.` or not).
    #[default]
    SameHost,
    /// Its domain and every subdomain of it (`example.com`, `img.example.com`).
    SameDomain,
    /// Anywhere the links lead (depth still limits it).
    Any,
}

impl Scope {
    /// Whether a page at `url` is in this scope of `start`.
    pub fn allows(self, start: &Url, url: &Url) -> bool {
        let (Some(a), Some(b)) = (start.host_str(), url.host_str()) else {
            return false;
        };
        match self {
            Scope::Any => true,
            Scope::SameHost => bare_host(a).eq_ignore_ascii_case(bare_host(b)),
            Scope::SameDomain => {
                let d = registrable(a);
                let b = b.to_ascii_lowercase();
                b == d || b.ends_with(&format!(".{d}"))
            }
        }
    }
}

/// Whether `url` is under `start`'s folder (`/docs/` for `/docs/intro.html`, the folder itself
/// for `/docs/`), on its host: a part of a site, not the pages above it or beside it.
pub fn below(start: &Url, url: &Url) -> bool {
    if !Scope::SameHost.allows(start, url) {
        return false;
    }
    let path = start.path();
    let folder = &path[..path.rfind('/').map_or(0, |i| i + 1)];
    let folder = folder.to_lowercase();
    let target = url.path().to_lowercase();
    target.starts_with(&folder) || format!("{target}/") == folder
}

/// `host` without a leading `www.`.
fn bare_host(host: &str) -> &str {
    match host.get(..4) {
        Some(w) if w.eq_ignore_ascii_case("www.") && host.len() > 4 => &host[4..],
        _ => host,
    }
}

/// Second-level labels that countries register names under (`example.co.uk`, `site.com.au`):
/// without the whole public suffix list, the ones most addresses use.
const SECOND_LEVEL: [&str; 12] = [
    "co", "com", "net", "org", "gov", "edu", "ac", "or", "ne", "go", "gob", "nic",
];

/// The domain a name was registered as (`img.example.co.uk` → `example.co.uk`), lowercase. An
/// IP address is itself.
pub fn registrable(host: &str) -> String {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host.parse::<std::net::IpAddr>().is_ok() || host.starts_with('[') {
        return host;
    }
    let labels: Vec<&str> = host.split('.').collect();
    let n = labels.len();
    if n <= 2 {
        return host;
    }
    let keep = if labels[n - 1].len() == 2 && SECOND_LEVEL.contains(&labels[n - 2]) {
        3
    } else {
        2
    };
    labels[n.saturating_sub(keep)..].join(".")
}

/// The groups of file types offered (each a list of extensions).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Group {
    Images,
    Video,
    Audio,
    Documents,
    Archives,
    Programs,
    /// Web pages themselves (to keep a site to read offline).
    Pages,
    /// What pages need to show: style sheets, scripts, fonts.
    Assets,
}

impl Group {
    pub const ALL: [Group; 8] = [
        Group::Images,
        Group::Video,
        Group::Audio,
        Group::Documents,
        Group::Archives,
        Group::Programs,
        Group::Pages,
        Group::Assets,
    ];

    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Group::Images => &[
                "jpg", "jpeg", "jpe", "png", "gif", "bmp", "webp", "svg", "ico", "tif", "tiff",
                "avif", "heic", "heif", "jfif",
            ],
            Group::Video => &[
                "mp4", "m4v", "mkv", "webm", "avi", "mov", "wmv", "flv", "mpg", "mpeg", "3gp",
                "ogv", "ts", "m2ts", "vob",
            ],
            Group::Audio => &[
                "mp3", "m4a", "aac", "flac", "wav", "ogg", "oga", "opus", "wma", "aiff", "aif",
                "mid", "midi", "ape",
            ],
            Group::Documents => &[
                "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp", "rtf",
                "txt", "csv", "epub", "mobi", "djvu", "md", "pps", "ppsx",
            ],
            Group::Archives => &[
                "zip", "rar", "7z", "tar", "gz", "tgz", "bz2", "tbz2", "xz", "txz", "zst", "iso",
                "cab", "lz", "lzh", "arj", "z",
            ],
            Group::Programs => &[
                "exe",
                "msi",
                "msix",
                "appx",
                "dmg",
                "pkg",
                "deb",
                "rpm",
                "apk",
                "appimage",
                "bin",
                "run",
                "jar",
                "flatpakref",
            ],
            Group::Pages => &[
                "html", "htm", "shtml", "xhtml", "php", "asp", "aspx", "jsp", "cgi", "cfm",
            ],
            Group::Assets => &[
                "css", "js", "mjs", "woff", "woff2", "ttf", "otf", "eot", "json", "xml",
            ],
        }
    }

    /// The group extension `ext` (lowercase, no dot) belongs to.
    pub fn of(ext: &str) -> Option<Group> {
        Group::ALL
            .into_iter()
            .find(|g| g.extensions().contains(&ext))
    }
}

/// The file types a grab collects: groups and extensions of the user's own.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Types {
    groups: Vec<Group>,
    custom: Vec<String>,
}

impl Types {
    /// `custom`: extensions separated by spaces, commas or semicolons, with or without `*.`.
    pub fn new(groups: &[Group], custom: &str) -> Types {
        Types {
            groups: groups.to_vec(),
            custom: split_extensions(custom),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.groups.is_empty() && self.custom.is_empty()
    }

    pub fn has(&self, g: Group) -> bool {
        self.groups.contains(&g)
    }

    /// Whether a file named `name` is one to collect.
    pub fn wants(&self, name: &str) -> bool {
        let Some(ext) = extension(name) else {
            return false;
        };
        self.custom.iter().any(|c| c == &ext)
            || Group::of(&ext).is_some_and(|g| self.groups.contains(&g))
    }
}

/// Extensions as typed (`.mp4, *.mkv;avi`): lowercase, without dots and stars.
pub fn split_extensions(typed: &str) -> Vec<String> {
    typed
        .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
        .map(|t| {
            t.trim_start_matches('*')
                .trim_start_matches('.')
                .to_ascii_lowercase()
        })
        .filter(|t| !t.is_empty())
        .collect()
}

/// `name`'s extension, lowercase (`photo.JPG` → `jpg`); none for `README` or `.hidden`.
pub fn extension(name: &str) -> Option<String> {
    let (stem, ext) = name.rsplit_once('.')?;
    (!stem.is_empty()
        && !ext.is_empty()
        && ext.len() <= 10
        && ext.chars().all(|c| c.is_ascii_alphanumeric()))
    .then(|| ext.to_ascii_lowercase())
}

/// The last part of `url`'s path, decoded (`/a/My%20File.pdf` → `My File.pdf`); empty for a
/// folder (`/a/`).
pub fn last_segment(url: &Url) -> String {
    let seg = url
        .path_segments()
        .and_then(|mut s| s.next_back())
        .unwrap_or_default();
    percent_encoding::percent_decode_str(seg)
        .decode_utf8_lossy()
        .into_owned()
}

/// Whether a link to `url` may be a page to explore: a folder, a name without an extension or a
/// page's extension. Others (`.zip`, `.jpg`) are files.
pub fn may_be_page(url: &Url) -> bool {
    match extension(&last_segment(url)) {
        None => true,
        Some(ext) => Group::Pages.extensions().contains(&ext.as_str()) || ext == "pl",
    }
}

/// The extension a file of MIME type `mime` usually has (for a name without one).
pub fn extension_for_mime(mime: &str) -> Option<&'static str> {
    let m = mime
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    Some(match m.as_str() {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        "image/avif" => "avif",
        "image/bmp" => "bmp",
        "image/x-icon" | "image/vnd.microsoft.icon" => "ico",
        "video/mp4" => "mp4",
        "video/webm" => "webm",
        "video/x-matroska" => "mkv",
        "video/quicktime" => "mov",
        "audio/mpeg" => "mp3",
        "audio/mp4" => "m4a",
        "audio/ogg" => "ogg",
        "audio/flac" => "flac",
        "audio/wav" | "audio/x-wav" => "wav",
        "application/pdf" => "pdf",
        "application/zip" | "application/x-zip-compressed" => "zip",
        "application/x-7z-compressed" => "7z",
        "application/vnd.rar" | "application/x-rar-compressed" => "rar",
        "application/gzip" | "application/x-gzip" => "gz",
        "application/x-msdownload" | "application/vnd.microsoft.portable-executable" => "exe",
        "application/x-msi" => "msi",
        "application/epub+zip" => "epub",
        "text/plain" => "txt",
        "text/css" => "css",
        "text/javascript" | "application/javascript" => "js",
        "text/html" | "application/xhtml+xml" => "html",
        _ => return None,
    })
}

/// Addresses to include or leave out: `*` stands for any text and `?` for one character, and a
/// pattern with neither matches any address that contains it. Letter case doesn't count.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Patterns(Vec<String>);

impl Patterns {
    /// Patterns separated by spaces or new lines.
    pub fn parse(typed: &str) -> Patterns {
        Patterns(typed.split_whitespace().map(str::to_lowercase).collect())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether any pattern matches `address`.
    pub fn matches(&self, address: &str) -> bool {
        let address = address.to_lowercase();
        self.0.iter().any(|p| {
            if p.contains(['*', '?']) {
                wildcard(p, &address)
            } else {
                address.contains(p.as_str())
            }
        })
    }
}

/// `text` matched whole by `pattern`: `*` any run of characters, `?` one character.
pub fn wildcard(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    // The classic greedy match with one backtrack point (linear for `*`-only patterns).
    let (mut pi, mut ti) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        // `*` first: a pattern's `*` meeting a `*` in the address is still the wildcard.
        if pi < p.len() && p[pi] == '*' {
            star = Some((pi, ti));
            pi += 1;
        } else if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if let Some((sp, st)) = star {
            pi = sp + 1;
            ti = st + 1;
            star = Some((sp, st + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|c| *c == '*')
}

/// Where a file of `url` goes to keep the site's folders: its host (and port when not the
/// scheme's own), then each folder of its path, each made a valid folder name.
pub fn site_folders(url: &Url) -> Vec<String> {
    let mut host = url.host_str().unwrap_or("site").to_owned();
    if let Some(port) = url.port() {
        host = format!("{host}_{port}");
    }
    let mut out = vec![mdm_engine::naming::sanitize(&host)];
    if let Some(segs) = url.path_segments() {
        let segs: Vec<&str> = segs.collect();
        for s in &segs[..segs.len().saturating_sub(1)] {
            let s = percent_encoding::percent_decode_str(s).decode_utf8_lossy();
            if s.is_empty() || s == "." || s == ".." {
                continue;
            }
            out.push(mdm_engine::naming::sanitize(&s));
        }
    }
    out
}

/// The name a page at `url` is saved under: its own name, `index.html` for a folder, `.html`
/// added to a name without a page's extension.
pub fn page_name(url: &Url) -> String {
    let last = last_segment(url);
    if last.is_empty() {
        return "index.html".into();
    }
    let name = mdm_engine::naming::sanitize(&last);
    match extension(&name) {
        Some(ext) if Group::Pages.extensions().contains(&ext.as_str()) => name,
        _ => format!("{name}.html"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn wildcards() {
        assert!(wildcard("*.jpg", "https://a/b/c.jpg"));
        assert!(!wildcard("*.jpg", "https://a/b/c.jpg.html"));
        assert!(wildcard("*/gallery/*", "https://a/gallery/1.png"));
        assert!(wildcard("https://?/*", "https://a/x"));
        assert!(!wildcard("https://?/*", "https://ab/x"));
        assert!(wildcard("*", ""));
        assert!(wildcard("a*b*c", "aXXbYYc"));
        assert!(!wildcard("a*b*c", "aXXbYY"));
        assert!(wildcard("**a", "ba"));
    }

    #[test]
    fn patterns_without_wildcards_match_anywhere_and_case_does_not_count() {
        let p = Patterns::parse("thumb  *.PNG\n*/big/*");
        assert!(p.matches("https://x/THUMBS/1.jpg"));
        assert!(p.matches("https://x/a.png"));
        assert!(p.matches("https://x/big/a.jpg"));
        assert!(!p.matches("https://x/small/a.jpg"));
        assert!(Patterns::parse("  ").is_empty());
        assert!(!Patterns::parse("").matches("anything"));
    }

    #[test]
    fn scopes() {
        let start = u("https://www.example.co.uk/a/");
        let same_host = u("http://example.co.uk/b");
        let sub = u("https://img.example.co.uk/c.jpg");
        let other = u("https://other.example/");
        assert!(Scope::SameHost.allows(&start, &same_host));
        assert!(!Scope::SameHost.allows(&start, &sub));
        assert!(Scope::SameDomain.allows(&start, &sub));
        assert!(Scope::SameDomain.allows(&start, &same_host));
        assert!(!Scope::SameDomain.allows(&start, &other));
        assert!(!Scope::SameDomain.allows(&u("https://notexample.co.uk/"), &sub));
        assert!(Scope::Any.allows(&start, &other));
        assert_eq!(registrable("a.b.example.com"), "example.com");
        assert_eq!(registrable("example.com.au"), "example.com.au");
        assert_eq!(registrable("x.example.com.au"), "example.com.au");
        assert_eq!(registrable("127.0.0.1"), "127.0.0.1");
        assert_eq!(registrable("localhost"), "localhost");
    }

    #[test]
    fn part_of_a_site_is_its_start_folder_and_below() {
        let start = u("https://www.example.com/docs/intro.html");
        assert!(below(&start, &u("https://example.com/docs/")));
        assert!(below(&start, &u("https://example.com/docs/a/b.html")));
        assert!(
            below(&start, &u("https://example.com/docs")),
            "the folder without its slash"
        );
        assert!(!below(&start, &u("https://example.com/")));
        assert!(!below(&start, &u("https://example.com/docsx/a.html")));
        assert!(!below(&start, &u("https://other.example/docs/a.html")));
        let top = u("https://example.com/");
        assert!(below(&top, &u("https://example.com/anything/x.html")));
    }

    #[test]
    fn types_by_group_and_custom_extensions() {
        let t = Types::new(&[Group::Images], ".psd, *.XCF;raw");
        assert!(t.wants("a.JPG"));
        assert!(t.wants("layers.psd"));
        assert!(t.wants("x.xcf"));
        assert!(!t.wants("a.zip"));
        assert!(!t.wants("README"));
        assert!(!t.wants(".jpg"));
        assert!(Types::new(&[], "").is_empty());
        assert_eq!(Group::of("mkv"), Some(Group::Video));
        assert_eq!(Group::of("nope"), None);
        // Every extension is in one group only.
        for g in Group::ALL {
            for e in g.extensions() {
                assert_eq!(Group::of(e), Some(g), "{e}");
            }
        }
    }

    #[test]
    fn pages_and_files_are_told_apart_by_their_address() {
        assert!(may_be_page(&u("https://a/")));
        assert!(may_be_page(&u("https://a/about")));
        assert!(may_be_page(&u("https://a/p.php?id=3")));
        assert!(may_be_page(&u("https://a/v1.2/")));
        assert!(!may_be_page(&u("https://a/x.zip")));
        assert!(!may_be_page(&u("https://a/photo.jpg?w=200")));
    }

    #[test]
    fn local_folders_and_page_names() {
        assert_eq!(
            site_folders(&u("https://Example.com/a/b%20c/x.jpg")),
            ["example.com", "a", "b c"]
        );
        assert_eq!(
            site_folders(&u("http://127.0.0.1:8080/../x/y/")),
            ["127.0.0.1_8080", "x", "y"]
        );
        assert_eq!(site_folders(&u("https://h/x.jpg")), ["h"]);
        assert_eq!(page_name(&u("https://h/a/")), "index.html");
        assert_eq!(page_name(&u("https://h/about")), "about.html");
        assert_eq!(page_name(&u("https://h/p.php?id=1")), "p.php");
        assert_eq!(extension_for_mime("image/JPEG; q=1"), Some("jpg"));
        assert_eq!(extension_for_mime("application/x-unknown"), None);
    }

    #[test]
    fn a_star_in_the_address_is_matched_by_the_patterns_star() {
        assert!(wildcard("https://a/*.jpg", "https://a/*x.jpg"));
        assert!(wildcard("*", "*"));
        assert!(wildcard("a*c", "a*b*c"));
        assert!(!wildcard("a*c", "a*b*d"));
    }
}
