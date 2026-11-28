//! A site saved to read offline ("Whole site for offline reading", or any project that makes its
//! pages work offline): where each file found goes on the disk ([`local_path`]), and its pages and
//! style sheets changed so that their links lead to those copies ([`rewrite_html`],
//! [`rewrite_css`]). A link to a file that wasn't saved is made absolute, so it still leads to the
//! site; a link to another site, an e-mail address or a script stays as it was.
//!
//! Only the links change: every other byte of a page stays as the site sent it (a page in an older
//! encoding than UTF-8 stays in it). A copy changed once and changed again, after a later run saved
//! more files, comes out as if changed once with all of them: a link that already leads to a saved
//! file is recognised and left alone.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::{Component, Path, PathBuf};

use percent_encoding::{AsciiSet, CONTROLS, percent_decode_str, utf8_percent_encode};
use url::Url;

use crate::crawl::{Found, FoundKind};
use crate::extract::{self, Holds, Piece, Tag};
use crate::filter;

/// The longest folder or file name an offline copy makes: a longer one is cut, with a number made
/// from all of it that keeps two such names apart (a path too long for Windows fails to save).
const MAX_PART: usize = 80;
/// The longest query a file's name keeps whole (`post@id=2.html`).
const MAX_QUERY: usize = 40;

/// What is changed in a saved file, by its name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Text {
    Html,
    Css,
}

/// Whether the saved file `file` has links to change, and which kind of text it is.
pub fn text_of(file: &Path) -> Option<Text> {
    let ext = file.extension()?.to_string_lossy().to_ascii_lowercase();
    match ext.as_str() {
        "html" | "htm" | "xhtml" | "shtml" => Some(Text::Html),
        "css" => Some(Text::Css),
        _ => None,
    }
}

/// Where `found` goes in an offline copy, relative to the copy's folder: the site's folder (its
/// host, and its port when not the scheme's own), each folder of its address, then its name. A
/// page's name ends in `.html` so a browser opens it from the disk (`about` → `about.html`, a
/// folder's page → `index.html`, `list.php` → `list.php.html`); an address with a query keeps it in
/// the name (`post?id=2` → `post@id=2.html`, `photo.jpg?w=200` → `photo@w=200.jpg`).
pub fn local_path(found: &Found) -> PathBuf {
    let Ok(url) = Url::parse(&found.url) else {
        return PathBuf::from(mdm_engine::naming::sanitize(&found.name));
    };
    let mut path = PathBuf::new();
    for part in filter::site_folders(&url) {
        path.push(clamp(&part));
    }
    path.push(local_name(&url, found));
    path
}

/// A shorter place for `found` than [`local_path`]'s, for a folder deep enough that the full one
/// would pass Windows' limit on a path's length: the site's folder, then `_` and a name made
/// from its address, with its extension (`example.com/_/1f3a….html`).
pub fn short_path(found: &Found) -> PathBuf {
    let ext = if found.kind == FoundKind::Page {
        Some("html".to_owned())
    } else {
        filter::extension(&found.name)
    };
    let name = match ext {
        Some(e) => format!("{:016x}.{e}", fnv(&found.url)),
        None => format!("{:016x}", fnv(&found.url)),
    };
    let host = Url::parse(&found.url)
        .ok()
        .and_then(|u| filter::site_folders(&u).into_iter().next())
        .unwrap_or_else(|| "site".to_owned());
    PathBuf::from(clamp(&host)).join("_").join(name)
}

/// `path` made different from every path in `taken` (compared as the disk does, letter case
/// aside where it doesn't tell them apart): `name~2.ext`, `name~3.ext`…; then added to `taken`.
pub fn unique_in(path: PathBuf, taken: &mut HashSet<String>) -> PathBuf {
    let mut candidate = path.clone();
    let mut n = 1;
    while !taken.insert(key(&candidate)) {
        n += 1;
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let name = match path.extension() {
            Some(e) => format!("{stem}~{n}.{}", e.to_string_lossy()),
            None => format!("{stem}~{n}"),
        };
        candidate = path.with_file_name(name);
    }
    candidate
}

/// `p` as [`unique_in`] compares it.
pub fn place_key(p: &Path) -> String {
    key(p)
}

