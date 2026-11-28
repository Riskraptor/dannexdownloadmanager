//! gallery-dl (#58): image galleries (Wikimedia Commons, Flickr, DeviantArt, Pixiv…) as yt-dlp
//! is for video pages. An optional helper, installed the same way: a release from its own page
//! (Codeberg), its `SHA256SUMS` checked against its author's OpenPGP key (pinned in
//! `keys/gallery-dl.asc`), the program against that list, kept in `tools/gallery-dl/<version>/`
//! with the previous version, never older; or the person's own copy (`chosen`). It runs in the
//! same job as yt-dlp does (`proc::spawn`), so Stop and DDM's end take it and all it started.
//!
//! A download is a folder, its files named by gallery-dl. Their bytes come down through DDM's
//! engine (#769), as a video's streams do ([`crate::streams`]): gallery-dl lists the files
//! without downloading them (`--no-download`, each one's address, the name it gives it in the
//! gallery's folder and the HTTP headers it wants, printed by `--print`, [`LISTED`]), and DDM's
//! engine downloads each into that very place, several at once, over the connections Options →
//! Connection gives a file, under the gallery's speed limit (one for all its files, a change
//! applying at once) and the one for all downloads, paused and resumed like a file. A file
//! already there is skipped, as gallery-dl skips it (it prints `# path` for it), so a gallery
//! started again fetches only its new files, whoever downloaded the others.
//!
//! What the engine can't download stays gallery-dl's (as a live stream stays yt-dlp's): a video
//! it hands to yt-dlp (`ytdl:`), a text it writes (`text:`), a file whose type it learns only
//! from the server, and a file the engine was refused (a site that wants gallery-dl's own
//! session). gallery-dl then runs as before DDM's engine did the bytes, over the whole gallery:
//! it skips every file already there and downloads those (one path a line, `# path` for one
//! already there). A gallery-dl that can't list the files so (one older than `--print`) downloads
//! them all itself, as before.

use std::collections::{HashSet, VecDeque};
use std::io::{BufRead, BufReader};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant, SystemTime};

use futures::future::BoxFuture;
use mdm_engine::download::{ConnView, State as DlState};
use mdm_engine::limiter::Limiter;
use mdm_engine::{Destination, Download, Request, ServiceFault, Unlocked, Unlocker};
use reqwest::header::HeaderMap;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::proc::{self, FromBrowser, ProcError, Spawned};
use crate::video::{Progress, State};
use crate::{Error, Installed, chosen};

/// gallery-dl's author's signing key (Mike Fährmann, fingerprint
/// 3E09F5908333DD83DBDCE7375680CA389D365A88): the same key on keys.openpgp.org, on his GitHub
/// account and on his Codeberg account. It signs each release's `SHA256SUMS`.
const KEY: &str = include_str!("../keys/gallery-dl.asc");
/// Where releases come from (Gitea's API, as GitHub's: `tag_name`, `assets`).
const RELEASES: &str = "https://codeberg.org/api/v1/repos/mikf/gallery-dl/releases/latest";
/// DDM as sites see it: gallery-dl's own name is refused by some (Wikimedia's servers answer
/// 429 to unnamed tools); this one says who asks and where to read about it.
pub const USER_AGENT: &str = concat!(
    "DannexDownloadManager/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/Riskraptor/dannexdownloadmanager) gallery-dl"
);

/// gallery-dl's program for this system in its releases. None on macOS (it publishes no build
/// for it: Homebrew's `gallery-dl`, chosen in Helpers, works there) and on Linux other than
/// x86-64 (the same, from the system's packages or pip).
pub fn asset() -> Option<&'static str> {
    if cfg!(windows) {
        Some("gallery-dl.exe")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("gallery-dl.bin")
    } else {
        None
    }
}

/// Whether `sums` carries a valid signature `sig` by gallery-dl's author's key.
pub fn signed_by_gallery_dl(sums: &[u8], sig: &[u8]) -> bool {
    crate::signed_by(KEY, sums, sig)
}

/// The oldest gallery-dl DDM can drive: `--directory` (the files right in the gallery's own
/// folder) came in 1.20.0. Debian 11's 1.17 lacks it; Ubuntu 22.04's 1.20.5 and every later
/// distribution's has it (#236).
pub const OLDEST: &str = "1.20.0";

/// What a gallery-dl release accepts of the options DDM passes, past those every release from
/// [`OLDEST`] has (`--extractor-info`, `--sleep`, `--range`, `--limit-rate`, `--cookies`,
/// `--proxy`, `--ignore-config`). Each release from 1.17 to 1.30 was asked in mdm-lab (#236):
/// argparse refuses an option it doesn't know, so one too many fails the whole download, as
/// `--no-input` did with Ubuntu 24.04's 1.26.9.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Accepts {
    /// `--no-input` (1.27.1): never a question on the console. Without it the same holds, as
    /// DDM gives it no input to read (`proc::spawn`): a question ends it instead of waiting.
    pub no_input: bool,
    /// `--user-agent` (1.24.0); before it, the same setting as `-o user-agent=…`.
    pub user_agent: bool,
    /// Worth asking for the files' list for DDM's engine (#769, [`listing_args`]): `--print`
    /// came in 1.27.7 (Ubuntu 24.04's 1.26.9 lacks it; before 1.30.0 it still printed the paths
    /// and skipped the files already there, which the listing reads too). The system's copy,
    /// whose version isn't asked, is asked too: one that refuses the options (argparse's exit
    /// code 2) downloads the files itself after all.
    pub listing: bool,
}

impl Accepts {
    /// For `version` as gallery-dl prints it (`1.26.9`, `1.31.0-dev`). A version that can't be
    /// read is taken as a recent one, as before #236. The system's copy (#571), whose version
    /// isn't asked (finding it starts no program), gets what every release DDM drives accepts:
    /// a distribution's may be one of the oldest.
    pub fn of(version: &str) -> Accepts {
        if version == crate::SYSTEM {
            return Accepts {
                no_input: false,
                user_agent: false,
                listing: true,
            };
        }
        let at_least = |v: &str| release(version).is_none_or(|have| Some(have) >= release(v));
        Accepts {
            no_input: at_least("1.27.1"),
            user_agent: at_least("1.24.0"),
            listing: at_least("1.27.7"),
        }
    }
}

/// `1.26.9` as (1, 26, 9): the leading digits of its first three parts (`0-dev` is 0).
fn release(version: &str) -> Option<(u32, u32, u32)> {
    let mut parts = version.trim().trim_start_matches('v').split('.').map(|p| {
        let digits: String = p.chars().take_while(char::is_ascii_digit).collect();
        digits.parse::<u32>().ok()
    });
    let major = parts.next()??;
    let minor = parts.next()??;
    Some((major, minor, parts.next().flatten().unwrap_or(0)))
}

/// Whether a gallery-dl of `version` is older than [`OLDEST`] (one that can't be read isn't).
pub fn too_old(version: &str) -> bool {
    release(version).is_some_and(|have| Some(have) < release(OLDEST))
}

/// The gallery-dl to use: the person's own (`chosen`), else DDM's install in `dir`; on Linux
/// and macOS, else the system's (#571, as yt-dlp's: a package manager's or Homebrew's, kept up
/// to date by it). A chosen one too old for DDM ([`too_old`], chosen before #236 checked) is
/// passed over; [`chosen_too_old`] names it, for the Helpers window to say so.
pub fn installed(dir: &Path) -> Option<Installed> {
    let own = chosen::chosen(dir, chosen::Helper::GalleryDl)
        .filter(|mine| !too_old(&mine.version))
        .or_else(|| own(dir));
    #[cfg(unix)]
    return own.or_else(|| crate::system::installed("gallery-dl"));
    #[cfg(not(unix))]
    own
}

/// The person's own gallery-dl when it is too old for DDM, so passed over.
pub fn chosen_too_old(dir: &Path) -> Option<Installed> {
    chosen::chosen(dir, chosen::Helper::GalleryDl).filter(|mine| too_old(&mine.version))
}

