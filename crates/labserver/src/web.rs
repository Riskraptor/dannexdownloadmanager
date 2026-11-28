//! A web site to save for offline reading (the site grabber's offline copies, crates/sitegrab):
//! real pictures (PNG), a style sheet importing another, a script, a PDF, pages a few levels deep
//! linked every way a site links them (relative, from the root, protocol-relative, absolute,
//! with a fragment, twice), a page without an extension, a folder's page, pages with a query, a
//! folder and a file named beyond ASCII, a redirect to a folder, a page slow to answer, a
//! `<base href>`, a refresh, `srcset`, `<picture>`, a picture loaded late (`data-src`), CSS in a
//! `style` attribute and a `<style>` element, a frame, a picture on another site
//! (`localhost:PORT`), a folder robots.txt keeps robots out of, pages that need a cookie the user
//! gives or one the site sets, and under `/web/big/` a site of thousands of pages.

use std::time::Duration;

use axum::extract::{Path, RawQuery};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};

/// Pages under `/web/big/`.
pub const BIG_PAGES: usize = 3000;

fn html(body: String) -> Response {
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], body).into_response()
}

fn typed(mime: &'static str, body: impl Into<Vec<u8>>) -> Response {
    ([(header::CONTENT_TYPE, mime)], body.into()).into_response()
}

/// `host:port` the request was sent to.
fn host(headers: &HeaderMap) -> String {
    headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("127.0.0.1")
        .to_owned()
}

fn port(headers: &HeaderMap) -> String {
    host(headers)
        .rsplit_once(':')
        .map_or_else(|| "80".to_owned(), |(_, p)| p.to_owned())
}

fn cookie(headers: &HeaderMap, pair: &str) -> bool {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .any(|v| v.split(';').any(|c| c.trim() == pair))
}

/// `/web/` itself.
pub(crate) async fn index(headers: HeaderMap) -> Response {
    page(&headers, "", None).await
}

/// `/web/{*path}` (axum decodes the path: `café/menü.html`).
pub(crate) async fn path(
    Path(path): Path<String>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
) -> Response {
    page(&headers, &path, query.as_deref()).await
}