/// The name of `found`'s file in an offline copy ([`local_path`]).
fn local_name(url: &Url, found: &Found) -> String {
    let page = found.kind == FoundKind::Page;
    let last = filter::last_segment(url);
    let name = if last.is_empty() {
        if page {
            "index.html".to_owned()
        } else {
            found.name.clone()
        }
    } else if !page
        && filter::extension(&last).is_none()
        && filter::extension(&found.name).is_some()
    {
        // An address that names no file (`/download?id=7`): the name the server gave it.
        found.name.clone()
    } else {
        last
    };
    let name = mdm_engine::naming::sanitize(&name);
    let query = url
        .query()
        .filter(|q| !q.is_empty())
        .map(|q| format!("@{}", query_part(q)))
        .unwrap_or_default();
    let ext = filter::extension(&name);
    let page_ext = matches!(ext.as_deref(), Some("html" | "htm"));
    let (stem, ext) = match ext {
        // A page of another extension keeps it before `.html`: `list.php` → `list.php.html`.
        Some(_) if page && !page_ext => (format!("{name}{query}"), "html".to_owned()),
        Some(e) => {
            let stem = &name[..name.len() - e.len() - 1];
            (format!("{stem}{query}"), e)
        }
        None if page => (format!("{name}{query}"), "html".to_owned()),
        None => (format!("{name}{query}"), String::new()),
    };
    let stem = clamp(&stem);
    if ext.is_empty() {
        stem
    } else {
        format!("{stem}.{ext}")
    }
}

/// A query as part of a file's name: what Windows refuses in a name made `_`, a long one cut
/// with a number made from all of it.
fn query_part(q: &str) -> String {
    let q: String = q
        .chars()
        .map(|c| {
            if c.is_control() || r#"<>:"/\|?*"#.contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    if q.chars().count() <= MAX_QUERY {
        return q;
    }
    let head: String = q.chars().take(MAX_QUERY - 9).collect();
    format!("{head}-{:08x}", fnv(&q) as u32)
}

/// `part` cut to [`MAX_PART`] characters when longer, a number made from all of it at its end.
fn clamp(part: &str) -> String {
    if part.chars().count() <= MAX_PART {
        return part.to_owned();
    }
    let head: String = part.chars().take(MAX_PART - 9).collect();
    format!(
        "{}-{:08x}",
        head.trim_end_matches(['.', ' ']),
        fnv(part) as u32
    )
}

/// FNV-1a: a number from text that is the same in every run and every build (a name made from
/// it must be found again by a later run).
fn fnv(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// An offline copy's files: each address saved and its file on the disk, and the addresses that
/// led to a saved page (its address before a redirect).
#[derive(Clone, Debug, Default)]
pub struct Copy {
    files: HashMap<String, PathBuf>,
    /// The files, as [`key`] makes them comparable.
    on_disk: HashSet<String>,
}

impl Copy {
    /// The copy of the files found, each saved at the path given.
    pub fn of<'a>(saved: impl IntoIterator<Item = (&'a Found, PathBuf)>) -> Copy {
        let mut copy = Copy::default();
        for (found, file) in saved {
            for also in &found.also {
                copy.add(also, file.clone());
            }
            copy.add(&found.url, file);
        }
        copy
    }

    /// Address `url` is saved at `file`.
    pub fn add(&mut self, url: &str, file: PathBuf) {
        self.on_disk.insert(key(&file));
        let url = Url::parse(url).map_or_else(
            |_| url.to_owned(),
            |mut u| {
                u.set_fragment(None);
                u.to_string()
            },
        );
        self.files.insert(url, file);
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Where address `url` is saved, if it is.
    pub fn file(&self, url: &Url) -> Option<&Path> {
        self.files.get(url.as_str()).map(PathBuf::as_path)
    }

    /// Whether `file` is one of the copy's files.
    fn holds(&self, file: &Path) -> bool {
        self.on_disk.contains(&key(file))
    }
}

/// `p` made comparable: `.` and `..` worked out, `/` between its parts, and in lower case where
/// the system's disks don't tell cases apart (Windows, macOS).
fn key(p: &Path) -> String {
    let parts: Vec<String> = normalize(p)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    let s = parts.join("/");
    if cfg!(any(windows, target_os = "macos")) {
        s.to_lowercase()
    } else {
        s
    }
}

/// `p` with `.` and `..` worked out (without asking the disk).
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// What a link written in a copy's file escapes: what a web address can't hold as it is, and
/// what would end it early in an attribute or a CSS `url()`.
const PATH: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}')
    .add(b'|')
    .add(b'\\')
    .add(b'^')
    .add(b'[')
    .add(b']')
    .add(b'\'')
    .add(b'(')
    .add(b')');