fn own(dir: &Path) -> Option<Installed> {
    let text = std::fs::read(dir.join("gallery-dl").join("current.json")).ok()?;
    let i: Installed = serde_json::from_slice(&text).ok()?;
    i.exe.is_file().then_some(i)
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// Install the latest gallery-dl into `dir` unless the installed one is as new, downloaded with
/// DDM's engine (`net`), counted into `progress`. The person's own copy is theirs to update. A
/// system copy doesn't count as installed here: this is the way to DDM's own instead (Helpers,
/// #571), which then wins ([`installed`]).
pub async fn install_latest(
    dir: &Path,
    net: &mdm_engine::Options,
    progress: Option<&crate::update::Progress>,
) -> Result<Installed, Error> {
    if let Some(mine) = chosen::chosen(dir, chosen::Helper::GalleryDl) {
        return Ok(mine);
    }
    let Some(asset) = asset() else {
        return Err(Error::Missing(
            "build for this system: install gallery-dl with the system's packages (Homebrew, pip) and choose it in Helpers".into(),
        ));
    };
    let client = crate::release_client(net, RELEASES)?;
    let body = crate::get_bytes(&client, RELEASES).await?;
    let release: Release =
        serde_json::from_slice(&body).map_err(|e| Error::BadRelease(e.to_string()))?;
    let version = release.tag_name.trim_start_matches('v').to_owned();
    if let Some(current) = own(dir)
        && !crate::newer(&version, &current.version)
    {
        return Ok(current); // never the same again, never older
    }
    let url = |name: &str| {
        release
            .assets
            .iter()
            .find(|a| a.name == name)
            .map(|a| a.browser_download_url.clone())
            .ok_or_else(|| Error::Missing(name.to_owned()))
    };
    let sums = crate::signed_list(
        &client,
        &url("SHA256SUMS")?,
        &url("SHA256SUMS.sig")?,
        signed_by_gallery_dl,
    )
    .await?;
    let expected = crate::sum_for(&sums, asset)
        .ok_or_else(|| Error::Missing(format!("checksum for {asset}")))?;
    let exe_url = url(asset)?;
    let root = dir.join("gallery-dl");
    std::fs::create_dir_all(&root)?;
    // Named by its checksum: one release's cut-off download resumes, never another's.
    let download = root.join(format!(
        "gallery-dl-{}.download",
        hex::encode(&expected[..8])
    ));
    crate::fetch_verified(&exe_url, &download, expected, net, progress).await?;
    let program = std::fs::read(&download)?;
    let _ = std::fs::remove_file(&download);
    if Sha256::digest(&program).as_slice() != expected {
        return Err(Error::BadChecksum);
    }
    crate::put(dir, "gallery-dl", &version, asset, &program, &exe_url)
}

/// Whether gallery-dl has a way to read `url` (a gallery, a user's page, a post…). Asked
/// offline: its extractors are matched against the address, nothing is fetched (`-E`, about a
/// second, most of it the program starting).
pub fn handles(tool: &Installed, url: &str) -> Result<bool, ProcError> {
    let cmd = proc::gallery_dl(tool, &["--extractor-info".into()], url)?;
    match proc::output(cmd, "gallery-dl", Duration::from_secs(30)) {
        Ok(_) => Ok(true),
        // Its words since 1.24, and before (#236: a distribution's may be that old).
        Err(ProcError::Failed { message, .. })
            if message.contains("Unsupported URL")
                || message.contains("No suitable extractor found") =>
        {
            Ok(false)
        }
        Err(e) => Err(e),
    }
}

/// A gallery's name, for its folder and its row: its site and the address's last part
/// (`commons.wikimedia.org - Category_Birds`), else its site; safe as a folder name.
pub fn name_of(page: &url::Url) -> String {
    let last = page
        .path_segments()
        .and_then(|mut s| s.rfind(|s| !s.is_empty()))
        .map(|s| {
            percent_encoding::percent_decode_str(s)
                .decode_utf8_lossy()
                .replace('_', " ")
        });
    let host = page
        .host_str()
        .unwrap_or("gallery")
        .trim_start_matches("www.");
    let name = match last {
        Some(l) => format!("{host} - {l}"),
        None => host.to_owned(),
    };
    mdm_engine::naming::sanitize(&name)
}

/// What a gallery download is asked for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ask {
    /// At most this many files, the first ones gallery-dl lists (on most sites the newest).
    pub limit: Option<u64>,
    /// Bytes/s, for the files gallery-dl downloads itself (#708: it takes a limit only as it
    /// starts). DDM's engine keeps to the gallery's own limit instead (#769: its options'
    /// `speed_limit`, then [`GalleryJob::set_speed_limit`]).
    pub rate: Option<u64>,
}

/// Paths kept of the files saved (to mark them as downloaded from the web at the end); a
/// gallery past this many still downloads, its later files unmarked.
const KEEP_PATHS: usize = 100_000;

/// The longest line of gallery-dl's read: a path is at most a few thousand bytes, a listed
/// file's line a few more (its address, its headers); a longer line is neither.
const LINE_MAX: u64 = 64 * 1024;

/// The files gallery-dl downloads itself named one by one in the log; the others are counted.
const NOTED: usize = 20;

/// How often the engine's downloads of a gallery's files are looked at.
const TICK: Duration = Duration::from_millis(150);

/// What `--print` writes for each file gallery-dl would save (#769; its `file` event, once the
/// path is made; with `--no-download` nothing is written): `MDMG`, then as JSON (`!j`),
/// separated by tabs (JSON writes a tab inside a value as `\t`): its address (`url-metadata`),
/// its path (`path-metadata`: the PathFormat's `path`, the line gallery-dl prints once it has
/// saved a file), its extension (empty when gallery-dl learns it from the server) and the HTTP
/// headers its extractor wants for it (`_http_headers`; `null`, or `None`, when there are none).
pub const LISTED: &str = "MDMG\t{gdl_url!j}\t{gdl_path.path!j}\t{extension!j}\t{_http_headers!j}";

/// gallery-dl's arguments to list a gallery's files in `folder` for DDM's engine (#769):
/// nothing downloaded and nothing slept for (`--sleep` waits before each file, downloaded or
/// not). Both spellings of the metadata options: newer releases call them `metadata-url` and
/// `metadata-path`, and a release ignores a name it doesn't know.
pub fn listing_args(folder: &Path, ask: &Ask) -> Vec<String> {
    let mut a: Vec<String> = vec![
        "--directory".into(),
        folder.to_string_lossy().into_owned(),
        "--no-download".into(),
        "--print".into(),
        format!("file:{LISTED}"),
    ];
    for (key, name) in [
        ("url-metadata", "gdl_url"),
        ("metadata-url", "gdl_url"),
        ("path-metadata", "gdl_path"),
        ("metadata-path", "gdl_path"),
    ] {
        a.extend(["-o".into(), format!("{key}={name}")]);
    }
    a.extend(range(ask));
    a
}

/// `--range 1-N`, for at most `ask.limit` files.
fn range(ask: &Ask) -> Vec<String> {
    ask.limit
        .filter(|n| *n > 0)
        .map(|n| vec!["--range".into(), format!("1-{n}")])
        .unwrap_or_default()
}

/// gallery-dl's arguments to download a gallery's files into `folder` itself (all of them, or
/// those DDM's engine left it: it skips every file already there), at most `rate` bytes/s.
fn download_args(folder: &Path, ask: &Ask, rate: Option<u64>) -> Vec<String> {
    let mut a: Vec<String> = vec![
        // The files right in `folder` (the gallery's own), not in subfolders of the site's.
        "--directory".into(),
        folder.to_string_lossy().into_owned(),
        // One file a second at most: gentle with the sites (Wikimedia answers 429 to
        // faster tools) and with the person's line.
        "--sleep".into(),
        "1".into(),
    ];
    a.extend(range(ask));
    if let Some(r) = rate.filter(|r| *r > 0) {
        a.extend(["--limit-rate".into(), r.to_string()]);
    }
    a
}

/// A file as gallery-dl lists it (#769, [`LISTED`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listed {
    /// Its address as gallery-dl gives it: a web address, or one of its own (`ytdl:…`,
    /// `text:…`).
    pub url: String,
    /// Where gallery-dl would save it.
    pub path: PathBuf,
    /// Its extension; empty when gallery-dl learns it from the server.
    pub extension: String,
    /// The headers gallery-dl's extractor sends for it (`_http_headers`).
    pub headers: Vec<(String, String)>,
}

/// What one line of gallery-dl's says: a file listed (#769), one saved (or, while it lists,
/// one it would save), or one already there.
#[derive(Debug, PartialEq)]
enum Line {
    Listed(Listed),
    Saved(PathBuf),
    There(PathBuf),
}

fn parse_line(line: &str) -> Option<Line> {
    let line = line.trim_end_matches(['\r', '\n']);
    if let Some(rest) = line.strip_prefix("MDMG\t") {
        return listed(rest).map(Line::Listed);
    }
    if let Some(p) = line.strip_prefix("# ") {
        return Some(Line::There(PathBuf::from(p)));
    }
    let p = Path::new(line);
    p.is_absolute().then(|| Line::Saved(p.to_owned()))
}

/// A text written as JSON; None for `null`, for gallery-dl's `None` (a field it doesn't have)
/// and for an empty one.
fn json_text(s: &str) -> Option<String> {
    match serde_json::from_str::<Value>(s.trim()) {
        Ok(Value::String(s)) if !s.is_empty() => Some(s),
        _ => None,
    }
}

/// A listed file's fields ([`LISTED`], after `MDMG` and its tab). None without an address or a
/// path (a release that doesn't know `url-metadata` or `path-metadata`): its path line follows
/// alone, and the file is gallery-dl's to download.
fn listed(rest: &str) -> Option<Listed> {
    let mut fields = rest.splitn(4, '\t');
    let url = json_text(fields.next()?)?;
    let path = PathBuf::from(json_text(fields.next()?)?);
    let extension = fields.next().and_then(json_text).unwrap_or_default();
    let headers = match fields
        .next()
        .map(|h| serde_json::from_str::<Value>(h.trim()))
    {
        Some(Ok(Value::Object(h))) => h
            .into_iter()
            .filter_map(|(k, v)| Some((k, v.as_str()?.to_owned())))
            .collect(),
        _ => Vec::new(),
    };
    Some(Listed {
        url,
        path,
        extension,
        headers,
    })
}

/// Read gallery-dl's lines as it prints them, each one that says something given to `each`.
/// Lines are read whole but kept short ([`LINE_MAX`]): a longer one is skipped to its end.
fn lines(stdout: impl std::io::Read, mut each: impl FnMut(Line)) {
    let mut reader = BufReader::new(stdout);
    let mut buf = Vec::new();
    let mut skipping = false;
    loop {
        buf.clear();
        match std::io::Read::take(&mut reader, LINE_MAX).read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        let whole = buf.ends_with(b"\n");
        if skipping || (!whole && buf.len() as u64 >= LINE_MAX) {
            skipping = !whole;
            continue;
        }
        if let Some(line) = parse_line(&String::from_utf8_lossy(&buf)) {
            each(line);
        }
    }
}

