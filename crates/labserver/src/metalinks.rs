//! Metalink documents describing /gen files (Metalink 4, RFC 5854, or metalinker.org's 3.0),
//! for the engine's and the app's Metalink tests: several files, each with its sources (each
//! with faults of its own), their priorities and countries, whole-file and piece hashes. Never
//! real ones.

use axum::extract::{Path, RawQuery};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use sha2::Digest as _;

use crate::content;

/// The largest file a document may describe: its hashes are worked out for each request.
const MAX_FILE: u64 = 64 << 20;
/// The most sources one file may have.
const MAX_SOURCES: usize = 8;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// SHA-256 of file `seed`'s `[from, to)`.
fn sha256(seed: u64, from: u64, to: u64) -> String {
    let mut h = sha2::Sha256::new();
    let mut buf = vec![0u8; 64 << 10];
    let mut at = from;
    while at < to {
        let n = (to - at).min(buf.len() as u64) as usize;
        content::fill(seed, at, &mut buf[..n]);
        h.update(&buf[..n]);
        at += n as u64;
    }
    hex::encode(h.finalize())
}

/// One file a document describes: its size, its name (folders allowed) and its content's seed.
struct Described {
    size: u64,
    name: String,
    seed: u64,
}

/// `GET /metalink/{name}`: a Metalink document (3.0 when `name` ends in `.metalink` or `v=3`,
/// else 4) for the files the query names; see `HELP`.
pub(crate) async fn document(Path(name): Path<String>, RawQuery(q): RawQuery) -> Response {
    let pairs: Vec<(String, String)> = q
        .and_then(|q| serde_urlencoded::from_str(&q).ok())
        .unwrap_or_default();
    let get = |k: &str| pairs.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
    let v3 = name.to_ascii_lowercase().ends_with(".metalink") || get("v") == Some("3");
    let seed: u64 = get("seed").and_then(|s| s.parse().ok()).unwrap_or(1);
    let sources = get("sources")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(1)
        .clamp(1, MAX_SOURCES);
    let hashes = get("hash") != Some("0");
    let pieces = get("pieces")
        .and_then(content::parse_size)
        .filter(|p| *p > 0);
    let wrong: Option<usize> = get("wrong").and_then(|s| s.parse().ok());
    let mut files = Vec::new();
    for (i, spec) in get("files")
        .unwrap_or("64K/one.bin,256K/docs/two.bin")
        .split(',')
        .enumerate()
    {
        let Some((size, file)) = spec.split_once('/') else {
            return (StatusCode::BAD_REQUEST, format!("bad file `{spec}`\n")).into_response();
        };
        let Some(size) = content::parse_size(size).filter(|s| *s > 0 && *s <= MAX_FILE) else {
            return (StatusCode::BAD_REQUEST, format!("bad size in `{spec}`\n")).into_response();
        };
        files.push(Described {
            size,
            name: file.to_owned(),
            seed: seed + i as u64,
        });
    }
    let mut doc = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    if v3 {
        doc.push_str("<metalink version=\"3.0\" xmlns=\"http://www.metalinker.org/\">\n<files>\n");
    } else {
        doc.push_str("<metalink xmlns=\"urn:ietf:params:xml:ns:metalink\">\n");
    }
    for (i, f) in files.iter().enumerate() {
        doc.push_str(&format!(
            "<file name=\"{}\">\n<size>{}</size>\n",
            esc(&f.name),
            f.size
        ));
        let mut whole = sha256(f.seed, 0, f.size);
        if wrong == Some(i + 1) {
            whole = "0".repeat(64);
        }
        let piece_list = pieces.map(|len| {
            let mut s = format!("<pieces length=\"{len}\" type=\"sha-256\">\n");
            let mut at = 0;
            let mut k = 0;
            while at < f.size {
                let to = (at + len).min(f.size);
                let digest = sha256(f.seed, at, to);
                if v3 {
                    s.push_str(&format!("<hash piece=\"{k}\">{digest}</hash>\n"));
                } else {
                    s.push_str(&format!("<hash>{digest}</hash>\n"));
                }
                at = to;
                k += 1;
            }
            s.push_str("</pieces>\n");
            s
        });
        if v3 {
            doc.push_str("<verification>\n");
            if hashes {
                doc.push_str(&format!("<hash type=\"sha256\">{whole}</hash>\n"));
            }
            doc.push_str(piece_list.as_deref().unwrap_or(""));
            doc.push_str("</verification>\n<resources>\n");
        } else {
            if hashes {
                doc.push_str(&format!("<hash type=\"sha-256\">{whole}</hash>\n"));
            }
            doc.push_str(piece_list.as_deref().unwrap_or(""));
        }
        for k in 1..=sources {
            let mut url = format!("/gen/{}/{}?seed={}", f.size, f.name, f.seed);
            if let Some(faults) = get(&format!("fault{k}")).filter(|s| !s.is_empty()) {
                url.push('&');
                url.push_str(faults);
            }
            let location = get(&format!("loc{k}"))
                .map(|l| format!(" location=\"{}\"", esc(l)))
                .unwrap_or_default();
            if v3 {
                doc.push_str(&format!(
                    "<url type=\"http\" preference=\"{}\"{location}>{}</url>\n",
                    100 - k,
                    esc(&url)
                ));
            } else {
                doc.push_str(&format!(
                    "<url priority=\"{k}\"{location}>{}</url>\n",
                    esc(&url)
                ));
            }
        }
        if v3 {
            doc.push_str("</resources>\n");
        }
        doc.push_str("</file>\n");
    }
    doc.push_str(if v3 {
        "</files>\n</metalink>\n"
    } else {
        "</metalink>\n"
    });
    let kind = if v3 {
        "application/metalink+xml"
    } else {
        "application/metalink4+xml"
    };
    ([(header::CONTENT_TYPE, kind)], doc).into_response()
}