/// The link from a file in folder `from` to file `to`: relative (`../img/a.png`), each part
/// percent-encoded; a `file:` address when they are on different drives.
pub fn relative(from: &Path, to: &Path) -> String {
    let (from, to_n) = (normalize(from), normalize(to));
    let a: Vec<Component> = from.components().collect();
    let b: Vec<Component> = to_n.components().collect();
    let same = |x: &Component, y: &Component| {
        let (x, y) = (
            x.as_os_str().to_string_lossy(),
            y.as_os_str().to_string_lossy(),
        );
        if cfg!(any(windows, target_os = "macos")) {
            x.to_lowercase() == y.to_lowercase()
        } else {
            x == y
        }
    };
    let mut n = 0;
    while n < a.len() && n < b.len() && same(&a[n], &b[n]) {
        n += 1;
    }
    // Nothing in common (another drive, or one of them relative): the file's own address.
    let rooted = |c: &[Component]| {
        c.first()
            .is_some_and(|c| matches!(c, Component::Prefix(_) | Component::RootDir))
    };
    if n == 0 && (rooted(&a) || rooted(&b)) {
        return Url::from_file_path(to).map_or_else(|()| String::new(), |u| u.to_string());
    }
    let mut parts: Vec<String> = vec!["..".to_owned(); a.len() - n];
    for c in &b[n..] {
        parts.push(utf8_percent_encode(&c.as_os_str().to_string_lossy(), PATH).to_string());
    }
    parts.join("/")
}

/// One change to a file: these bytes become this text.
type Edit = (Range<usize>, String);

/// How links in one file are changed.
struct Rewriter<'a> {
    /// What the file's relative links are resolved against (its address, or its `<base href>`).
    base: Url,
    /// The folder the file is in.
    dir: &'a Path,
    copy: &'a Copy,
}

impl Rewriter<'_> {
    /// Link `v` as the file should have it: Some(new) when it changes; and whether it leads to a
    /// file of the copy.
    fn link(&self, v: &str) -> (Option<String>, bool) {
        let t = v.trim();
        if t.is_empty() || t.starts_with('#') {
            return (None, false);
        }
        let scheme = scheme_of(t);
        // mailto:, javascript:, data:, tel:…: as they are.
        if scheme
            .is_some_and(|s| !s.eq_ignore_ascii_case("http") && !s.eq_ignore_ascii_case("https"))
        {
            return (None, false);
        }
        let (address, fragment) = match t.find('#') {
            Some(i) => (&t[..i], &t[i..]),
            None => (t, ""),
        };
        // Already one of the copy's files (changed so before): as it is.
        if scheme.is_none() && !t.starts_with(['/', '\\']) && self.leads_to_copy(address) {
            return (None, true);
        }
        let Some(url) = extract::resolve(&self.base, address) else {
            return (None, false);
        };
        if let Some(file) = self.copy.file(&url) {
            return (
                Some(format!("{}{fragment}", relative(self.dir, file))),
                true,
            );
        }
        if scheme.is_some() {
            return (None, false);
        }
        // Not saved: to the site, so it still works.
        (Some(format!("{url}{fragment}")), false)
    }

    /// Whether relative link `address` (no fragment) leads to one of the copy's files from the
    /// file's folder.
    fn leads_to_copy(&self, address: &str) -> bool {
        if address.is_empty() || address.contains('?') {
            return false;
        }
        let path = percent_decode_str(address).decode_utf8_lossy();
        let mut file = self.dir.to_path_buf();
        for part in path.split('/') {
            file.push(part);
        }
        self.copy.holds(&file)
    }

    /// A `srcset` with its addresses changed: Some(new) when one changes.
    fn srcset(&self, v: &str, local: &mut bool) -> Option<String> {
        let mut changed = false;
        let parts: Vec<String> = extract::srcset_candidates(v)
            .into_iter()
            .map(|(url, descriptors)| {
                let (new, here) = self.link(&url);
                *local |= here;
                changed |= new.is_some();
                let url = new.unwrap_or(url);
                if descriptors.is_empty() {
                    url
                } else {
                    format!("{url} {descriptors}")
                }
            })
            .collect();
        changed.then(|| parts.join(", "))
    }

    /// The changes to CSS text `css` (its own places).
    fn css_edits(&self, css: &str) -> Vec<Edit> {
        let mut edits = Vec::new();
        for u in extract::css_urls(css) {
            // An address written in an older encoding than UTF-8 (`text_for`): as it is.
            if css[u.token.clone()].contains('\u{1}') {
                continue;
            }
            let (Some(new), _) = self.link(&u.href) else {
                continue;
            };
            let new = css_string(&new);
            edits.push((u.token, if u.is_url { format!("url({new})") } else { new }));
        }
        edits
    }

    /// CSS text with its addresses changed: Some(new) when one changes.
    fn css_text(&self, css: &str) -> Option<String> {
        let edits = self.css_edits(css);
        apply(css.as_bytes(), edits).map(|b| String::from_utf8_lossy(&b).into_owned())
    }

    /// A refresh's content with its address changed.
    fn refresh(&self, v: &str) -> Option<String> {
        let (at, to) = extract::refresh_target(v)?;
        let (new, _) = self.link(to);
        new.map(|new| format!("{}{new}{}", &v[..at], &v[at + to.len()..]))
    }

    /// The changes to one tag.
    fn tag(&self, t: &Tag, edits: &mut Vec<Edit>) {
        if t.name == "base" {
            // Its links are made for the file's own place: the base would lead them astray.
            if let Some(i) = t.index("href")
                && let Some(raw) = &t.attrs[i].raw
                && !t.attrs[i].value.is_empty()
            {
                edits.push((raw.clone(), "\"\"".to_owned()));
            }
            return;
        }
        let mut local = false;
        let mut lazy: Option<String> = None;
        for (i, holds) in extract::values(t) {
            let a = &t.attrs[i];
            let Some(raw) = a.raw.clone() else { continue };
            // Bytes that aren't UTF-8 (an older encoding): left as they are.
            if a.value.contains('\u{fffd}') {
                continue;
            }
            let v = extract::decode(&a.value);
            let new = match holds {
                Holds::Url(_) | Holds::Address => {
                    let (new, here) = self.link(&v);
                    local |= here;
                    if here && t.name == "img" && extract::LAZY.contains(&a.name.as_str()) {
                        lazy = lazy.or_else(|| Some(new.clone().unwrap_or_else(|| v.clone())));
                    }
                    new
                }
                Holds::Meta => None,
                Holds::Srcset => self.srcset(&v, &mut local),
                Holds::Css => self.css_text(&v),
                Holds::Refresh => self.refresh(&v),
            };
            if let Some(new) = new {
                edits.push((raw, quoted(&new)));
            }
        }
        if local {
            // A check of the site's file (`integrity`) would refuse the changed copy, and a
            // cross-origin request is refused to a file on the disk.
            if let Some(i) = t.index("integrity")
                && let Some(raw) = &t.attrs[i].raw
                && !t.attrs[i].value.is_empty()
            {
                edits.push((raw.clone(), "\"\"".to_owned()));
            }
            if let Some(i) = t.index("crossorigin") {
                edits.push((t.attrs[i].whole.clone(), String::new()));
            }
        }
        // A picture loaded late by a script, behind a placeholder: shown without the script.
        if let Some(lazy) = lazy {
            match t.index("src") {
                Some(i) => {
                    let a = &t.attrs[i];
                    let placeholder = a.value.trim().is_empty()
                        || a.value
                            .trim_start()
                            .to_ascii_lowercase()
                            .starts_with("data:");
                    if placeholder && let Some(raw) = &a.raw {
                        edits.push((raw.clone(), quoted(&lazy)));
                    }
                }
                None => edits.push((t.name_end..t.name_end, format!(" src={}", quoted(&lazy)))),
            }
        }
    }
}