/// Why gallery-dl downloads a listed file itself rather than DDM's engine (#769).
#[derive(Clone, Debug, PartialEq, Eq)]
enum Why {
    /// A video it hands to yt-dlp (`ytdl:`).
    Delegated,
    /// A text it writes itself (`text:`).
    Text,
    /// An address of another kind than the web's.
    Scheme(String),
    /// Its type, so its extension, comes from the server.
    NoExtension,
    /// Its place isn't in the gallery's folder.
    Outside,
    /// gallery-dl gave no address for it (its line came without one).
    Unlisted,
    /// DDM's engine couldn't download it (its words).
    Refused(String),
}

impl Why {
    /// For the download's log (English, as gallery-dl's own lines).
    fn english(&self) -> String {
        match self {
            Why::Delegated => "a video it hands to yt-dlp".into(),
            Why::Text => "a text it writes".into(),
            Why::Scheme(s) => format!("an address of its own ({s}:)"),
            Why::NoExtension => "its type comes from the server".into(),
            Why::Outside => "its place is outside the gallery's folder".into(),
            Why::Unlisted => "gallery-dl gave no address for it".into(),
            Why::Refused(e) => format!("DDM's downloader couldn't download it: {e}"),
        }
    }
}

/// A listed file as DDM's engine downloads it (#769).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineFile {
    pub url: Url,
    /// Its place in the gallery's folder: gallery-dl's own.
    pub dest: PathBuf,
    /// gallery-dl's headers for it, and the gallery's site as the page it comes from
    /// (`Referer`, as gallery-dl's own requests and a browser's say) unless they name another.
    pub headers: Vec<(String, String)>,
}

/// Listed file `l` as DDM's engine downloads it into `folder` for the gallery at `page`, or why
/// gallery-dl downloads it itself.
fn classify(l: &Listed, folder: &Path, page: &Url) -> Result<EngineFile, Why> {
    let dest = within(folder, &l.path).ok_or(Why::Outside)?;
    let scheme = l
        .url
        .split_once(':')
        .map_or("", |(s, _)| s)
        .to_ascii_lowercase();
    match scheme.as_str() {
        "http" | "https" => {}
        "ytdl" => return Err(Why::Delegated),
        "text" => return Err(Why::Text),
        _ => return Err(Why::Scheme(scheme)),
    }
    let url = Url::parse(&l.url).map_err(|_| Why::Unlisted)?;
    if l.extension.trim().is_empty() {
        return Err(Why::NoExtension);
    }
    let mut headers = l.headers.clone();
    let origin = page.origin();
    if origin.is_tuple()
        && !headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("referer"))
    {
        headers.push((
            "Referer".into(),
            format!("{}/", origin.ascii_serialization()),
        ));
    }
    Ok(EngineFile { url, dest, headers })
}

/// A path as the file system tells paths apart: letter case and the slash's direction aside on
/// Windows.
fn key(p: &Path) -> String {
    let s = p.to_string_lossy();
    if cfg!(windows) {
        s.replace('/', "\\").to_lowercase()
    } else {
        s.into_owned()
    }
}

