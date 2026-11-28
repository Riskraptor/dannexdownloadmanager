//! Mirrors (Metalink sources): the addresses taken in turn, each asked without the main address's
//! validators, and the answers that show another file.

use super::*;

/// The address ranges are fetched from, with its own client and headers. `generation` counts
/// signed-link refreshes of the primary address.
#[derive(Clone)]
pub(super) struct Target {
    pub(super) client: Client,
    pub(super) url: Url,
    pub(super) headers: HeaderMap,
    /// A POST download's body for this address (None: GET; see `post_for`).
    pub(super) post: Option<Bytes>,
    pub(super) generation: u64,
}

/// Makes the independent request targets for a verified Metalink's alternate locations.
///
/// Each location gets a client built for that location, since its port, proxy path, DNS answer
/// and adapter route may differ from the primary.  Mirrors do not inherit cookies or HTTP
/// authentication across origins (`scoped_headers`).  An unsigned mirror list deliberately
/// stays inactive: a size and an ETag alone are not a content-integrity guarantee.
pub(super) fn mirror_targets(
    opts: &Options,
    verified: bool,
    origin: &Url,
    primary: &Url,
    headers: &HeaderMap,
    as_get: &AsGet,
    seen: &Arc<crate::server::ServerSeen>,
) -> Vec<Target> {
    if !verified || opts.post.is_some() {
        return Vec::new();
    }
    let mut known = HashSet::new();
    opts.mirrors
        .iter()
        .filter(|url| matches!(url.scheme(), "http" | "https"))
        .filter(|url| **url != *origin && **url != *primary)
        .filter(|url| known.insert(url.to_string()))
        .filter_map(|url| {
            let headers = scoped_headers(headers, origin, url);
            client(opts, url, as_get, seen).ok().map(|client| Target {
                client,
                url: url.clone(),
                headers,
                post: None,
                generation: 0,
            })
        })
        .collect()
}

/// `GET` bytes `[from, to)` of Metalink mirror `i`. Its copy is checked by the file's hashes,
/// so no `If-Range` goes with it (another server's dates and ETags are its own). Anything but
/// that range of a file of this size (a refusal, no answer, a whole file, a web page) leaves
/// the mirror out (`Ctx::drop_mirror`): it never changes what the download knows of the main
/// address (its ranges, its validators) nor stops it.
pub(super) async fn mirror_request(
    ctx: &Ctx,
    target: &Target,
    i: usize,
    from: u64,
    to: u64,
) -> Result<Body, Error> {
    let req = build(
        &target.client,
        target.url.clone(),
        None,
        &target.headers,
        ctx.opts.site_agent(),
    )
    .header(header::RANGE, format!("bytes={from}-{}", to - 1));
    let resp = match cancellable(&ctx.cancel, async { Ok(req.send().await?) }).await {
        Ok(resp) => resp,
        Err(Error::Cancelled) => return Err(Error::Cancelled),
        Err(e) => {
            ctx.drop_mirror(i, &e.to_string());
            return Err(Error::MirrorDropped);
        }
    };
    let why = if resp.status() != StatusCode::PARTIAL_CONTENT {
        format!("it answered {}", resp.status())
    } else if !ctx.web_page_ok && is_web_page(resp.headers()) {
        "it sent a web page".to_owned()
    } else {
        match parse_content_range(resp.headers()) {
            Ok((start, _, total)) if start == from && total.is_none_or(|t| t == ctx.size) => {
                return Ok(Body::Http(resp));
            }
            _ => "it sent another part or a file of another size".to_owned(),
        }
    };
    ctx.drop_mirror(i, &why);
    Err(Error::MirrorDropped)
}

/// A full answer that still describes the file being downloaded: the same length and, when the
/// download has validators, the same one it would send.
pub(super) fn same_file(ctx: &Ctx, resp: &Head) -> bool {
    let host = resp.url.host_str();
    let v = ctx.validators.lock().unwrap();
    let h = &resp.headers;
    let theirs = match v.if_range(host) {
        None => return resp.len == Some(ctx.size),
        Some(ours) if Some(ours) == v.etag.as_deref() => header_str(h, header::ETAG),
        Some(_) => header_str(h, header::LAST_MODIFIED),
    };
    resp.len == Some(ctx.size) && theirs.is_some() && theirs == v.if_range(host)
}

/// A full answer to an ETag `If-Range` that still matches by length and Last-Modified: the
/// ETag differs per node (CDNs, mirrors), not the file. The ETag is dropped for the rest of the
/// download, so the next request validates with the date (and the overlap check).
pub(super) fn per_node_etag(ctx: &Ctx, sent: Option<&HeaderValue>, resp: &Head) -> bool {
    let mut v = ctx.validators.lock().unwrap();
    // Sent with an ETag (dates never start with a quote) — possibly one another connection has
    // just dropped, so don't compare it with the current one.
    let sent_etag = sent.is_some_and(|s| s.as_bytes().starts_with(b"\""));
    // Another node's ETag differs from the one sent. The same ETag is that too only from a
    // server that advertised ranges: one that didn't, answering in full, has no ranges (#136),
    // and is told so at once rather than after a round of retries.
    let theirs = header_str(&resp.headers, header::ETAG);
    let other_node = ctx.advertised
        || (theirs.is_some() && theirs.map(str::as_bytes) != sent.map(HeaderValue::as_bytes));
    let same = sent_etag
        && other_node
        && resp.len == Some(ctx.size)
        && v.last_modified.is_some()
        && header_str(&resp.headers, header::LAST_MODIFIED) == v.last_modified.as_deref();
    if same {
        v.etag = None;
        v.etag_host = None;
    }
    same
}