/// `s` as an attribute's value, in double quotes.
fn quoted(s: &str) -> String {
    format!("\"{}\"", s.replace('&', "&amp;").replace('"', "&quot;"))
}

/// `s` as a CSS string, in single quotes.
fn css_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        match c {
            '\\' | '\'' => {
                out.push('\\');
                out.push(c);
            }
            '\n' => out.push_str("\\a "),
            _ => out.push(c),
        }
    }
    out.push('\'');
    out
}

/// The scheme `link` starts with (`mailto` in `mailto:x`), if it has one.
fn scheme_of(link: &str) -> Option<&str> {
    let colon = link.find(':')?;
    let scheme = &link[..colon];
    let mut chars = scheme.chars();
    (chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')))
    .then_some(scheme)
}

/// `bytes` with `edits` made (in order; one overlapping an earlier one is left out); None when
/// that changes nothing.
fn apply(bytes: &[u8], mut edits: Vec<Edit>) -> Option<Vec<u8>> {
    if edits.is_empty() {
        return None;
    }
    edits.sort_by_key(|(r, _)| (r.start, r.end));
    let mut out = Vec::with_capacity(bytes.len() + edits.len() * 16);
    let mut at = 0;
    for (r, text) in edits {
        if r.start < at || r.end > bytes.len() {
            continue;
        }
        out.extend_from_slice(&bytes[at..r.start]);
        out.extend_from_slice(text.as_bytes());
        at = r.end;
    }
    out.extend_from_slice(&bytes[at..]);
    (out != bytes).then_some(out)
}

/// Whether `bytes` is text whose markup is ASCII, as a page byte by byte (not UTF-16, not
/// compressed).
fn readable(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(1024)];
    !(head.starts_with(&[0xff, 0xfe])
        || head.starts_with(&[0xfe, 0xff])
        || head.starts_with(&[0x1f, 0x8b])
        || head.iter().filter(|&&b| b == 0).count() > head.len() / 8)
}

