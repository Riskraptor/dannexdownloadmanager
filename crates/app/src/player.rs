//! A loopback-only browser player for a torrent file. The browser is the media decoder; this
//! module supplies a seekable HTTP stream directly from librqbit while pieces are arriving.

use std::io::SeekFrom;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use crate::torrents::Host;

#[derive(Clone)]
struct Player {
    torrents: Arc<mdm_torrent::Torrents>,
    hash: String,
    file: usize,
    token: String,
    mime: &'static str,
    /// The page's line under the player, in the interface language (escaped when written).
    line: String,
    /// The page's title: the file's name.
    title: String,
}

/// `s` for HTML text and attributes.
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Whether a running torrent has a browser-playable media file.
pub fn has_playable_file(host: &Host, hash: &str) -> bool {
    host.get().and_then(|t| playable_file(&t, hash)).is_some()
}

/// The first chosen media file, or the first media file when all files are selected.
pub fn playable_file(torrents: &mdm_torrent::Torrents, hash: &str) -> Option<(usize, String)> {
    let (files, chosen, _) = torrents.files(hash)?;
    files
        .iter()
        .enumerate()
        .filter(|(index, _)| chosen.as_ref().is_none_or(|chosen| chosen.contains(index)))
        .find_map(|(index, file)| {
            (!file.padding && mime(&file.path.to_string_lossy()).is_some())
                .then(|| (index, file.path.to_string_lossy().into_owned()))
        })
}

/// Starts a random-token, loopback-only server and opens its player page in the default browser.
pub fn open(
    rt: &tokio::runtime::Handle,
    torrents: Arc<mdm_torrent::Torrents>,
    hash: String,
    file: usize,
    file_name: String,
) -> Result<(), String> {
    let mime = mime(&file_name).ok_or_else(|| "this torrent file is not playable".to_owned())?;
    let (listener, address) =
        loopback_listener(rt).map_err(|e| format!("could not start the player: {e}"))?;
    let token = hex::encode(rand::random::<[u8; 24]>());
    let title = std::path::Path::new(&file_name)
        .file_name()
        .map_or_else(|| file_name.clone(), |n| n.to_string_lossy().into_owned());
    let state = Player {
        torrents,
        hash,
        file,
        token: token.clone(),
        mime,
        // i18n: under the player of a torrent's video or sound (Right-click → Torrent → Play while downloading), in the browser
        line: crate::i18n::t(
            "Dannex Download Manager downloads this file in order while you watch or listen.",
        )
        .to_owned(),
        title,
    };
    let app = Router::new()
        .route("/player/{token}", get(page))
        .route("/media/{token}", get(media))
        .with_state(state);
    rt.spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            tracing::warn!("torrent player stopped: {e}");
        }
    });
    open::that_detached(format!("http://{address}/player/{token}"))
        .map_err(|e| format!("could not open the player: {e}"))
}

/// A listener on 127.0.0.1 at a free port, registered with `rt`'s reactor. Called from the
/// window's thread, which runs outside the runtime: Tokio's listener can only be made inside its
/// context (outside it, `from_std` panics: "there is no reactor running").
fn loopback_listener(
    rt: &tokio::runtime::Handle,
) -> std::io::Result<(tokio::net::TcpListener, SocketAddr)> {
    let listener =
        std::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))?;
    listener.set_nonblocking(true)?;
    let address = listener.local_addr()?;
    let _in_runtime = rt.enter();
    Ok((tokio::net::TcpListener::from_std(listener)?, address))
}

async fn page(State(player): State<Player>, Path(token): Path<String>) -> Response {
    if token != player.token {
        return StatusCode::NOT_FOUND.into_response();
    }
    let tag = if player.mime.starts_with("audio/") {
        "audio"
    } else {
        "video"
    };
    Html(format!(
        "<!doctype html><meta charset=utf-8><meta name=viewport content='width=device-width,initial-scale=1'><title>{}</title><style>body{{margin:0;background:#101216;color:#eee;font:16px system-ui;display:grid;place-items:center;min-height:100vh}}main{{width:min(96vw,1000px)}}video,audio{{width:100%;max-height:90vh}}p{{color:#b9c0ca}}</style><main><{tag} controls autoplay src='/media/{}'></{tag}><p>{}</p></main>",
        escape(&player.title),
        player.token,
        escape(&player.line)
    ))
    .into_response()
}