/// gallery-dl's `path` for a file as a place in `folder`: `folder` and the part of the path past
/// it (letter case aside on Windows), else the file's name in `folder`; never a place that
/// climbs out of it (`..`) or names a root.
fn within(folder: &Path, path: &Path) -> Option<PathBuf> {
    let text = path.to_string_lossy();
    let path = Path::new(text.strip_prefix(r"\\?\").unwrap_or(&text));
    let n = folder.components().count();
    let same =
        |a: Component, b: Component| key(Path::new(a.as_os_str())) == key(Path::new(b.as_os_str()));
    let rel: PathBuf = match path.strip_prefix(folder) {
        Ok(rel) => rel.to_owned(),
        Err(_)
            if path.components().count() > n
                && path
                    .components()
                    .zip(folder.components())
                    .all(|(a, b)| same(a, b)) =>
        {
            path.components().skip(n).collect()
        }
        Err(_) => PathBuf::from(path.file_name()?),
    };
    let plain = rel.components().next().is_some()
        && rel
            .components()
            .all(|c| matches!(c, Component::Normal(n) if kept_as_named(n)));
    plain.then(|| folder.join(rel))
}

/// Whether the system keeps a file or folder of this name as written. On Windows, not one with
/// a `:` (another stream of a file), ending in a dot or a space (dropped from the name), or a
/// device's (`CON`, `NUL`, `COM1`, … with any extension): gallery-dl's own rules for names keep
/// them out, but a name it was told to keep as the site gives it goes to gallery-dl instead.
fn kept_as_named(name: &std::ffi::OsStr) -> bool {
    if !cfg!(windows) {
        return true;
    }
    let name = name.to_string_lossy();
    if name.contains(':') || name.ends_with(['.', ' ']) {
        return false;
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end()
        .to_ascii_uppercase();
    let numbered = stem
        .strip_prefix("COM")
        .or_else(|| stem.strip_prefix("LPT"))
        .is_some_and(|n| {
            (n.len() == 1 && n.as_bytes()[0].is_ascii_digit())
                || matches!(n, "\u{b9}" | "\u{b2}" | "\u{b3}")
        });
    let device = numbered
        || matches!(
            stem.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
        );
    !device
}

/// A file's name, for the log.
fn file_name(p: &Path) -> String {
    p.file_name().map_or_else(
        || p.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// A listed file's address, from gallery-dl (#769): the engine's [`Unlocker`] for it, so the
/// address and headers gallery-dl gave this run are the ones asked, a download continued after
/// a stop too (its address then may have expired). Asked again (the address stopped working),
/// it has no other: the file goes to gallery-dl.
#[derive(Debug)]
struct GalleryLink {
    name: String,
    first: Mutex<Option<Unlocked>>,
}

impl GalleryLink {
    fn new(f: &EngineFile) -> GalleryLink {
        GalleryLink {
            name: file_name(&f.dest),
            first: Mutex::new(Some(Unlocked {
                url: f.url.clone(),
                file_name: None,
                size: None,
                max_connections: None,
                headers: crate::streams::header_map(&f.headers),
            })),
        }
    }
}

impl Unlocker for GalleryLink {
    fn name(&self) -> &str {
        "gallery-dl"
    }

    fn unlock<'a>(
        &'a self,
        _url: &'a Url,
        _note: &'a (dyn Fn(String, Option<f32>) + Send + Sync),
        _cancel: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Option<Unlocked>, ServiceFault>> {
        Box::pin(async move {
            match self.first.lock().unwrap().take() {
                Some(u) => Ok(Some(u)),
                None => Err(ServiceFault::Other(format!(
                    "gallery-dl's address for {} stopped working",
                    self.name
                ))),
            }
        })
    }

    fn noted(&self, u: &Unlocked) -> String {
        format!(
            "gallery-dl's address for {}: from {}",
            self.name,
            u.url.host_str().unwrap_or("?")
        )
    }
}

/// How far a gallery is, by its files (#769): the progress window's line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Files {
    /// Saved this run, or found already there.
    pub done: u64,
    /// The files gallery-dl listed so far; None when it downloads them all itself without
    /// listing them (started without DDM's engine, or a gallery-dl that can't list them).
    pub listed: Option<u64>,
    /// gallery-dl is still listing them.
    pub listing: bool,
    /// Of those listed, the files gallery-dl downloads itself (a video it hands to yt-dlp, a
    /// text, a file DDM's engine was refused).
    pub by_gallery_dl: u64,
}

#[derive(Default)]
struct Shared {
    progress: Progress,
    files: Files,
    /// gallery-dl downloads files itself now (all of them, or those the engine left it).
    helper: bool,
    /// Files saved this run, files already there.
    saved: u64,
    there: u64,
    /// Of those saved, the ones gallery-dl saved itself.
    by_helper: u64,
    /// Bytes of the files saved this run.
    bytes: u64,
    /// Every file counted, saved or there, as the file system tells them apart (`key`).
    counted: HashSet<String>,
    /// The files saved this run (at most `KEEP_PATHS`).
    files_saved: Vec<PathBuf>,
    /// Every file of this gallery seen this run: saved or already there (at most `KEEP_PATHS`).
    known: Vec<PathBuf>,
    /// The listed files left for gallery-dl to download (#769).
    left: Vec<PathBuf>,
}

impl Shared {
    /// File `p` saved this run (`size` bytes; by gallery-dl itself: `helper`), counted once.
    fn saved(&mut self, p: PathBuf, size: u64, helper: bool) {
        if !self.counted.insert(key(&p)) {
            return;
        }
        self.saved += 1;
        self.by_helper += u64::from(helper);
        self.bytes += size;
        self.files.done += 1;
        if self.files_saved.len() < KEEP_PATHS {
            self.files_saved.push(p.clone());
        }
        if self.known.len() < KEEP_PATHS {
            self.known.push(p);
        }
    }

    /// File `p` found already there, counted once.
    fn there(&mut self, p: PathBuf) {
        if !self.counted.insert(key(&p)) {
            return;
        }
        self.there += 1;
        self.files.done += 1;
        if self.known.len() < KEEP_PATHS {
            self.known.push(p);
        }
    }

    fn note(&mut self, text: String) {
        self.progress.events.push((SystemTime::now(), text));
    }

    /// The listing's end in the log (#769): how many files, how many there already, and how
    /// many more gallery-dl downloads itself than were named. The files left for gallery-dl.
    fn listed_note(&mut self) -> usize {
        let (n, there, left) = (self.files.listed.unwrap_or(0), self.there, self.left.len());
        self.note(format!(
            "gallery-dl listed {n} files ({there} already there)"
        ));
        if left > NOTED {
            self.note(format!(
                "{} more files gallery-dl downloads itself",
                left - NOTED
            ));
        }
        left
    }
}

impl Default for Progress {
    fn default() -> Self {
        Progress {
            state: State::Running,
            downloaded: 0,
            total: None,
            exact: None,
            speed: 0.0,
            events: Vec::new(),
            side_files: Vec::new(),
            sub_problems: Vec::new(),
            engine: None,
            helper: None,
            place: None,
        }
    }
}

/// A gallery downloading into a folder.
pub struct GalleryJob {
    shared: Arc<Mutex<Shared>>,
    /// gallery-dl while it runs (listing, or downloading itself).
    child: Arc<Mutex<Option<Spawned>>>,
    stop: Arc<AtomicBool>,
    /// The gallery's own speed limit, one for all its files (#769).
    limit: Arc<Limiter>,
}

impl GalleryJob {
    /// Download `url`'s files into `folder`, the gallery's own: listed by gallery-dl and
    /// downloaded by DDM's `engine` (#769; its options' `speed_limit` is the whole gallery's),
    /// those it can't download by gallery-dl, at most `ask.rate` bytes/s; without an engine,
    /// every file by gallery-dl, as before. The folder is made, and everything else that looks
    /// at the disk done, on the job's own thread (#743): the job runs from now.
    pub fn start(
        tool: &Installed,
        url: &str,
        folder: &Path,
        ask: &Ask,
        browser: &FromBrowser,
        engine: Option<crate::streams::Engine>,
    ) -> Result<GalleryJob, ProcError> {
        proc::safe(url)?;
        // The browser's cookies and User-Agent (gallery-dl has no Referer option: left out),
        // their file written on the job's thread (#743).
        let prepared = browser.prepare(url)?;
        let agent = browser
            .user_agent
            .as_deref()
            .map(str::trim)
            .filter(|a| !a.is_empty())
            .unwrap_or(USER_AGENT)
            .to_owned();
        let rate = match &engine {
            Some(e) => e.opts.speed_limit,
            None => ask.rate,
        };
        let job = GalleryJob {
            shared: Default::default(),
            child: Default::default(),
            stop: Default::default(),
            limit: Arc::new(Limiter::new(rate)),
        };
        let mut run = Run {
            tool: tool.clone(),
            url: url.to_owned(),
            folder: folder.to_owned(),
            ask: ask.clone(),
            theirs: Vec::new(),
            agent,
            engine,
            shared: job.shared.clone(),
            child: job.child.clone(),
            stop: job.stop.clone(),
            limit: job.limit.clone(),
        };
        let (shared, child) = (job.shared.clone(), job.child.clone());
        std::thread::spawn(move || {
            // The cookie file, written here, lives as long as the run.
            let _cookies = match prepared.write() {
                Ok((theirs, cookies)) => {
                    run.theirs = theirs;
                    cookies
                }
                Err(e) => {
                    let mut sh = shared.lock().unwrap_or_else(|p| p.into_inner());
                    sh.progress.speed = 0.0;
                    sh.progress.state = State::Failed(e.to_string());
                    return;
                }
            };
            // A panic must not leave the gallery "running" forever, nor gallery-dl behind (#79).
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || run.run())).is_err() {
                if let Some(mut s) = child.lock().unwrap_or_else(|p| p.into_inner()).take() {
                    s.kill();
                }
                let mut sh = shared.lock().unwrap_or_else(|p| p.into_inner());
                if sh.progress.state == State::Running {
                    sh.progress.speed = 0.0;
                    sh.progress.state =
                        State::Failed("the gallery download stopped on an internal error".into());
                }
            }
        });
        Ok(job)
    }

    /// DDM's engine's part alone (#769), over files listed already as gallery-dl's listing gives
    /// them, with no gallery-dl to run: for the tests of what shows a gallery (the app's rows
    /// and progress window), which have none. A file left for gallery-dl stays undownloaded.
    #[doc(hidden)]
    pub fn start_listed(
        page: &str,
        folder: &Path,
        listed: Vec<Listed>,
        engine: crate::streams::Engine,
    ) -> GalleryJob {
        let job = GalleryJob {
            shared: Default::default(),
            child: Default::default(),
            stop: Default::default(),
            limit: Arc::new(Limiter::new(engine.opts.speed_limit)),
        };
        let run = Run {
            tool: Installed {
                version: String::new(),
                exe: PathBuf::new(),
            },
            url: page.to_owned(),
            folder: folder.to_owned(),
            ask: Ask::default(),
            theirs: Vec::new(),
            agent: USER_AGENT.to_owned(),
            engine: None,
            shared: job.shared.clone(),
            child: job.child.clone(),
            stop: job.stop.clone(),
            limit: job.limit.clone(),
        };
        let (tx, rx) = mpsc::channel();
        for l in listed {
            let _ = tx.send(Line::Listed(l));
        }
        drop(tx);
        std::thread::spawn(move || {
            if let Err(e) = std::fs::create_dir_all(&run.folder) {
                return run.end_failed(format!("Cannot make the gallery's folder: {e}"));
            }
            let Ok(page) = Url::parse(&run.url) else {
                return run.end_failed("not a web address".into());
            };
            match run.fetch_listed(&engine, &page, rx) {
                None => run.end_stopped(),
                Some(_) => {
                    run.shared.lock().unwrap().listed_note();
                    run.end(true, None);
                }
            }
        });
        job
    }

    /// Its progress. Stopped only once the engine's downloads have saved where they stopped: a
    /// start again before then would find their files still held.
    pub fn progress(&self) -> Progress {
        self.shared.lock().unwrap().progress.clone()
    }

    /// How far it is by its files (#769).
    pub fn files(&self) -> Files {
        self.shared.lock().unwrap().files.clone()
    }

    /// The files saved so far this run (asked once, at the end: not every frame).
    pub fn saved(&self) -> Vec<PathBuf> {
        self.shared.lock().unwrap().files_saved.clone()
    }

    /// Every file of this gallery seen this run: saved, or already there.
    pub fn known_files(&self) -> Vec<PathBuf> {
        self.shared.lock().unwrap().known.clone()
    }

    /// Stop: each file DDM's engine was downloading keeps where it got to (its part file and
    /// resume state: a start again continues it), gallery-dl ends with all it started; what was
    /// saved stays.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Release);
        if let Some(mut s) = self.child.lock().unwrap().take() {
            s.kill();
        }
    }

    /// A new speed limit for the whole gallery (bytes/s): the engine's downloads take it at once
    /// (#769). gallery-dl downloading by itself took its limit as it started.
    pub fn set_speed_limit(&self, rate: Option<u64>) {
        self.limit.set(rate);
    }

    pub fn speed_limit(&self) -> Option<u64> {
        self.limit.get()
    }

    /// gallery-dl downloads files itself now (all of them, or those DDM's engine left it): a new
    /// speed limit then takes starting it again (#708).
    pub fn helper_running(&self) -> bool {
        self.shared.lock().unwrap().helper
    }
}

impl Drop for GalleryJob {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The browser's context (`FromBrowser::args`, made for yt-dlp) as gallery-dl takes it: its
/// cookies, and its User-Agent or DDM's (`--user-agent`, or `-o user-agent=…` before 1.24); no
/// Referer, which gallery-dl has no option for.
fn browser_args(theirs: Vec<String>, accepts: Accepts) -> Vec<String> {
    let mut out = Vec::new();
    let mut agent = None;
    let mut theirs = theirs.into_iter();
    while let Some(a) = theirs.next() {
        match a.as_str() {
            "--referer" => {
                theirs.next();
            }
            "--user-agent" => agent = theirs.next(),
            "--cookies" => {
                out.push(a);
                out.extend(theirs.next());
            }
            _ => out.push(a),
        }
    }
    let agent = agent.unwrap_or_else(|| USER_AGENT.to_owned());
    if accepts.user_agent {
        out.extend(["--user-agent".into(), agent]);
    } else {
        out.extend(["-o".into(), format!("user-agent={agent}")]);
    }
    out
}

/// A gallery download's own thread: what it was asked for, and what it shares with its
/// [`GalleryJob`].
struct Run {
    tool: Installed,
    url: String,
    folder: PathBuf,
    ask: Ask,
    /// The browser's arguments for gallery-dl (`FromBrowser::args`).
    theirs: Vec<String>,
    /// The User-Agent gallery-dl asks with (the browser's, else DDM's): the engine's too.
    agent: String,
    engine: Option<crate::streams::Engine>,
    shared: Arc<Mutex<Shared>>,
    child: Arc<Mutex<Option<Spawned>>>,
    stop: Arc<AtomicBool>,
    limit: Arc<Limiter>,
}

/// How gallery-dl's listing, and the engine's downloads of what it listed, ended (#769).
enum Listing {
    /// Listed: the files left for gallery-dl (`Shared::left`), and what gallery-dl said when it
    /// couldn't list every file.
    Done {
        left: usize,
        failure: Option<String>,
    },
    /// This gallery-dl can't list the files for the engine (why, for the log; empty when it
    /// wasn't asked): it downloads them all itself.
    Unable(String),
    Stopped,
    Failed(String),
}

/// What the engine's part saw of gallery-dl's listing (#769, `Run::fetch_listed`).
#[derive(Default)]
struct Seen {
    /// A file came with its address (gallery-dl can list them for the engine).
    listed_any: bool,
    /// Files it printed as it would save them, without their listing.
    unlisted: usize,
}

/// How a run of gallery-dl downloading by itself ended.
enum Pass {
    /// It ended: whether it said all went well, and what it said.
    Ended {
        ok: bool,
        failure: String,
    },
    Stopped,
    Failed(String),
}

impl Run {
    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Acquire)
    }

    fn note(&self, text: String) {
        self.shared.lock().unwrap().note(text);
    }

    fn run(self) {
        if let Err(e) = std::fs::create_dir_all(&self.folder) {
            return self.end_failed(format!("Cannot make the gallery's folder: {e}"));
        }
        let listing = match &self.engine {
            Some(engine) if Accepts::of(&self.tool.version).listing => self.list_and_fetch(engine),
            _ => Listing::Unable(String::new()),
        };
        let (listed, failure) = match listing {
            Listing::Stopped => return self.end_stopped(),
            Listing::Failed(e) => return self.end_failed(e),
            Listing::Done { left: 0, failure } => return self.end(true, failure),
            Listing::Done { failure, .. } => (true, failure),
            Listing::Unable(why) => {
                if !why.is_empty() {
                    self.note(why);
                }
                (false, None)
            }
        };
        // The rate it was started with (#708: the lower of the gallery's limit and its share of
        // the limit for all downloads), unless the gallery's own limit is lower now.
        let rate = match (self.ask.rate, self.limit.get()) {
            (Some(a), Some(l)) => Some(a.min(l)),
            (a, l) => a.or(l),
        };
        match self.gallery_dl_downloads(rate, listed) {
            Pass::Stopped => self.end_stopped(),
            // Not started: what DDM's engine saved stays saved, and is said so.
            Pass::Failed(e) => self.end(listed, Some(e)),
            Pass::Ended { ok, failure: said } => {
                self.end(listed, failure.or_else(|| (!ok).then_some(said)))
            }
        }
    }

    /// gallery-dl just started: held where Stop finds it, its output for the caller. None when
    /// Stop came meanwhile (it is ended).
    fn hold(&self, mut spawned: Spawned) -> Option<std::process::ChildStdout> {
        let stdout = spawned.child.stdout.take().expect("piped");
        let mut slot = self.child.lock().unwrap();
        if self.stopped() {
            spawned.kill();
            return None;
        }
        *slot = Some(spawned);
        Some(stdout)
    }

    /// gallery-dl's end: its exit code and what it said. None when Stop ended it. Polled with
    /// the lock free in between, so Stop never waits on it.
    fn exit(&self) -> Option<(Option<i32>, String)> {
        loop {
            let mut slot = self.child.lock().unwrap();
            let s = slot.as_mut()?;
            match s.child.try_wait() {
                Ok(Some(st)) => {
                    let s = slot.take()?;
                    drop(slot);
                    return Some((st.code(), s.failure().to_string()));
                }
                Ok(None) => {}
                Err(e) => {
                    slot.take();
                    return Some((None, e.to_string()));
                }
            }
            drop(slot);
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// Listed file `path` is gallery-dl's to download (#769), and why: said in the log for the
    /// first few.
    fn leave(&self, path: PathBuf, why: Why) {
        let mut sh = self.shared.lock().unwrap();
        sh.files.by_gallery_dl += 1;
        if sh.left.len() < NOTED {
            sh.note(format!(
                "{}: gallery-dl downloads it itself ({})",
                file_name(&path),
                why.english()
            ));
        }
        sh.left.push(path);
    }

    /// gallery-dl lists the gallery's files, and DDM's engine downloads them as they come,
    /// several at once (#769); a file the engine can't download is left to gallery-dl.
    fn list_and_fetch(&self, engine: &crate::streams::Engine) -> Listing {
        let Ok(page) = Url::parse(&self.url) else {
            return Listing::Failed("not a web address".into());
        };
        let accepts = Accepts::of(&self.tool.version);
        let mut ours = listing_args(&self.folder, &self.ask);
        ours.extend(browser_args(self.theirs.clone(), accepts));
        let spawned = match proc::gallery_dl(&self.tool, &ours, &self.url)
            .and_then(|cmd| proc::spawn(cmd, "gallery-dl"))
        {
            Ok(s) => s,
            Err(e) => return Listing::Failed(e.to_string()),
        };
        let Some(stdout) = self.hold(spawned) else {
            return Listing::Stopped;
        };
        let (tx, rx) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            lines(stdout, |l| {
                let _ = tx.send(l);
            })
        });
        let seen = self.fetch_listed(engine, &page, rx);
        let _ = reader.join();
        let ended = self.exit();
        let (Some(seen), Some((code, said)), false) = (seen, ended, self.stopped()) else {
            return Listing::Stopped;
        };
        let mut sh = self.shared.lock().unwrap();
        let n = sh.files.listed.unwrap_or(0);
        // argparse's exit code: an option this release doesn't know (`--print` before 1.27.7).
        if !seen.listed_any && code == Some(2) {
            sh.files = Files::default();
            sh.left.clear();
            return Listing::Unable(format!(
                "This gallery-dl can't list the files for DDM's downloader ({said}): it downloads them itself"
            ));
        }
        // It printed the files it would save, but none's address: it downloads them itself.
        if !seen.listed_any && seen.unlisted > 0 {
            sh.files = Files::default();
            sh.left.clear();
            return Listing::Unable(
                "gallery-dl listed no addresses for DDM's downloader: it downloads the files itself"
                    .into(),
            );
        }
        if n == 0 && code != Some(0) {
            return Listing::Failed(said);
        }
        let left = sh.listed_note();
        let failure = (code != Some(0)).then(|| {
            sh.note(format!("gallery-dl couldn't list every file: {said}"));
            said
        });
        Listing::Done { left, failure }
    }

    /// DDM's engine downloads the files gallery-dl lists (`rx`, its lines as it prints them)
    /// into the gallery's folder, as they come, several at once, until they are all in (#769):
    /// a file already there is counted, one the engine can't download left to gallery-dl. What
    /// it saw of the listing; None when Stop came (each file under way keeps where it got to).
    fn fetch_listed(
        &self,
        engine: &crate::streams::Engine,
        page: &Url,
        rx: mpsc::Receiver<Line>,
    ) -> Option<Seen> {
        self.shared.lock().unwrap().files = Files {
            listed: Some(0),
            listing: true,
            ..Files::default()
        };
        let mut fetch = Fetch::new(engine, &self.limit, &self.agent);
        let mut queue: VecDeque<EngineFile> = VecDeque::new();
        // The places listed (`key`), and the path of the file listed last: the line gallery-dl
        // prints for it once "saved" follows its listing.
        let mut places: HashSet<String> = HashSet::new();
        let mut last: Option<PathBuf> = None;
        let mut seen = Seen::default();
        let mut reading = true;
        loop {
            while reading {
                let line = match rx.try_recv() {
                    Ok(line) => line,
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        reading = false;
                        self.shared.lock().unwrap().files.listing = false;
                        break;
                    }
                };
                match line {
                    Line::Listed(l) => {
                        seen.listed_any = true;
                        last = Some(l.path.clone());
                        let file = classify(&l, &self.folder, page);
                        let place = file
                            .as_ref()
                            .map_or_else(|_| key(&l.path), |f| key(&f.dest));
                        // Listed twice: gallery-dl would find the first there by then.
                        if !places.insert(place) {
                            continue;
                        }
                        if let Some(n) = self.shared.lock().unwrap().files.listed.as_mut() {
                            *n += 1;
                        }
                        match file {
                            Ok(f) => queue.push_back(f),
                            Err(why) => self.leave(l.path, why),
                        }
                    }
                    Line::There(p) => {
                        if places.insert(key(&p)) {
                            let mut sh = self.shared.lock().unwrap();
                            if let Some(n) = sh.files.listed.as_mut() {
                                *n += 1;
                            }
                            sh.there(p);
                        }
                    }
                    // The file listed last, as gallery-dl says it "saved" it; else one it
                    // printed without its listing (`--print` failed for it).
                    Line::Saved(p) if last.as_ref() == Some(&p) => last = None,
                    Line::Saved(p) => {
                        if places.insert(key(&p)) {
                            seen.unlisted += 1;
                            if let Some(n) = self.shared.lock().unwrap().files.listed.as_mut() {
                                *n += 1;
                            }
                            self.leave(p, Why::Unlisted);
                        }
                    }
                }
            }
            if self.stopped() {
                fetch.stop();
                return None;
            }
            fetch.start_more(&mut queue, self);
            fetch.poll(self);
            fetch.report(&self.shared);
            if !reading && queue.is_empty() && fetch.running.is_empty() {
                break;
            }
            std::thread::sleep(TICK);
        }
        let mut sh = self.shared.lock().unwrap();
        sh.progress.speed = 0.0;
        sh.progress.engine = None;
        Some(seen)
    }

    /// gallery-dl downloads the gallery's files itself, at most `rate` bytes/s: all of them, or
    /// those DDM's engine left it (`listed`), skipping every file already there. After a
    /// listing, only its lines naming a file left to it are counted: the others are DDM's,
    /// counted already, and a line naming one in a way DDM doesn't know (a release writing the
    /// console's code page) is left to the look on the disk at the end (`end`).
    fn gallery_dl_downloads(&self, rate: Option<u64>, listed: bool) -> Pass {
        let mut ours = download_args(&self.folder, &self.ask, rate);
        ours.extend(browser_args(
            self.theirs.clone(),
            Accepts::of(&self.tool.version),
        ));
        let spawned = match proc::gallery_dl(&self.tool, &ours, &self.url)
            .and_then(|cmd| proc::spawn(cmd, "gallery-dl"))
        {
            Ok(s) => s,
            Err(e) => return Pass::Failed(e.to_string()),
        };
        let base = {
            let mut sh = self.shared.lock().unwrap();
            sh.helper = true;
            sh.progress.engine = None;
            sh.bytes
        };
        let Some(stdout) = self.hold(spawned) else {
            return Pass::Stopped;
        };
        let started = Instant::now();
        let only: Option<HashSet<String>> = listed.then(|| {
            self.shared
                .lock()
                .unwrap()
                .left
                .iter()
                .map(|p| key(p))
                .collect()
        });
        lines(stdout, |line| {
            if let (Some(only), Line::Saved(p) | Line::There(p)) = (&only, &line)
                && !only.contains(&key(p))
            {
                return;
            }
            // The disk asked before the lock is taken (the window reads the progress under it).
            let size = match &line {
                Line::Saved(p) => std::fs::metadata(p).map_or(0, |m| m.len()),
                _ => 0,
            };
            let mut sh = self.shared.lock().unwrap();
            match line {
                Line::Saved(p) => sh.saved(p, size, true),
                Line::There(p) => sh.there(p),
                Line::Listed(_) => {}
            }
            sh.progress.downloaded = sh.bytes;
            let secs = started.elapsed().as_secs_f64().max(0.5);
            sh.progress.speed = (sh.bytes - base) as f64 / secs;
        });
        let ended = self.exit();
        if self.stopped() {
            return Pass::Stopped;
        }
        match ended {
            Some((code, failure)) => Pass::Ended {
                ok: code == Some(0),
                failure,
            },
            None => Pass::Stopped,
        }
    }

    /// The run's end: what was saved said in the log, and its state: done (when files failed,
    /// the others are saved, and said so; starting it again tries those only), else failed.
    fn end(&self, listed: bool, failure: Option<String>) {
        // The files left to gallery-dl that its lines didn't name as listed (a release older
        // than 1.25.0 writes the console's code page, `?` for a name in another script): looked
        // for on the disk, outside the lock, and counted as saved when there.
        let unnamed: Vec<PathBuf> = {
            let sh = self.shared.lock().unwrap();
            sh.left
                .iter()
                .filter(|p| !sh.counted.contains(&key(p)))
                .cloned()
                .collect()
        };
        let found: Vec<(PathBuf, u64)> = unnamed
            .into_iter()
            .filter_map(|p| {
                let m = std::fs::metadata(&p).ok().filter(|m| m.is_file())?;
                Some((p, m.len()))
            })
            .collect();
        let mut sh = self.shared.lock().unwrap();
        for (p, size) in found {
            sh.saved(p, size, true);
        }
        sh.progress.speed = 0.0;
        sh.progress.engine = None;
        sh.helper = false;
        sh.files.listing = false;
        let (saved, there, by_helper) = (sh.saved, sh.there, sh.by_helper);
        let missing = sh
            .left
            .iter()
            .filter(|p| !sh.counted.contains(&key(p)))
            .count();
        let line = if listed && by_helper > 0 {
            format!(
                "Saved {saved} files ({there} already there); gallery-dl downloaded {by_helper} of them itself"
            )
        } else if listed {
            format!("Saved {saved} files ({there} already there)")
        } else {
            format!("gallery-dl saved {saved} files ({there} already there)")
        };
        sh.note(line);
        let failure = failure
            .or_else(|| (missing > 0).then(|| format!("{missing} files couldn't be downloaded")));
        sh.progress.state = match failure {
            None => State::Done(self.folder.clone()),
            Some(f) if saved + there > 0 => {
                sh.note(format!("Some files failed: {f}"));
                State::Done(self.folder.clone())
            }
            Some(f) => State::Failed(f),
        };
    }

    fn end_stopped(&self) {
        let mut sh = self.shared.lock().unwrap();
        sh.progress.speed = 0.0;
        sh.progress.engine = None;
        sh.helper = false;
        sh.files.listing = false;
        sh.progress.state = State::Stopped;
    }

    fn end_failed(&self, why: String) {
        let mut sh = self.shared.lock().unwrap();
        sh.progress.speed = 0.0;
        sh.progress.engine = None;
        sh.helper = false;
        sh.files.listing = false;
        sh.progress.state = State::Failed(why);
    }
}

