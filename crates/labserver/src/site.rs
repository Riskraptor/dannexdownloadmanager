//! A small web site for the site grabber's tests (crates/sitegrab): pages a few links deep under
//! `/site/`, a style sheet that imports another, pictures in `src`, `srcset`, `style` and CSS,
//! a video and its poster, documents behind a `<base href>`, a download whose address names no
//! file, a broken link, a page on "another site" (`other.test` on this server's port: a test
//! maps that name to this server), a folder its `/robots.txt` disallows, and links a browser
//! never follows (a comment, a script's text). Every file is one of `/gen`'s.

use axum::extract::Path;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};

/// `/robots.txt`: every robot kept out of `/site/private/`.
pub(crate) async fn robots() -> Response {
    (
        [(header::CONTENT_TYPE, "text/plain")],
        "# The lab site's (crates/labserver/src/site.rs, web.rs)\nUser-agent: *\nDisallow: /site/private/\nDisallow: /web/private/\n",
    )
        .into_response()
}

fn html(body: String) -> Response {
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], body).into_response()
}

/// The port this server was asked on (for the other site's absolute links).
fn port(headers: &HeaderMap) -> String {
    headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.rsplit_once(':'))
        .map(|(_, p)| p.to_owned())
        .unwrap_or_else(|| "80".into())
}

/// `/site/` itself.
pub(crate) async fn index(headers: HeaderMap) -> Response {
    page(&headers, "")
}

/// `/site/{*path}`.
pub(crate) async fn path(Path(path): Path<String>, headers: HeaderMap) -> Response {
    page(&headers, &path)
}

fn page(headers: &HeaderMap, path: &str) -> Response {
    let other = format!("http://other.test:{}", port(headers));
    let body = match path {
        "" | "index.html" => format!(
            r#"<!doctype html><html><head><meta charset=utf-8><title>Lab site</title>
<link rel=stylesheet href="style.css"><link rel="icon" href="/gen/1K/favicon.ico">
</head><body><h1>Lab site</h1>
<!-- <a href="commented.html">not a link</a> -->
<ul>
<li><a href="gallery.html">Gallery</a>
<li><a href="docs/">Documents</a>
<li><a href="private/secret.html">Private</a>
<li><a href="missing.html">A broken link</a>
<li><a href="/gen/7K?cd=bundle.zip">A download whose address names no file</a>
<li><a href="{other}/site/elsewhere.html">Another site</a>
<li><a href="mailto:lab@example.invalid">Mail</a>
</ul>
<img src="/gen/2K/photos/a.jpg" srcset="/gen/3K/photos/a@2x.jpg 2x" alt="">
<img src="{other}/gen/4K/cdn/c.png" alt="">
<div style="background-image:url('/gen/5K/bg.gif')"></div>
<script>var s = '<img src="/gen/9K/scripted.jpg">';</script>
</body></html>"#
        ),
        "style.css" => {
            return (
                [(header::CONTENT_TYPE, "text/css")],
                "@import \"more.css\";\nbody { background: url(/gen/6K/texture.png) }\n",
            )
                .into_response();
        }
        "more.css" => {
            return (
                [(header::CONTENT_TYPE, "text/css")],
                ".x { background: url(\"/gen/1K/more.webp\") }\n",
            )
                .into_response();
        }
        "gallery.html" => r#"<!doctype html><title>Gallery</title>
<a href="./">Home</a>
<img src="/gen/2K/photos/b.jpg"><a href="/gen/8K/photos/big.jpg">Full size</a>
<video src="/gen/20K/clip.mp4" poster="/gen/1K/poster.jpg"></video>
<a href="deep/level2.html">Deeper</a>"#
            .to_owned(),
        "deep/level2.html" => r#"<!doctype html><title>Level 2</title>
<img src="/gen/2K/photos/level2.jpg"><a href="level3.html">Deeper still</a>"#
            .to_owned(),
        "deep/level3.html" => r#"<!doctype html><title>Level 3</title>
<img src="/gen/2K/photos/level3.jpg"><a href="/gen/11K/song.mp3">A song</a>"#
            .to_owned(),
        "docs/" | "docs/index.html" => r#"<!doctype html><title>Documents</title>
<a href="/gen/10K/manual.pdf">Manual</a> <a href="/gen/12K/notes.txt">Notes</a>
<base href="/gen/13K/"><a href="table.xlsx">Table</a>"#
            .to_owned(),
        "private/secret.html" => r#"<!doctype html><title>Private</title>
<img src="/gen/2K/photos/secret.jpg">"#
            .to_owned(),
        "elsewhere.html" => r#"<!doctype html><title>Elsewhere</title>
<img src="/gen/2K/photos/elsewhere.jpg">"#
            .to_owned(),
        _ => return (StatusCode::NOT_FOUND, "no such page\n").into_response(),
    };
    html(body)
}
