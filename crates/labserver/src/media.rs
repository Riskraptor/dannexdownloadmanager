//! Real, playable media for the browser extension's tests (tools/extension/e2e.mjs): a tone (MP3),
//! a test picture with a tone (MP4, WebM), made by `assets/media/make.ps1` from ffmpeg's own test
//! sources and built into the server. `/media/NAME` serves one with its type, in ranges; its query
//! makes it look as a site's does: another type or none, a radio station's endless stream, a
//! larger MP4 (a `free` box after its data).

use std::collections::HashMap;
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Path, RawQuery};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;

use crate::content::parse_size;

const TONE_MP3: &[u8] = include_bytes!("../assets/media/tone.mp3");
const CLIP_MP4: &[u8] = include_bytes!("../assets/media/clip.mp4");
const CLIP_WEBM: &[u8] = include_bytes!("../assets/media/clip.webm");

/// A sample by its name: its bytes and its own type.
fn sample(name: &str) -> Option<(&'static [u8], &'static str)> {
    match name {
        "tone.mp3" => Some((TONE_MP3, "audio/mpeg")),
        "clip.mp4" => Some((CLIP_MP4, "video/mp4")),
        "clip.webm" => Some((CLIP_WEBM, "video/webm")),
        _ => None,
    }
}

/// The most a `pad` may add (a player fetching a file of a few MB in ranges, as TikTok's).
const PAD_MAX: u64 = 64 << 20;
/// A radio stream's pace: about twice the tone's own rate, so a test needn't wait for it.
const RADIO_CHUNK: usize = 4096;
const RADIO_EVERY: Duration = Duration::from_millis(1000);

/// `/media/{*path}?file=NAME&type=TYPE|none&pad=SIZE&icy=NAME`: the sample named by the path's last
/// part (tone.mp3, clip.mp4, clip.webm), else by `file` (an address naming no file:
/// `/media/play/42?file=tone.mp3`). `type`: that Content-Type instead (`none`: no header at all);
/// `pad` (an MP4): that many bytes more, a `free` box at its end; `icy`: a radio station's stream
/// (Icecast's): the tone again and again, endless, no length, `icy-name: NAME`.
pub(crate) async fn media(
    Path(path): Path<String>,
    RawQuery(q): RawQuery,
    headers: HeaderMap,
) -> Response {
    let m: HashMap<String, String> = q
        .and_then(|q| serde_urlencoded::from_str(&q).ok())
        .unwrap_or_default();
    let last = path.rsplit('/').next().unwrap_or_default();
    let Some((bytes, own_type)) = sample(last).or_else(|| m.get("file").and_then(|f| sample(f)))
    else {
        return (
            StatusCode::NOT_FOUND,
            "no such sample (tone.mp3, clip.mp4, clip.webm)\n",
        )
            .into_response();
    };
    let ty = match m.get("type").map(String::as_str) {
        Some("none") => None,
        Some(t) if !t.is_empty() => Some(t.to_owned()),
        _ => Some(own_type.to_owned()),
    };
    if let Some(station) = m.get("icy") {
        return radio(bytes, ty, station);
    }
    let pad = m
        .get("pad")
        .and_then(|p| parse_size(p))
        .filter(|_| own_type == "video/mp4")
        .map(|p| p.clamp(8, PAD_MAX));
    let data: Bytes = match pad {
        Some(n) => {
            let mut v = Vec::with_capacity(bytes.len() + n as usize);
            v.extend_from_slice(bytes);
            v.extend_from_slice(&(n as u32).to_be_bytes());
            v.extend_from_slice(b"free");
            v.resize(bytes.len() + n as usize, 0);
            Bytes::from(v)
        }
        None => Bytes::from_static(bytes),
    };
    let size = data.len() as u64;
    let range = headers
        .get(header::RANGE)
        .and_then(|r| r.to_str().ok())
        .and_then(|r| http_range_header::parse_range_header(r).ok())
        .and_then(|r| r.validate(size).ok())
        .filter(|v| v.len() == 1)
        .map(|v| (*v[0].start(), *v[0].end() + 1));
    let (start, end, status) = match range {
        Some((s, e)) => (s, e, StatusCode::PARTIAL_CONTENT),
        None => (0, size, StatusCode::OK),
    };
    let mut builder = Response::builder()
        .status(status)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, end - start);
    if let Some(ty) = ty {
        builder = builder.header(header::CONTENT_TYPE, ty);
    }
    if status == StatusCode::PARTIAL_CONTENT {
        builder = builder.header(
            header::CONTENT_RANGE,
            format!("bytes {start}-{}/{size}", end - 1),
        );
    }
    builder
        .body(Body::from(data.slice(start as usize..end as usize)))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// A radio station's stream: `bytes` over and over, RADIO_CHUNK at a time, until the client goes.
fn radio(bytes: &'static [u8], ty: Option<String>, station: &str) -> Response {
    let stream = futures::stream::unfold(0usize, move |at| async move {
        if at > 0 {
            tokio::time::sleep(RADIO_EVERY).await;
        }
        let from = at % bytes.len();
        let to = (from + RADIO_CHUNK).min(bytes.len());
        let chunk = Bytes::from_static(&bytes[from..to]);
        Some((Ok::<_, std::io::Error>(chunk), at + (to - from)))
    });
    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header("icy-br", "16")
        .header(header::CACHE_CONTROL, "no-cache, no-store");
    if let Ok(name) = header::HeaderValue::from_str(station) {
        builder = builder.header("icy-name", name);
    }
    if let Some(ty) = ty {
        builder = builder.header(header::CONTENT_TYPE, ty);
    }
    builder
        .body(Body::from_stream(stream))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_are_what_they_say() {
        let (mp3, ty) = sample("tone.mp3").unwrap();
        assert_eq!(ty, "audio/mpeg");
        // An MPEG audio frame's sync (no ID3 tag: make.ps1 writes none).
        assert!(mp3[0] == 0xff && mp3[1] & 0xe0 == 0xe0);
        let (mp4, _) = sample("clip.mp4").unwrap();
        // ftyp first, then moov (its header first: faststart).
        assert_eq!(&mp4[4..8], b"ftyp");
        let ftyp = u32::from_be_bytes(mp4[0..4].try_into().unwrap()) as usize;
        assert_eq!(&mp4[ftyp + 4..ftyp + 8], b"moov");
        let (webm, _) = sample("clip.webm").unwrap();
        assert_eq!(&webm[0..4], &[0x1a, 0x45, 0xdf, 0xa3]);
        assert!(sample("other.mp4").is_none());
    }
}