/// `bytes` as text to look for addresses in: as it is when it is UTF-8, else each byte beyond
/// ASCII made `\u{1}` (the same length: the places found are the bytes' own, and an address
/// holding one is left as it is).
fn text_for(bytes: &[u8]) -> Cow<'_, str> {
    match std::str::from_utf8(bytes) {
        Ok(s) => Cow::Borrowed(s),
        Err(_) => Cow::Owned(
            bytes
                .iter()
                .map(|&b| if b.is_ascii() { b as char } else { '\u{1}' })
                .collect(),
        ),
    }
}

/// The HTML page `page`, saved at `file` from address `url`, with its links leading to the copy's
/// files; None when nothing changes, or when it isn't a page to change (UTF-16, compressed).
pub fn rewrite_html(page: &[u8], url: &Url, file: &Path, copy: &Copy) -> Option<Vec<u8>> {
    if !readable(page) {
        return None;
    }
    // The first `<base href>` counts for the whole page.
    let mut base = None;
    extract::scan(page, |p| {
        if let Piece::Tag(t) = p
            && t.name == "base"
            && base.is_none()
            && let Some(h) = t.attr("href")
        {
            base = Some(extract::decode(h.trim()));
        }
    });
    let base = base
        .filter(|b| !b.is_empty())
        .and_then(|b| url.join(&b).ok())
        .filter(|b| matches!(b.scheme(), "http" | "https"))
        .unwrap_or_else(|| url.clone());
    let dir = file.parent().unwrap_or(Path::new(""));
    let r = Rewriter { base, dir, copy };
    let mut edits = Vec::new();
    extract::scan(page, |p| match p {
        Piece::Tag(t) => r.tag(&t, &mut edits),
        Piece::Style(range) => {
            let bytes = &page[range.clone()];
            let text = text_for(bytes);
            for (sub, new) in r.css_edits(&text) {
                edits.push((range.start + sub.start..range.start + sub.end, new));
            }
        }
    });
    apply(page, edits)
}

/// The style sheet `css`, saved at `file` from address `url`, with its `url()`s and `@import`s
/// leading to the copy's files; None when nothing changes.
pub fn rewrite_css(css: &[u8], url: &Url, file: &Path, copy: &Copy) -> Option<Vec<u8>> {
    if !readable(css) {
        return None;
    }
    let dir = file.parent().unwrap_or(Path::new(""));
    let r = Rewriter {
        base: url.clone(),
        dir,
        copy,
    };
    let text = text_for(css);
    apply(css, r.css_edits(&text))
}

/// A page at the copy's top that opens its start page (`target`, relative to it): the project's
/// `name` and start `address` shown, for a browser that doesn't follow the refresh.
pub fn start_page(name: &str, address: &str, target: &str) -> String {
    let esc = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    };
    let (name, address, target) = (esc(name), esc(address), esc(target));
    format!(
        "<!doctype html>\n<!-- Dannex Download Manager: offline copy -->\n<html><head><meta charset=\"utf-8\">\n\
         <meta http-equiv=\"refresh\" content=\"0; url={target}\">\n<title>{name}</title></head>\n\
         <body><p><a href=\"{target}\">{name}</a></p><p>{address}</p></body></html>\n"
    )
}

