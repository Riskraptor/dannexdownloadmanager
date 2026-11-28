//! An answer that is a web page, not the file: a bot check, a page in place of a file the address
//! names, the names and extensions that expect one or the other.

use super::*;

/// Types of file a web page never is, by extension: documents, archives, programs, disk images,
/// audio, video and pictures. A name without one of these (a page's `.php`, no extension at all)
/// says nothing.
pub(super) const FILE_EXTENSIONS: &[&str] = &[
    "doc",
    "docx",
    "docm",
    "dot",
    "dotx",
    "xls",
    "xlsx",
    "xlsm",
    "xlsb",
    "ppt",
    "pptx",
    "pps",
    "ppsx",
    "odt",
    "ods",
    "odp",
    "rtf",
    "pdf",
    "epub",
    "mobi",
    "djvu",
    "zip",
    "rar",
    "7z",
    "gz",
    "tgz",
    "bz2",
    "xz",
    "zst",
    "tar",
    "cab",
    "arj",
    "lz",
    "lzh",
    "exe",
    "msi",
    "msix",
    "msixbundle",
    "appx",
    "apk",
    "deb",
    "rpm",
    "appimage",
    "dmg",
    "pkg",
    "jar",
    "iso",
    "img",
    "vhd",
    "vhdx",
    "mp3",
    "m4a",
    "aac",
    "flac",
    "wav",
    "wma",
    "ogg",
    "opus",
    "mp4",
    "m4v",
    "mkv",
    "avi",
    "mov",
    "wmv",
    "flv",
    "webm",
    "mpg",
    "mpeg",
    "jpg",
    "jpeg",
    "png",
    "gif",
    "bmp",
    "webp",
    "tif",
    "tiff",
    "psd",
    "torrent",
];

/// A site's bot check turning the request away: Cloudflare's, which marks it
/// (`cf-mitigated: challenge`, #376), or Akamai's, whose edge answers a connection it takes for a
/// program's with its own 403 page ("Access Denied", served by `AkamaiGHost`, #771), where a
/// browser gets the file. The site's own 403 behind Akamai comes from its servers, not the edge's
/// page, and stays a plain refusal.
pub(crate) fn is_bot_check(status: StatusCode, h: &HeaderMap) -> bool {
    status == StatusCode::FORBIDDEN
        && (h
            .get("cf-mitigated")
            .is_some_and(|v| v.as_bytes().eq_ignore_ascii_case(b"challenge"))
            || (header_str(h, header::SERVER)
                .is_some_and(|s| s.trim().eq_ignore_ascii_case("AkamaiGHost"))
                && is_web_page(h)))
}

pub(crate) fn is_web_page(h: &HeaderMap) -> bool {
    h.get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(is_web_page_type)
}

pub(super) fn is_web_page_type(content_type: &str) -> bool {
    let mime = content_type.split(';').next().unwrap_or("").trim();
    mime.eq_ignore_ascii_case("text/html") || mime.eq_ignore_ascii_case("application/xhtml+xml")
}

/// The server names what it sends (`Content-Disposition`): its own file, page or not.
pub(crate) fn named(h: &HeaderMap) -> bool {
    h.contains_key(header::CONTENT_DISPOSITION)
}

/// Where a download is saved under a name of its own (the browser's, the user's), that name.
pub(super) fn dest_name(dest: &Destination) -> Option<&str> {
    match dest {
        Destination::File(p) | Destination::Free(p) => p.file_name().and_then(|n| n.to_str()),
        Destination::Dir(_) => None,
    }
}

/// The download expects a kind of file a web page isn't (#533): the browser's own download had
/// another type (`expected_type`), or its name (`name`, else the address's) is a file's. A web
/// page in its place, without a name of the server's, is an error (`Error::GotWebPage`). A page
/// the browser saved as one (`text/html`), a `.html` link, or a server's own type for the file
/// (`application/octet-stream`) are downloads like any other.
pub(super) fn expects_other_file(
    expected_type: Option<&str>,
    name: Option<&str>,
    url: &Url,
) -> bool {
    if let Some(t) = expected_type.map(str::trim).filter(|t| !t.is_empty()) {
        return !is_web_page_type(t);
    }
    let name = name.map_or_else(|| naming::file_name(None, url), str::to_owned);
    Path::new(&name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| FILE_EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// Bytes that start as an HTML document (after a byte order mark and white space).
pub(super) fn sniffs_as_html(body: &[u8]) -> bool {
    let body = body.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(body);
    let start = body
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(body.len());
    let head: Vec<u8> = body[start..]
        .iter()
        .take(16)
        .map(u8::to_ascii_lowercase)
        .collect();
    ["<!doctype html", "<html", "<head", "<body", "<title"]
        .iter()
        .any(|t| head.starts_with(t.as_bytes()))
}

/// Whether the download is itself a web page (it was served as one, or is named like one).
pub(super) fn expects_web_page(dest: &Path, content_type: Option<&str>) -> bool {
    content_type.is_some_and(is_web_page_type)
        || dest.extension().and_then(|e| e.to_str()).is_some_and(|e| {
            ["htm", "html", "xhtml", "shtml"]
                .iter()
                .any(|x| e.eq_ignore_ascii_case(x))
        })
}

#[cfg(test)]
mod bot_check_tests {
    use super::*;

    fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(*k, header::HeaderValue::from_static(v));
        }
        h
    }

    /// Cloudflare's challenge and Akamai's edge refusal (#771) are bot checks; a 403 from the site
    /// itself, or Akamai's edge sending something other than its page, is not.
    #[test]
    fn bot_checks_are_cloudflares_challenge_and_akamais_edge_page() {
        let forbidden = StatusCode::FORBIDDEN;
        assert!(is_bot_check(
            forbidden,
            &headers(&[("cf-mitigated", "challenge")])
        ));
        let akamai = headers(&[("server", "AkamaiGHost"), ("content-type", "text/html")]);
        assert!(is_bot_check(forbidden, &akamai));
        assert!(!is_bot_check(StatusCode::NOT_FOUND, &akamai));
        assert!(!is_bot_check(
            forbidden,
            &headers(&[
                ("server", "AkamaiGHost"),
                ("content-type", "application/pdf")
            ])
        ));
        assert!(!is_bot_check(
            forbidden,
            &headers(&[("server", "nginx"), ("content-type", "text/html")])
        ));
        assert!(!is_bot_check(forbidden, &HeaderMap::new()));
    }
}