/// How a listed file's download began.
enum Begun {
    /// Whole already, no resume state beside it: counted as there.
    There,
    /// Under way, and from where.
    Running(Download, Origin),
}

/// Where a listed file's download began.
enum Origin {
    Start,
    /// From its resume state.
    Resumed,
    /// From the start, its resume state unusable (why).
    Restarted(String),
}

/// One file under way.
struct Running {
    file: EngineFile,
    dl: Download,
}

/// The engine's downloads of a gallery's files under way (#769).
struct Fetch<'a> {
    engine: &'a crate::streams::Engine,
    limit: &'a Arc<Limiter>,
    agent: &'a str,
    /// As many connections as Options → Connection gives a file: that many files at once, a few
    /// files sharing them (`start_more`).
    budget: usize,
    running: Vec<Running>,
}

impl<'a> Fetch<'a> {
    fn new(engine: &'a crate::streams::Engine, limit: &'a Arc<Limiter>, agent: &'a str) -> Self {
        Fetch {
            engine,
            limit,
            agent,
            budget: engine.opts.connections().max(1) as usize,
            running: Vec::new(),
        }
    }

    /// Start the files waiting in `queue`, as many at once as the budget's connections: each
    /// gets its share of them as it starts (one each while many are left, more for the last
    /// few), so the gallery takes no more connections than one file would.
    fn start_more(&mut self, queue: &mut VecDeque<EngineFile>, run: &Run) {
        while self.running.len() < self.budget {
            let Some(f) = queue.pop_front() else {
                break;
            };
            let left = queue.len() + 1 + self.running.len();
            let share = (self.budget / left.min(self.budget)).max(1);
            match self.begin(&f, share) {
                Begun::There => run.shared.lock().unwrap().there(f.dest),
                Begun::Running(dl, origin) => {
                    match origin {
                        Origin::Start => {}
                        Origin::Resumed => run.note(format!("{}: resuming", file_name(&f.dest))),
                        Origin::Restarted(why) => run.note(format!(
                            "{}: its saved progress couldn't be used ({why}): from the start",
                            file_name(&f.dest)
                        )),
                    }
                    self.running.push(Running { file: f, dl });
                }
            }
        }
    }