/// Whether `text` is a start page [`start_page`] made (a file of the user's own is never
/// overwritten).
pub fn is_start_page(text: &str) -> bool {
    text.contains("<!-- Dannex Download Manager: offline copy -->")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(url: &str, kind: FoundKind, name: &str) -> Found {
        Found {
            url: url.into(),
            page: String::new(),
            name: name.into(),
            size: None,
            kind,
            also: Vec::new(),
        }
    }

    fn local(url: &str, kind: FoundKind) -> String {
        let name = filter::last_segment(&Url::parse(url).unwrap());
        local_path(&found(url, kind, &name))
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/")
    }

    #[test]
    fn files_go_where_the_address_says_and_pages_end_in_html() {
        use FoundKind::*;
        assert_eq!(
            local("https://Example.com/", Page),
            "example.com/index.html"
        );
        assert_eq!(
            local("https://example.com/docs/", Page),
            "example.com/docs/index.html"
        );
        assert_eq!(
            local("https://example.com/about", Page),
            "example.com/about.html"
        );
        assert_eq!(
            local("https://example.com/list.php", Page),
            "example.com/list.php.html"
        );
        assert_eq!(
            local("https://example.com/blog/post?id=2", Page),
            "example.com/blog/post@id=2.html"
        );
        assert_eq!(
            local("https://example.com/a.htm?x=1&y=2", Page),
            "example.com/a@x=1&y=2.htm"
        );
        assert_eq!(
            local("http://127.0.0.1:8080/img/photo.jpg?w=200", File),
            "127.0.0.1_8080/img/photo@w=200.jpg"
        );
        assert_eq!(
            local("https://example.com/css/site.css", Style),
            "example.com/css/site.css"
        );
        assert_eq!(
            local("https://example.com/caf%C3%A9/men%C3%BC.html", Page),
            "example.com/café/menü.html"
        );
        // What Windows refuses in a name: made `_`; a reserved device name kept apart.
        assert_eq!(
            local("https://example.com/a:b/con.txt?q=<x>|y", File),
            "example.com/a_b/_con@q=%3Cx%3E_y.txt"
        );
        // An address that names no file takes the name the server gave.
        let f = found("https://example.com/download?id=7", File, "bundle.zip");
        assert_eq!(
            local_path(&f),
            Path::new("example.com").join("bundle@id=7.zip")
        );
    }

    #[test]
    fn long_names_and_queries_are_cut_and_kept_apart() {
        let long = "x".repeat(200);
        let a = local(&format!("https://h/{long}a.png"), FoundKind::File);
        let b = local(&format!("https://h/{long}b.png"), FoundKind::File);
        assert_ne!(a, b);
        let name = a.rsplit('/').next().unwrap();
        assert!(name.chars().count() <= MAX_PART + 4, "{name}");
        assert!(name.ends_with(".png"));
        let q = local(&format!("https://h/p?{long}"), FoundKind::Page);
        assert!(q.len() < 80, "{q}");
        // The same address gives the same name in every run.
        assert_eq!(a, local(&format!("https://h/{long}a.png"), FoundKind::File));
    }

    #[test]
    fn a_place_too_long_or_taken_gets_another() {
        let f = found(
            "https://h.test/a/very/deep/page?x=1",
            FoundKind::Page,
            "page",
        );
        let short = short_path(&f);
        assert!(short.starts_with("h.test"));
        assert_eq!(short.extension().unwrap(), "html");
        assert_eq!(short, short_path(&f), "the same in every run");
        let mut taken = HashSet::new();
        let a = unique_in(PathBuf::from("h/A.html"), &mut taken);
        let b = unique_in(PathBuf::from("h/a.html"), &mut taken);
        let c = unique_in(PathBuf::from("h/a.html"), &mut taken);
        assert_eq!(a, PathBuf::from("h/A.html"));
        if cfg!(any(windows, target_os = "macos")) {
            assert_eq!(b, PathBuf::from("h/a~2.html"), "one file to this disk");
            assert_eq!(c, PathBuf::from("h/a~3.html"));
        } else {
            assert_eq!(b, PathBuf::from("h/a.html"));
            assert_eq!(c, PathBuf::from("h/a~2.html"));
        }
    }

    #[test]
    fn relative_links_between_files() {
        let root = if cfg!(windows) {
            PathBuf::from(r"C:\sites")
        } else {
            PathBuf::from("/sites")
        };
        let from = root.join("h").join("docs");
        assert_eq!(
            relative(&from, &root.join("h").join("img").join("a.png")),
            "../img/a.png"
        );
        assert_eq!(relative(&from, &from.join("index.html")), "index.html");
        assert_eq!(
            relative(&from, &root.join("cdn").join("my pic (1).png")),
            "../../cdn/my%20pic%20%281%29.png"
        );
        assert_eq!(
            relative(&from, &from.join("menü #1.html")),
            "men%C3%BC%20%231.html"
        );
        #[cfg(windows)]
        assert_eq!(
            relative(&from, Path::new(r"D:\other\x.png")),
            "file:///D:/other/x.png"
        );
    }

    /// The copy of a small site under `root`: each address and its local path.
    fn site(root: &Path, files: &[(&str, FoundKind)]) -> (Copy, Vec<(Found, PathBuf)>) {
        let saved: Vec<(Found, PathBuf)> = files
            .iter()
            .map(|(u, k)| {
                let name = filter::last_segment(&Url::parse(u).unwrap());
                let f = found(u, *k, &name);
                let p = root.join(local_path(&f));
                (f, p)
            })
            .collect();
        let copy = Copy::of(saved.iter().map(|(f, p)| (f, p.clone())));
        (copy, saved)
    }

    #[test]
    fn a_pages_links_lead_to_the_copy_or_to_the_site() {
        use FoundKind::*;
        let root = std::env::temp_dir().join("ddm-offline-test");
        let (copy, saved) = site(
            &root,
            &[
                ("https://h.test/", Page),
                ("https://h.test/about", Page),
                ("https://h.test/docs/", Page),
                ("https://h.test/blog/post?id=1", Page),
                ("https://h.test/img/a.png", File),
                ("https://h.test/img/a@2x.png", File),
                ("https://h.test/img/lazy.png", File),
                ("https://h.test/img/bg.png", File),
                ("https://h.test/css/site.css", Style),
                ("https://cdn.test/lib.js", File),
            ],
        );
        let index = &saved[0].1;
        let page = br##"<!doctype html><html><head>
<link rel=stylesheet href="css/site.css" integrity="sha384-xyz" crossorigin="anonymous">
<script src="//cdn.test/lib.js" crossorigin></script>
<link rel=canonical href="/">
</head><body>
<a href="about">About</a> <a href='docs/#part'>Docs</a> <a href="/blog/post?id=1">Post</a>
<a href="blog/post?id=2">Not saved</a> <a href="https://elsewhere.test/x">Away</a>
<a href="mailto:me@h.test">Mail</a> <a href="#top">Top</a> <a href="javascript:void(0)">JS</a>
<img src="img/a.png" srcset="img/a.png 1x, img/a@2x.png 2x">
<img src="data:image/gif;base64,R0lG" data-src="/img/lazy.png" alt="">
<img data-src="img/lazy.png">
<div style="background: url(&quot;/img/bg.png&quot;)"></div>
<form action="search"><input name=q></form>
<style>.x { background: url(/img/bg.png) } .y { background: url(missing.png) }</style>
</body></html>"##;
        let url = Url::parse("https://h.test/").unwrap();
        let out = rewrite_html(page, &url, index, &copy).expect("changed");
        let out = String::from_utf8(out).unwrap();
        for want in [
            r#"<link rel=stylesheet href="css/site.css" integrity="" >"#,
            r#"<script src="../cdn.test/lib.js" ></script>"#,
            r#"<link rel=canonical href="index.html">"#,
            r#"<a href="about.html">About</a>"#,
            r##"<a href="docs/index.html#part">Docs</a>"##,
            r#"<a href="blog/post@id=1.html">Post</a>"#,
            r#"<a href="https://h.test/blog/post?id=2">Not saved</a>"#,
            r#"<a href="https://elsewhere.test/x">Away</a>"#,
            r#"<a href="mailto:me@h.test">Mail</a>"#,
            r##"<a href="#top">Top</a>"##,
            r#"<a href="javascript:void(0)">JS</a>"#,
            r#"<img src="img/a.png" srcset="img/a.png 1x, img/a@2x.png 2x">"#,
            r#"<img src="img/lazy.png" data-src="img/lazy.png" alt="">"#,
            r#"<img src="img/lazy.png" data-src="img/lazy.png">"#,
            r#"<div style="background: url('img/bg.png')"></div>"#,
            r#"<form action="https://h.test/search">"#,
            ".x { background: url('img/bg.png') } .y { background: url('https://h.test/missing.png') }",
        ] {
            assert!(out.contains(want), "{want}\nnot in\n{out}");
        }
        // Done again (a later run saved more): nothing more changes.
        assert_eq!(rewrite_html(out.as_bytes(), &url, index, &copy), None);
        // Nothing to change: None.
        assert_eq!(rewrite_html(b"<p>plain</p>", &url, index, &copy), None);
    }

    #[test]
    fn a_deeper_page_and_its_base_and_refresh() {
        use FoundKind::*;
        let root = std::env::temp_dir().join("ddm-offline-test2");
        let (copy, saved) = site(
            &root,
            &[
                ("https://h.test/docs/guide.html", Page),
                ("https://h.test/", Page),
                ("https://h.test/files/r.pdf", File),
                ("https://h.test/next.html", Page),
            ],
        );
        let guide = &saved[0].1;
        let page = br#"<base href="https://h.test/files/"><meta http-equiv=refresh content="5; url='/next.html'">
<a href="r.pdf">PDF</a> <a href="../">Home</a>"#;
        let url = Url::parse("https://h.test/docs/guide.html").unwrap();
        let out = String::from_utf8(rewrite_html(page, &url, guide, &copy).unwrap()).unwrap();
        assert!(out.contains(r#"<base href="">"#), "{out}");
        assert!(out.contains(r#"content="5; url='../next.html'""#), "{out}");
        assert!(out.contains(r#"<a href="../files/r.pdf">PDF</a>"#), "{out}");
        // `../` from the base (`/files/`) is the site's top.
        assert!(out.contains(r#"<a href="../index.html">Home</a>"#), "{out}");
        assert_eq!(rewrite_html(out.as_bytes(), &url, guide, &copy), None);
    }

    #[test]
    fn a_page_in_an_older_encoding_keeps_its_bytes() {
        use FoundKind::*;
        let root = std::env::temp_dir().join("ddm-offline-test3");
        let (copy, saved) = site(
            &root,
            &[("https://h.test/", Page), ("https://h.test/about", Page)],
        );
        // "Café" in ISO-8859-1 around a link.
        let page = b"<p>Caf\xe9</p><a href=\"about\" title=\"caf\xe9\">x</a><a href=\"caf\xe9.html\">y</a>";
        let url = Url::parse("https://h.test/").unwrap();
        let out = rewrite_html(page, &url, &saved[0].1, &copy).unwrap();
        let want = b"<p>Caf\xe9</p><a href=\"about.html\" title=\"caf\xe9\">x</a><a href=\"caf\xe9.html\">y</a>";
        assert_eq!(
            out, want,
            "only the link changed; the address written in Latin-1 is left"
        );
        // UTF-16 and compressed pages are left.
        assert_eq!(
            rewrite_html(&[0xff, 0xfe, b'<', 0], &url, &saved[0].1, &copy),
            None
        );
    }

    #[test]
    fn style_sheets_lead_to_the_copy() {
        use FoundKind::*;
        let root = std::env::temp_dir().join("ddm-offline-test4");
        let (copy, saved) = site(
            &root,
            &[
                ("https://h.test/css/site.css", Style),
                ("https://h.test/css/fonts.css", Style),
                ("https://h.test/img/bg.png", File),
                ("https://h.test/fonts/f.woff2", File),
            ],
        );
        let css = br#"@import "fonts.css"; @import url(/css/fonts.css);
/* url(gone.png) */ body { background: url(../img/bg.png) }
@font-face { src: url("/fonts/f.woff2") format("woff2"), url(other.woff) }"#;
        let url = Url::parse("https://h.test/css/site.css").unwrap();
        let out = String::from_utf8(rewrite_css(css, &url, &saved[0].1, &copy).unwrap()).unwrap();
        // Links that already lead to the copy's files (`fonts.css`, `../img/bg.png`) stay.
        assert_eq!(
            out,
            "@import \"fonts.css\"; @import url('fonts.css');\n/* url(gone.png) */ body { background: url(../img/bg.png) }\n@font-face { src: url('../fonts/f.woff2') format(\"woff2\"), url('https://h.test/css/other.woff') }"
        );
        assert_eq!(rewrite_css(out.as_bytes(), &url, &saved[0].1, &copy), None);
    }

    #[test]
    fn a_redirected_page_is_found_by_both_addresses() {
        let root = std::env::temp_dir().join("ddm-offline-test5");
        let mut moved = found("https://h.test/moved/", FoundKind::Page, "index.html");
        moved.also.push("https://h.test/old".into());
        let home = found("https://h.test/", FoundKind::Page, "index.html");
        let saved = [
            (moved.clone(), root.join(local_path(&moved))),
            (home.clone(), root.join(local_path(&home))),
        ];
        let copy = Copy::of(saved.iter().map(|(f, p)| (f, p.clone())));
        assert_eq!(copy.len(), 3);
        let url = Url::parse("https://h.test/").unwrap();
        let out = rewrite_html(br#"<a href="old">x</a>"#, &url, &saved[1].1, &copy).unwrap();
        assert_eq!(out, br#"<a href="moved/index.html">x</a>"#);
    }

    #[test]
    fn the_start_page_opens_the_copy() {
        let p = start_page("Docs <1>", "https://h.test/?a=1&b=2", "h.test/index.html");
        assert!(p.contains(r#"content="0; url=h.test/index.html""#));
        assert!(p.contains("Docs &lt;1&gt;"));
        assert!(p.contains("a=1&amp;b=2"));
        assert!(is_start_page(&p));
        assert!(!is_start_page("<html>mine</html>"));
        assert_eq!(text_of(Path::new("a/b.HTM")), Some(Text::Html));
        assert_eq!(text_of(Path::new("a/b.css")), Some(Text::Css));
        assert_eq!(text_of(Path::new("a/b.png")), None);
    }
}