async fn page(headers: &HeaderMap, path: &str, query: Option<&str>) -> Response {
    let host = host(headers);
    let other = format!("http://localhost:{}", port(headers));
    if path.ends_with(".png") {
        let name = path.rsplit('/').next().unwrap_or(path);
        return typed("image/png", png(name));
    }
    if let Some(rest) = path.strip_prefix("big/") {
        return big(rest);
    }
    let body = match path {
        "" | "index.html" => format!(
            r##"<!doctype html><html><head><meta charset="utf-8"><title>Offline lab</title>
<link rel="stylesheet" href="css/main.css">
<link rel="icon" href="/web/img/favicon.png">
<script src="js/app.js"></script>
<style>.hero {{ background: url(img/styled.png) }}</style>
</head><body>
<h1 id="top">Offline lab</h1>
<nav><ul>
<li><a id="about" href="about">About</a>
<li><a id="docs" href="docs/">Documents</a>
<li><a id="part" href="docs/#part">Documents, a part</a>
<li><a id="post1" href="/web/blog/post?id=1">First post</a>
<li><a id="post2" href="blog/post?id=2">Second post</a>
<li><a id="menu" href="caf%C3%A9/men%C3%BC.html">Menü</a>
<li><a id="moved" href="redirect-me">Moved</a>
<li><a id="slow" href="slow.html">Slow</a>
<li><a id="proto" href="//{host}/web/proto.html">Protocol-relative</a>
<li><a id="absolute" href="http://{host}/web/absolute.html">Absolute</a>
<li><a id="elsewhere" href="{other}/web/elsewhere.html">Another site</a>
<li><a id="away" href="https://example.invalid/away">Away</a>
<li><a id="mail" href="mailto:lab@example.invalid">Mail</a>
<li><a id="report" href="files/report.pdf">Report (PDF)</a>
<li><a id="again" href="about">About, again</a>
<li><a id="private" href="private/hidden.html">Private</a>
<li><a id="consent" href="consent">Cookies</a>
<li><a id="members" href="members/">Members</a>
<li><a id="refresh" href="refresh.html">Refresh</a>
<li><a id="top-link" href="#top">Top</a>
</ul></nav>
<img id="logo" src="img/logo.png" srcset="img/logo@2x.png 2x" alt="logo">
<img id="lazy" src="data:image/gif;base64,R0lGODlhAQABAAAAACw=" data-src="img/lazy.png" alt="lazy">
<picture><source srcset="img/hero.png" type="image/png"><img id="hero" src="img/hero-small.png" alt="hero"></picture>
<div id="inline" class="hero" style="background-image:url('img/inline.png')">styled</div>
<img id="cdn" src="{other}/web/img/cdn.png" alt="cdn">
<iframe id="frame" src="frame.html"></iframe>
</body></html>"##
        ),
        "about" => r#"<!doctype html><title>About</title><h1>About</h1>
<a id="home" href="./">Home</a> <a id="guide" href="docs/guide.html">Guide</a>
<img id="photo" src="img/photo.png" alt="photo">"#
            .to_owned(),
        "docs/" | "docs/index.html" => r#"<!doctype html><title>Documents</title>
<h1 id="part">Documents</h1>
<a id="home" href="../">Home</a> <a id="guide" href="guide.html">Guide</a>
<a id="report" href="/web/files/report.pdf">Report</a> <a id="based" href="based.html">Based</a>"#
            .to_owned(),
        "docs/guide.html" => r#"<!doctype html><title>Guide</title>
<a id="deeper" href="deep/a/b.html">Deeper</a> <a id="about" href="../about">About</a>
<img id="photo" src="../img/photo.png" alt="photo">"#
            .to_owned(),
        "docs/based.html" => r#"<!doctype html><title>Based</title><base href="/web/files/">
<a id="report" href="report.pdf">Report</a> <a id="home" href="../">Home</a>"#
            .to_owned(),
        "docs/deep/a/b.html" => r#"<!doctype html><title>Deep</title>
<a id="home" href="/web/">Home</a> <img id="deep" src="/web/img/deep.png" alt="deep">"#
            .to_owned(),
        "blog/post" => return post(query),
        "café/menü.html" => r#"<!doctype html><meta charset="utf-8"><title>Menü</title>
<a id="home" href="../">Home</a> <img id="nandu" src="%C3%B1and%C3%BA.png" alt="ñandú">"#
            .to_owned(),
        "redirect-me" => {
            return (
                StatusCode::MOVED_PERMANENTLY,
                [(header::LOCATION, "/web/moved/")],
            )
                .into_response();
        }
        "moved/" => r#"<!doctype html><title>Moved</title>
<img id="here" src="here.png" alt="here"> <a id="home" href="../">Home</a>"#
            .to_owned(),
        "slow.html" => {
            tokio::time::sleep(Duration::from_secs(3)).await;
            r#"<!doctype html><title>Slow</title><a href="./">Home</a>"#.to_owned()
        }
        "proto.html" | "absolute.html" | "frame.html" | "elsewhere.html" => {
            r#"<!doctype html><title>A page</title><p>A page.</p><a id="home" href="./">Home</a>"#
                .to_owned()
        }
        "private/hidden.html" => "<!doctype html><title>Private</title>".to_owned(),
        "refresh.html" => r#"<!doctype html><title>Refresh</title>
<meta http-equiv="refresh" content="300; url=about"><p>Refreshes to About in five minutes.</p>"#
            .to_owned(),
        // A page that sets a cookie, and one that needs it: a crawler keeps a session's cookies.
        "consent" => {
            return (
                [
                    (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                    (header::SET_COOKIE, "consent=yes; Path=/; HttpOnly"),
                ],
                r#"<!doctype html><title>Consent</title><a id="needs" href="needs-consent.html">Next</a>"#,
            )
                .into_response();
        }
        "needs-consent.html" if cookie(headers, "consent=yes") => {
            r#"<!doctype html><title>Consented</title><img src="img/consented.png" alt="">"#
                .to_owned()
        }
        // A page for members: the cookie the user gives (`member=yes`).
        "members/" if cookie(headers, "member=yes") => {
            r#"<!doctype html><title>Members</title><img src="../img/member.png" alt="">"#
                .to_owned()
        }
        "needs-consent.html" | "members/" => {
            return (StatusCode::FORBIDDEN, "no cookie\n").into_response();
        }
        "css/main.css" => {
            return typed(
                "text/css",
                "@import \"fonts.css\";\nbody { background: #fff url(../img/bg.png) repeat-x; }\n\
                 .logo { background-image: url(\"/web/img/logo.png\"); }\n\
                 /* url(../img/commented.png) */\n",
            );
        }
        "css/fonts.css" => {
            return typed(
                "text/css",
                "@font-face { font-family: Lab; src: url(../fonts/lab.woff2) format(\"woff2\"); }\n",
            );
        }
        "fonts/lab.woff2" => return typed("font/woff2", b"wOF2-not-a-real-font".to_vec()),
        "js/app.js" => {
            return typed(
                "text/javascript",
                "document.addEventListener('DOMContentLoaded', function () {\n  document.body.setAttribute('data-js', 'ran');\n});\n",
            );
        }
        "files/report.pdf" => return typed("application/pdf", PDF.to_vec()),
        _ => return (StatusCode::NOT_FOUND, "no such page\n").into_response(),
    };
    html(body)
}

/// `/web/blog/post?id=N` (1 or 2): each post links to the other, its picture has a query too.
fn post(query: Option<&str>) -> Response {
    let n = match query {
        Some("id=1") => 1,
        Some("id=2") => 2,
        _ => return (StatusCode::NOT_FOUND, "no such post\n").into_response(),
    };
    let other = 3 - n;
    html(format!(
        r#"<!doctype html><title>Post {n}</title><h1>Post {n}</h1>
<a id="other" href="post?id={other}">Post {other}</a> <a id="home" href="../">Home</a>
<img id="pic" src="/web/img/post.png?v={n}" alt="">"#
    ))
}

/// `/web/big/`: an index of [`BIG_PAGES`] pages, each linking back and to the next three.
fn big(rest: &str) -> Response {
    if rest.is_empty() || rest == "index.html" {
        let mut body = String::from("<!doctype html><title>Big</title><ul>\n");
        for i in 0..BIG_PAGES {
            body.push_str(&format!("<li><a href=\"p{i}.html\">Page {i}</a>\n"));
        }
        body.push_str("</ul>");
        return html(body);
    }
    let Some(n) = rest
        .strip_prefix('p')
        .and_then(|r| r.strip_suffix(".html"))
        .and_then(|n| n.parse::<usize>().ok())
        .filter(|n| *n < BIG_PAGES)
    else {
        return (StatusCode::NOT_FOUND, "no such page\n").into_response();
    };
    let mut body = format!("<!doctype html><title>Page {n}</title><a href=\"./\">Index</a>\n");
    for k in 1..=3 {
        body.push_str(&format!(
            "<a href=\"p{}.html\">Next</a>\n",
            (n + k) % BIG_PAGES
        ));
    }
    html(body)
}

/// A small PDF.
const PDF: &[u8] = b"%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n\
2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n\
3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 72 72]>>endobj\n\
trailer<</Root 1 0 R>>\n%%EOF\n";

/// A real PNG of 8 by 8 pixels in a colour made from `name` (each picture its own).
fn png(name: &str) -> Vec<u8> {
    let h = name.bytes().fold(0x811c_9dc5u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    });
    let rgb = [(h >> 16) as u8, (h >> 8) as u8, h as u8];
    let (w, ht) = (8u32, 8u32);
    // Each row: no filter (0), then its pixels.
    let row: Vec<u8> = std::iter::once(0)
        .chain(rgb.iter().copied().cycle().take(3 * w as usize))
        .collect();
    let raw = row.repeat(ht as usize);
    // zlib: one stored deflate block.
    let mut z = vec![0x78, 0x01, 0x01];
    let len = raw.len() as u16;
    z.extend_from_slice(&len.to_le_bytes());
    z.extend_from_slice(&(!len).to_le_bytes());
    z.extend_from_slice(&raw);
    z.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&ht.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    for (kind, data) in [
        (&b"IHDR"[..], ihdr),
        (&b"IDAT"[..], z),
        (&b"IEND"[..], Vec::new()),
    ] {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut c = kind.to_vec();
        c.extend_from_slice(&data);
        out.extend_from_slice(&c);
        out.extend_from_slice(&crc32(&c).to_be_bytes());
    }
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xffff_ffffu32;
    for &b in data {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xedb8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
    }
    !c
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + u32::from(x)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pictures_are_real_pngs() {
        let p = png("logo.png");
        assert!(p.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!(crc32(b"IEND"), 0xae42_6082);
        assert_eq!(adler32(b"Wikipedia"), 0x11e6_0398);
        assert_ne!(png("a.png"), png("b.png"));
    }
}