    /// File `f`'s download, over `share` connections: continued from its resume state when there
    /// is one, else from the start; none when it is whole already.
    fn begin(&self, f: &EngineFile, share: usize) -> Begun {
        // An earlier run's files under their former names (`.mdm.*`) go on as today's.
        mdm_engine::state::migrate_legacy(&f.dest);
        let state = mdm_engine::state::state_path(&f.dest);
        if !state.exists() && f.dest.is_file() {
            return Begun::There;
        }
        if let Some(dir) = f.dest.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let mut opts = self.engine.opts.clone();
        opts.max_connections = share as u32;
        // The gallery's limit is for all its files together; the one for all downloads stays.
        opts.speed_limit = None;
        opts.group = Some(self.limit.clone());
        opts.unlocker = Some(Arc::new(GalleryLink::new(f)));
        // gallery-dl's User-Agent, unless its extractor names another for the file: no other.
        let agent = f
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("user-agent"))
            .map_or(self.agent, |(_, v)| v.as_str());
        opts.speak_as(agent.to_owned());
        opts.agent_fallback = None;
        opts.browser_final = None;
        opts.expected_type = None;
        opts.post = None;
        opts.checksum = None;
        opts.request_cap = None;
        let _rt = self.engine.handle.enter();
        let mut origin = Origin::Start;
        if state.exists() {
            match Download::resume(state.clone(), HeaderMap::new(), opts.clone()) {
                Ok(dl) => return Begun::Running(dl, Origin::Resumed),
                // A state that can't be read (another release's, damaged): from the start, and
                // said why.
                Err(e) => {
                    let _ = std::fs::remove_file(&state);
                    let _ = std::fs::remove_file(mdm_engine::naming::part_path(&f.dest));
                    origin = Origin::Restarted(e.to_string());
                }
            }
        }
        Begun::Running(
            Download::start(
                Request {
                    url: f.url.clone(),
                    headers: HeaderMap::new(),
                    dest: Destination::File(f.dest.clone()),
                },
                opts,
            ),
            origin,
        )
    }

    /// The downloads that ended: a file saved is counted; one the engine couldn't download goes
    /// to gallery-dl, from the start (the engine's part file and resume state go).
    fn poll(&mut self, run: &Run) {
        let mut i = 0;
        while i < self.running.len() {
            let state = self.running[i].dl.snapshot().state;
            if !matches!(state, DlState::Done | DlState::Failed | DlState::Cancelled) {
                i += 1;
                continue;
            }
            let Running { file, dl } = self.running.remove(i);
            match self.engine.handle.block_on(dl.wait()) {
                Ok(o) => run.shared.lock().unwrap().saved(o.path, o.size, false),
                Err(e) => {
                    let _ = std::fs::remove_file(mdm_engine::state::state_path(&file.dest));
                    let _ = std::fs::remove_file(mdm_engine::naming::part_path(&file.dest));
                    run.leave(file.dest, Why::Refused(e.to_string()));
                }
            }
        }
    }

    /// The gallery's progress: the bytes of its files saved and under way, the speed and the
    /// connections of those under way (numbered one after another).
    fn report(&self, shared: &Mutex<Shared>) {
        let mut connections: Vec<ConnView> = Vec::new();
        let (mut under_way, mut speed) = (0u64, 0u64);
        for r in &self.running {
            let s = r.dl.snapshot();
            under_way += s.downloaded;
            for c in s.connections {
                speed += c.speed;
                connections.push(ConnView {
                    id: connections.len() as _,
                    ..c
                });
            }
        }
        let mut sh = shared.lock().unwrap();
        sh.progress.downloaded = sh.bytes + under_way;
        sh.progress.speed = speed as f64;
        sh.progress.engine = Some(crate::streams::View {
            segments: Vec::new(),
            connections,
            server: None,
        });
    }

    /// Stop: each download keeps where it got to (its resume state saved) before this returns.
    fn stop(&mut self) {
        for r in &self.running {
            r.dl.cancel();
        }
        for r in self.running.drain(..) {
            let _ = self.engine.handle.block_on(r.dl.wait());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUMS: &[u8] = include_bytes!("../tests/fixtures/gallery-dl-SHA256SUMS");
    const SIG: &[u8] = include_bytes!("../tests/fixtures/gallery-dl-SHA256SUMS.sig");

    #[test]
    fn a_real_release_list_is_signed_by_the_pinned_key() {
        assert!(signed_by_gallery_dl(SUMS, SIG));
        let mut forged = SUMS.to_vec();
        forged[0] = if forged[0] == b'0' { b'1' } else { b'0' };
        assert!(!signed_by_gallery_dl(&forged, SIG));
        // Each key signs for itself only.
        assert!(!crate::signed_by_ytdlp(SUMS, SIG));
        let sums = String::from_utf8_lossy(SUMS);
        assert_eq!(
            hex::encode(crate::sum_for(&sums, "gallery-dl.exe").unwrap()),
            "4dda3f14efbc8a8e616af1fc7e1e88d795995326b47692453e620117254dec18"
        );
    }

    /// #236: each release gets only the options it accepts, as found by asking every release
    /// from 1.17 to 1.30 (Ubuntu 24.04's 1.26.9 refused `--no-input`).
    #[test]
    fn each_release_gets_only_the_options_it_accepts() {
        let accepts = |v: &str| {
            let a = Accepts::of(v);
            (a.no_input, a.user_agent)
        };
        assert_eq!(accepts("1.20.0"), (false, false));
        assert_eq!(accepts("1.23.5"), (false, false));
        assert_eq!(accepts("1.24.0"), (false, true));
        assert_eq!(accepts("1.26.9"), (false, true));
        assert_eq!(accepts("1.27.0"), (false, true));
        assert_eq!(accepts("1.27.1"), (true, true));
        assert_eq!(accepts("1.32.19"), (true, true));
        assert_eq!(accepts("1.31.0-dev"), (true, true));
        assert_eq!(accepts("2.0"), (true, true));
        // One that can't be read: as recent as DDM's own.
        assert_eq!(accepts("unknown"), (true, true));
        // The system's, not asked (#571): what every release from 1.20.0 accepts.
        assert_eq!(accepts("system"), (false, false));
        // #769: the files' list for DDM's engine from `--print` (1.27.7) on; the system's is
        // asked for it too (one that refuses the options downloads the files itself).
        let listing = |v: &str| Accepts::of(v).listing;
        assert!(!listing("1.20.5") && !listing("1.26.9") && !listing("1.27.6"));
        assert!(listing("1.27.7") && listing("1.29.7") && listing("1.31.0-dev"));
        assert!(listing("unknown") && listing("system"));
        assert!(!too_old("system"));
        assert!(too_old("1.17.0") && too_old("1.19.3") && too_old("0.9"));
        assert!(!too_old("1.20.0") && !too_old("1.26.9") && !too_old("unknown"));

        let args = |v: &str| -> Vec<String> {
            let tool = Installed {
                version: v.into(),
                exe: "gallery-dl".into(),
            };
            proc::gallery_dl(&tool, &["--sleep".into(), "1".into()], "https://x.test/g")
                .unwrap()
                .get_args()
                .map(|a| a.to_string_lossy().into_owned())
                .collect()
        };
        assert_eq!(
            args("1.26.9"),
            [
                "--sleep",
                "1",
                "--ignore-config",
                "-o",
                "output.stdout=utf-8",
                "-o",
                "output.stderr=utf-8",
                "--",
                "https://x.test/g"
            ]
        );
        assert_eq!(
            args("1.27.1"),
            [
                "--sleep",
                "1",
                "--ignore-config",
                "-o",
                "output.stdout=utf-8",
                "-o",
                "output.stderr=utf-8",
                "--no-input",
                "--",
                "https://x.test/g"
            ]
        );
    }

    /// The browser's User-Agent (else DDM's) and cookies, as each release takes them.
    #[test]
    fn the_browsers_context_as_each_release_takes_it() {
        let theirs = || -> Vec<String> {
            [
                "--referer",
                "https://x.test/",
                "--user-agent",
                "Firefox/150",
                "--cookies",
                "/c.txt",
            ]
            .map(String::from)
            .to_vec()
        };
        let new = Accepts::of("1.30.0");
        let old = Accepts::of("1.20.5");
        assert_eq!(
            browser_args(theirs(), new),
            ["--cookies", "/c.txt", "--user-agent", "Firefox/150"]
        );
        assert_eq!(
            browser_args(theirs(), old),
            ["--cookies", "/c.txt", "-o", "user-agent=Firefox/150"]
        );
        assert_eq!(browser_args(Vec::new(), new), ["--user-agent", USER_AGENT]);
        assert_eq!(
            browser_args(Vec::new(), old),
            ["-o".to_owned(), format!("user-agent={USER_AGENT}")]
        );
    }

    /// A gallery-dl chosen before #236 checked, too old for DDM: DDM's own copy instead.
    #[test]
    fn a_chosen_release_too_old_gives_way_to_mdms_own() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("old-gallery-dl");
        std::fs::write(&exe, b"x").unwrap();
        let json = serde_json::json!({ "gallery-dl": { "version": "1.17.0", "exe": exe } });
        std::fs::write(dir.path().join("programs.json"), json.to_string()).unwrap();
        // Not the old one: on Linux and macOS a gallery-dl the system has may stand in (#571).
        assert!(
            installed(dir.path()).is_none_or(|i| i.version != "1.17.0"),
            "not the old one"
        );
        assert_eq!(
            chosen_too_old(dir.path()).map(|i| i.version).as_deref(),
            Some("1.17.0")
        );
        let own = crate::put(
            dir.path(),
            "gallery-dl",
            "1.32.19",
            asset().unwrap_or("gallery-dl"),
            b"x",
            "https://x/",
        )
        .unwrap();
        assert_eq!(installed(dir.path()), Some(own));
    }

    #[test]
    fn its_lines_are_files_saved_or_already_there() {
        let saved = if cfg!(windows) {
            r"C:\Users\x\Pictures\wikimediacommons\Birds\Rhea (de11b288).jpg"
        } else {
            "/home/x/Pictures/wikimediacommons/Birds/Rhea (de11b288).jpg"
        };
        assert_eq!(
            parse_line(&format!("{saved}\r\n")),
            Some(Line::Saved(PathBuf::from(saved)))
        );
        assert_eq!(
            parse_line(&format!("# {saved}")),
            Some(Line::There(PathBuf::from(saved)))
        );
        assert_eq!(parse_line("[wikimedia][info] something"), None);
        assert_eq!(parse_line(""), None);
    }

    #[test]
    fn a_long_output_is_counted_without_holding_it() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.jpg");
        let json = |s: &str| serde_json::to_string(s).unwrap();
        let mut text = String::new();
        for i in 0..5000 {
            let url = format!("https://upload.example/{i}.jpg");
            text.push_str(&format!(
                "MDMG\t{}\t{}\t\"jpg\"\tnull\n{}\n# {}\n",
                json(&url),
                json(&file.to_string_lossy()),
                file.display(),
                file.display()
            ));
        }
        // A line far longer than any path or listing is skipped to its end, whatever it holds.
        text.push_str(&format!("# {}", "x".repeat(200_000)));
        text.push('\n');
        text.push_str(&format!("{}\n", file.display()));
        let (mut listed, mut saved, mut there) = (0, 0, 0);
        lines(text.as_bytes(), |l| match l {
            Line::Listed(l) => {
                assert_eq!(l.path, file);
                listed += 1;
            }
            Line::Saved(p) => {
                assert_eq!(p, file);
                saved += 1;
            }
            Line::There(p) => {
                assert_eq!(p, file);
                there += 1;
            }
        });
        assert_eq!((listed, saved, there), (5000, 5001, 5000));
    }

    /// The path gallery-dl gives a file, for its place in the gallery's folder.
    fn sample(folder: &Path, name: &str) -> PathBuf {
        folder.join(name)
    }

    /// A listing line as gallery-dl's `--print` writes it ([`LISTED`]): each field as JSON
    /// (`json.dumps`: non-ASCII letters escaped, a path's backslashes doubled).
    fn printed(url: &str, path: &Path, ext: Option<&str>, headers: &str) -> String {
        let ascii = |s: &str| {
            s.chars()
                .map(|c| match c {
                    '"' => "\\\"".to_owned(),
                    '\\' => "\\\\".to_owned(),
                    c if c.is_ascii() && !c.is_ascii_control() => c.to_string(),
                    c => c
                        .encode_utf16(&mut [0u16; 2])
                        .iter()
                        .map(|u| format!("\\u{u:04x}"))
                        .collect(),
                })
                .collect::<String>()
        };
        let text = |s: &str| format!("\"{}\"", ascii(s));
        format!(
            "MDMG\t{}\t{}\t{}\t{headers}\n",
            text(url),
            text(&path.to_string_lossy()),
            ext.map_or_else(|| "null".to_owned(), text)
        )
    }

    /// #769: gallery-dl's listing as `--print` writes it for a gallery's files (a Wikimedia
    /// Commons picture with a name of its site's, one whose extractor wants a header of its own,
    /// a video it hands to yt-dlp, a text, one whose type comes from the server): each one's
    /// address, place and headers, and which DDM's engine downloads.
    #[test]
    fn its_listing_says_each_files_address_place_and_headers() {
        let folder = if cfg!(windows) {
            PathBuf::from(r"C:\Users\x\Downloads\commons.wikimedia.org - Category_Birds")
        } else {
            PathBuf::from("/home/x/Downloads/commons.wikimedia.org - Category_Birds")
        };
        let page = Url::parse("https://commons.wikimedia.org/wiki/Category:Birds").unwrap();
        let rhea = sample(&folder, "Rhea americana – Nandú (de11b288).jpg");
        let line = printed(
            "https://upload.wikimedia.org/wikipedia/commons/a/a5/Rhea_americana.jpg",
            &rhea,
            Some("jpg"),
            "null",
        );
        let Some(Line::Listed(l)) = parse_line(&line) else {
            panic!("{line}")
        };
        assert_eq!(l.path, rhea, "letters beyond ASCII come back as they were");
        assert_eq!(l.extension, "jpg");
        assert!(l.headers.is_empty());
        let f = classify(&l, &folder, &page).unwrap();
        assert_eq!(f.dest, rhea);
        assert_eq!(
            f.url.as_str(),
            "https://upload.wikimedia.org/wikipedia/commons/a/a5/Rhea_americana.jpg"
        );
        // The site as the page the file comes from, as gallery-dl's requests say.
        assert_eq!(
            f.headers,
            [(
                "Referer".to_owned(),
                "https://commons.wikimedia.org/".to_owned()
            )]
        );

        // Headers of its extractor's own (`_http_headers`): kept, its Referer the only one.
        let clip = sample(&folder, "clip-4f2a.mp4");
        let line = printed(
            "https://media-files9.bunkr.example/clip-4f2a.mp4",
            &clip,
            Some("mp4"),
            r#"{"Referer": "https://bunkr.example/", "Accept": "video/*", "X-Bad": 1}"#,
        );
        let Some(Line::Listed(l)) = parse_line(&line) else {
            panic!("{line}")
        };
        let f = classify(&l, &folder, &page).unwrap();
        assert_eq!(f.dest, clip);
        assert!(
            f.headers
                .contains(&("Referer".into(), "https://bunkr.example/".into()))
        );
        assert!(f.headers.contains(&("Accept".into(), "video/*".into())));
        assert_eq!(f.headers.len(), 2, "a header that isn't text is left out");

        // What stays gallery-dl's: a video it hands to yt-dlp (no extension of its own until
        // then), a text it writes, an address of another kind, a type the server tells.
        let theirs = |url: &str, name: &str, ext: Option<&str>| {
            let line = printed(url, &sample(&folder, name), ext, "None");
            let Some(Line::Listed(l)) = parse_line(&line) else {
                panic!("{line}")
            };
            classify(&l, &folder, &page).unwrap_err()
        };
        assert_eq!(
            theirs(
                "ytdl:https://twitter.com/i/web/status/1790000000000000000",
                "twitter_1790000000000000000_1",
                None
            ),
            Why::Delegated
        );
        assert_eq!(
            theirs("text:A caption of the post", "post-12.txt", Some("txt")),
            Why::Text
        );
        assert_eq!(
            theirs("ftp://files.example/a.jpg", "a.jpg", Some("jpg")),
            Why::Scheme("ftp".into())
        );
        assert_eq!(
            theirs("https://cdn.example/media/12", "12", None),
            Why::NoExtension
        );

        // A release without `url-metadata` / `path-metadata` writes `None`: no listing, so its
        // path line alone says the file (gallery-dl downloads it).
        assert_eq!(parse_line("MDMG\tNone\tNone\t\"jpg\"\tNone"), None);
        assert_eq!(parse_line("MDMG\tnull\tnull\tnull\tnull\r\n"), None);
    }

    /// A listed file's place is in the gallery's folder: gallery-dl's own path in it (letter
    /// case aside on Windows), else its name there; never one that climbs out.
    #[test]
    fn a_listed_file_stays_in_the_gallerys_folder() {
        let folder = if cfg!(windows) {
            PathBuf::from(r"C:\Users\x\Gal")
        } else {
            PathBuf::from("/home/x/Gal")
        };
        assert_eq!(
            within(&folder, &folder.join("a.jpg")),
            Some(folder.join("a.jpg"))
        );
        assert_eq!(
            within(&folder, &folder.join("sub").join("b.png")),
            Some(folder.join("sub").join("b.png")),
            "a sub-folder of gallery-dl's"
        );
        let elsewhere = folder.parent().unwrap().join("Other").join("c.jpg");
        assert_eq!(within(&folder, &elsewhere), Some(folder.join("c.jpg")));
        assert_eq!(within(&folder, &folder.join("..").join("d.jpg")), None);
        assert_eq!(within(&folder, &folder), None);
        if cfg!(windows) {
            assert_eq!(
                within(&folder, Path::new(r"c:\users\X\gal\e.jpg")),
                Some(folder.join("e.jpg"))
            );
            assert_eq!(
                within(&folder, Path::new(r"\\?\C:\Users\x\Gal\f.jpg")),
                Some(folder.join("f.jpg"))
            );
            // Names Windows doesn't keep as written: another stream of a file, a dot or a
            // space at the end, a device's (in a sub-folder too).
            for odd in [
                "h.jpg:hidden",
                "i.jpg.",
                "j.jpg ",
                "CON.jpg",
                "nul",
                "com1.png",
                "Lpt9 .gif",
                "COM0.jpg",
                "COM\u{b9}.jpg",
            ] {
                assert_eq!(within(&folder, &folder.join(odd)), None, "{odd}");
            }
            assert_eq!(within(&folder, &folder.join("aux").join("k.jpg")), None);
            for fine in ["COM10.jpg", "console.jpg", "a.b.jpg", "lpt.png"] {
                assert_eq!(
                    within(&folder, &folder.join(fine)),
                    Some(folder.join(fine)),
                    "{fine}"
                );
            }
        }
        // Listed outside it in a way that can't be placed: gallery-dl's to download.
        let page = Url::parse("https://x.example/g").unwrap();
        let l = Listed {
            url: "https://x.example/g.jpg".into(),
            path: folder.join("..").join("g.jpg"),
            extension: "jpg".into(),
            headers: Vec::new(),
        };
        assert_eq!(classify(&l, &folder, &page), Err(Why::Outside));
    }

    /// #769: the listing's arguments: the gallery's folder, nothing downloaded, the format
    /// `--print` writes after each file's path is made, both spellings of the metadata options,
    /// at most so many files; and no `--sleep`, which would wait a second for each file listed.
    #[test]
    fn the_listing_asks_for_each_files_address_and_place() {
        let folder = Path::new("/g/Birds");
        let a = listing_args(
            folder,
            &Ask {
                limit: Some(30),
                rate: Some(1000),
            },
        );
        assert_eq!(
            a,
            [
                "--directory",
                "/g/Birds",
                "--no-download",
                "--print",
                "file:MDMG\t{gdl_url!j}\t{gdl_path.path!j}\t{extension!j}\t{_http_headers!j}",
                "-o",
                "url-metadata=gdl_url",
                "-o",
                "metadata-url=gdl_url",
                "-o",
                "path-metadata=gdl_path",
                "-o",
                "metadata-path=gdl_path",
                "--range",
                "1-30",
            ]
        );
        // gallery-dl downloading itself: today's arguments, its limit at the start.
        assert_eq!(
            download_args(folder, &Ask::default(), Some(2048)),
            [
                "--directory",
                "/g/Birds",
                "--sleep",
                "1",
                "--limit-rate",
                "2048"
            ]
        );
    }

    #[test]
    fn a_gallery_is_named_by_its_address() {
        let u = |s: &str| url::Url::parse(s).unwrap();
        assert_eq!(
            name_of(&u(
                "https://commons.wikimedia.org/wiki/Category:Featured_pictures_of_birds"
            )),
            "commons.wikimedia.org - Category_Featured pictures of birds"
        );
        assert_eq!(
            name_of(&u(
                "https://www.flickr.com/photos/nasawebbtelescope/albums/72177720313923911/"
            )),
            "flickr.com - 72177720313923911"
        );
        assert_eq!(name_of(&u("https://imgur.com/")), "imgur.com");
    }

    #[test]
    fn installs_go_in_their_own_folder() {
        let dir = tempfile::tempdir().unwrap();
        let name = asset().unwrap_or("gallery-dl");
        let now = crate::put(
            dir.path(),
            "gallery-dl",
            "1.32.19",
            name,
            b"x",
            "https://x/",
        )
        .unwrap();
        assert_eq!(installed(dir.path()), Some(now));
        assert!(crate::own_ytdlp(dir.path()).is_none(), "not yt-dlp's");
    }
}