async fn media(
    State(player): State<Player>,
    Path(token): Path<String>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    if token != player.token {
        return Err(StatusCode::NOT_FOUND);
    }
    let mut stream = player
        .torrents
        .stream(&player.hash, player.file)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let length = stream
        .seek(SeekFrom::End(0))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let (start, end) =
        range(headers.get(header::RANGE), length).ok_or(StatusCode::RANGE_NOT_SATISFIABLE)?;
    stream
        .seek(SeekFrom::Start(start))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let partial = start != 0 || end != length;
    let mut response = Response::new(Body::from_stream(ReaderStream::with_capacity(
        stream.take(end - start),
        65_536,
    )));
    *response.status_mut() = if partial {
        StatusCode::PARTIAL_CONTENT
    } else {
        StatusCode::OK
    };
    let h = response.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(player.mime));
    h.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    h.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&(end - start).to_string())
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    );
    if partial {
        h.insert(
            header::CONTENT_RANGE,
            HeaderValue::from_str(&format!("bytes {start}-{}/{}", end - 1, length))
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        );
    }
    Ok(response)
}

/// The bytes asked for, `start..end` (end exclusive), within the file (RFC 9110 §14.1.2): a last
/// byte past the end is the end; `bytes=-N` is the last N bytes. None: nothing of it can be sent.
fn range(value: Option<&HeaderValue>, length: u64) -> Option<(u64, u64)> {
    let Some(value) = value else {
        return Some((0, length));
    };
    let value = value.to_str().ok()?.trim().strip_prefix("bytes=")?;
    // One range only (a player asks for one); several would need a multipart answer.
    let (start, end) = value.split_once('-')?;
    let (start, end) = if start.trim().is_empty() {
        let suffix = end.trim().parse::<u64>().ok()?;
        (length.saturating_sub(suffix), length)
    } else {
        let start = start.trim().parse::<u64>().ok()?;
        let end = if end.trim().is_empty() {
            length
        } else {
            end.trim()
                .parse::<u64>()
                .ok()?
                .saturating_add(1)
                .min(length)
        };
        (start, end)
    };
    (start < end).then_some((start, end))
}

fn mime(name: &str) -> Option<&'static str> {
    let ext = std::path::Path::new(name)
        .extension()?
        .to_str()?
        .to_ascii_lowercase();
    Some(match ext.as_str() {
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "ogv" => "video/ogg",
        "mp3" => "audio/mpeg",
        "m4a" | "aac" => "audio/mp4",
        "ogg" | "opus" => "audio/ogg",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::range;
    use axum::http::{HeaderValue, header};

    #[test]
    fn ranges_are_checked_and_made_exclusive() {
        assert_eq!(range(None, 100), Some((0, 100)));
        assert_eq!(
            range(Some(&HeaderValue::from_static("bytes=4-9")), 100),
            Some((4, 10))
        );
        assert_eq!(
            range(Some(&HeaderValue::from_static("bytes=4-")), 100),
            Some((4, 100))
        );
        assert_eq!(
            range(Some(&HeaderValue::from_static("bytes=100-")), 100),
            None
        );
        // A last byte past the end is the end; the last N bytes.
        assert_eq!(
            range(Some(&HeaderValue::from_static("bytes=90-5000")), 100),
            Some((90, 100))
        );
        assert_eq!(
            range(Some(&HeaderValue::from_static("bytes=-10")), 100),
            Some((90, 100))
        );
        assert_eq!(
            range(Some(&HeaderValue::from_static("bytes=-500")), 100),
            Some((0, 100))
        );
        assert_eq!(
            range(Some(&HeaderValue::from_static("bytes=-0")), 100),
            None
        );
        assert_eq!(
            range(Some(&HeaderValue::from_static("items=0-5")), 100),
            None
        );
        assert_eq!(super::escape("<a&'\">"), "&lt;a&amp;&#39;&quot;&gt;");
        let _ = header::RANGE;
    }

    /// The player's listener is made from the window's thread, outside the runtime (2.0.0
    /// panicked there: "there is no reactor running"), and answers on 127.0.0.1.
    #[test]
    fn the_player_listens_when_started_from_the_window_thread() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let (listener, address) = super::loopback_listener(rt.handle()).unwrap();
        assert!(address.ip().is_loopback() && address.port() != 0);
        let accepted = rt.spawn(async move { listener.accept().await.map(|(_, from)| from) });
        let _client = std::net::TcpStream::connect(address).unwrap();
        let from = rt.block_on(accepted).unwrap().unwrap();
        assert!(from.ip().is_loopback());
    }
}
