//! One connection's work (`worker`): its requests for a segment, what it receives and writes at its
//! offset, the speed limit, stalls and shedding, and the writes and reads at a file's offset.

use super::*;

/// Bytes buffered per connection before claiming and writing them.
pub(super) const BATCH: usize = 256 * 1024;

/// Received bytes are written at least this often, so a slow connection shows progress and a
/// pause keeps what arrived.
pub(super) const FLUSH_EVERY: Duration = Duration::from_secs(1);

/// While the resume save flushes the part file to the disk (`Shared::flushing`), the connections
/// keep what they receive in memory, at most this much for the whole download, instead of
/// waiting on the disk (#361): on NTFS a file's writes wait for its flush, whatever their
/// handle (measured: 8 connections' writes held for each whole flush, 3-10 s on a slow disk).
pub(super) const FLUSH_HOLD_MAX: usize = 256 << 20;

/// Shedding a slow connection (`Options::shed_slow`): after `SHED_GRACE` on a response, a
/// connection under 1/`SHED_RATIO` of the best speed one connection reached, with more than
/// `SHED_LEFT` of work left at its own pace, reconnects. A slow TCP path (or a slow server
/// slot) rarely survives a new request, and otherwise every free connection can end up stuck
/// on one (measured: 14 s instead of ~2 s for 32 MiB when every third request was slow).
pub(super) const SHED_GRACE: Duration = Duration::from_secs(2);

pub(super) const SHED_RATIO: u64 = 4;

pub(super) const SHED_LEFT: Duration = Duration::from_secs(2);

/// A stalled connection (`stalled`): silent this long (or four times the quickest range
/// answer, when that is longer) while the download's other connections still receive. It
/// reconnects then instead of waiting out `Options::read_timeout` (12 s) with its part.
pub(super) const STALL_AFTER: Duration = Duration::from_secs(3);

/// How often a connection waiting for data looks whether it is stalled.
pub(super) const STALL_TICK: Duration = Duration::from_millis(500);

/// The line still works: the download's other connections got this many bytes after the silent
/// one's last (more than a line that stops for all of them leaves in flight).
pub(super) const OTHERS_GOT: u64 = 1 << 20;

/// A download starts with one connection per this many bytes (at most `max_connections`).
pub(super) const INITIAL_PIECE: u64 = 64 * 1024;

/// A file up to this size starts on its first connection alone, and the others open once its
/// pace says they would bring something (`ramp`): a new connection costs its time to first byte,
/// in which the first one may already have finished. A bigger file opens them all at once: its
/// connections have long enough to pay for their start, and waiting to look only delays them.
pub(super) const RAMP_SIZE: u64 = 16 << 20;

/// When the first connection's pace is looked at (`ramp`): half its time to first byte, within
/// these bounds (a short look underestimates the pace, which only opens more connections).
pub(super) const RAMP_LOOK_MIN: Duration = Duration::from_millis(50);

pub(super) const RAMP_LOOK_MAX: Duration = Duration::from_millis(400);

/// A pending connection is taken over after this many first-byte times of the probe (and never
/// sooner than PREEMPT_MIN).
pub(super) const PREEMPT_TTFB: u32 = 2;

pub(super) const PREEMPT_MIN: Duration = Duration::from_millis(20);

/// How one answer's body ended in [`receive`].
pub(super) enum Received {
    /// The segment is complete, at this end.
    Complete(u64),
    /// Another connection took the segment over.
    Lost,
    /// All of a capped request (`request_cap`) came, up to here; the segment goes on.
    More(u64),
}

/// One connection: take an assignment, fetch it, repeat until there is nothing left to do.
pub(super) async fn worker(
    ctx: Arc<Ctx>,
    id: WorkerId,
    mut first: Option<Body>,
) -> Result<(), Error> {
    let mut after = None;
    let mut failures = 0u32;
    // Bytes this connection had received at its last failure: progress since resets the budget.
    let mut at_last_failure = 0u64;
    loop {
        if ctx.cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let floor = split_floor(&ctx);
        let Some(a) = ctx.shared.with_planner(|p| {
            p.set_split_floor(floor);
            p.assign(id, after.take())
        }) else {
            ctx.shared.conn(id, |c| c.status = ConnStatus::Finished);
            return Ok(());
        };
        ctx.shared.conn(id, |c| {
            c.status = ConnStatus::Connecting;
            c.speed = 0;
        });
        let target = ctx.target();

        // The server ignores ranges: only the probe's plain GET can bring data. While it
        // still streams it takes this segment over as it gets there; once it has stopped,
        // the download starts over on one connection.
        if first.is_none() && ctx.ranges.load(Ordering::SeqCst) == RANGES_NO {
            ctx.shared.with_planner(|p| p.release(id));
            return if ctx.shared.with_planner(|p| p.holds(0)) {
                ctx.shared.conn(id, |c| c.status = ConnStatus::Finished);
                Ok(())
            } else {
                Err(Error::NoRanges)
            };
        }
        let whole = first.is_some();
        // Re-fetch the last few bytes already on disk and compare them before going on.
        let overlap = if first.is_some() {
            0
        } else {
            a.written.min(ctx.opts.verify_overlap)
        };
        let result = match first.take() {
            Some(body) if a.from == 0 => {
                let to = request_end(&ctx, id, 0).unwrap_or(a.to);
                receive_all(&ctx, id, &target, Some(body), 0, to, 0, whole).await
            }
            Some(_) => Err(Error::BadRange(
                "probe segment no longer starts at 0".into(),
            )),
            None => {
                let to = request_end(&ctx, id, a.from).unwrap_or(a.to);
                receive_all(&ctx, id, &target, None, a.from, to, overlap, false).await
            }
        };
        match result {
            Ok(Some(end)) => {
                failures = 0;
                after = Some(end);
            }
            Ok(None) => {
                // Preempted (another connection took the segment over): ask again.
                ctx.shared.with_planner(|p| p.release(id));
            }
            Err(Error::NoRanges) => {
                // The server ignores ranges: connection 0 (the probe) keeps the whole file if it
                // can, every other connection stops. If it can't have it all at once (it was
                // split), it still takes each segment over as it reaches it, while it streams.
                ctx.shared.with_planner(|p| p.release(id));
                return if ctx.shared.with_planner(|p| p.make_single(0) || p.holds(0)) {
                    ctx.shared.conn(id, |c| c.status = ConnStatus::Finished);
                    Ok(())
                } else {
                    Err(Error::NoRanges)
                };
            }
            Err(e) => {
                ctx.shared.with_planner(|p| p.release(id));
                if e.is_fatal() {
                    return Err(e);
                }
                // Shed: reconnect at once for the rest of the segment (not a failure), on a new
                // connection (`Lanes::renew`: over HTTP/2 the lane's would carry the next request).
                if matches!(e, Error::SlowConnection) {
                    ctx.lanes.renew(id);
                    continue;
                }
                // A mirror left out (`Ctx::drop_mirror` said why): the next source, at once.
                if matches!(e, Error::MirrorDropped) {
                    continue;
                }
                let (working, receiving) = ctx
                    .shared
                    .with_planner(|p| (p.active_workers(), p.receiving_workers()));
                let others = working > 0;
                // An extra connection refused while others work: the host's connection limit.
                // Once learned (twice seen), this connection retires instead of retrying — if the
                // others fill the cap. Below it the server most likely still counts a connection
                // of ours that just closed (a shed or a split reconnects at once): retry as usual
                // (benchmark 2026-10-29: retiring here left 50 MiB on one connection for 12 s).
                // Only while others are receiving data (`HostLimits::refused`): with none, the
                // whole server or the network is gone (#4: a server restarting refused every
                // connection, and taught a cap of 1 the download then resumed with).
                if e.refused() && others && receiving > 0 {
                    let host = ctx.target().url.host_str().unwrap_or("").to_owned();
                    if let Some(cap) = ctx.host_limits().refused(&host, receiving as u32)
                        && working as u32 >= cap
                    {
                        ctx.shared.event(format!(
                            "Connection {}: {host} allows {cap} connection(s) (learned)",
                            id + 1
                        ));
                        ctx.shared
                            .conn(id, |c| c.status = ConnStatus::Retired { host, cap });
                        return Ok(());
                    }
                }
                // A dead or forbidden link is not retried. While other connections work it
                // means this extra one is refused (a per-client connection limit): retire it.
                if e.link_dead() {
                    // A redirected address that expired (signed CDN links): go where the
                    // original address leads now, if that is still this file.
                    if !matches!(e, Error::LinkExpired)
                        && ctx.refresh_target(target.generation).await
                    {
                        ctx.shared.conn(id, |c| c.status = ConnStatus::Connecting);
                        continue;
                    }
                    let e = match e {
                        Error::Status(_) if ctx.origin.is_some() => Error::LinkExpired,
                        e => e,
                    };
                    ctx.shared
                        .conn(id, |c| c.status = ConnStatus::Failed(e.clone()));
                    return if others { Ok(()) } else { Err(e) };
                }
                let mut received = 0;
                ctx.shared.conn(id, |c| received = c.downloaded);
                if received > at_last_failure {
                    failures = 0;
                }
                at_last_failure = received;
                failures += 1;
                if failures > ctx.opts.max_retries {
                    let others = ctx.shared.with_planner(|p| p.active_workers()) > 0;
                    ctx.shared
                        .conn(id, |c| c.status = ConnStatus::Failed(e.clone()));
                    return if others { Ok(()) } else { Err(e) };
                }
                ctx.shared
                    .event(format!("Connection {}: {e}; retrying", id + 1));
                ctx.shared
                    .conn(id, |c| c.status = ConnStatus::Retrying(e.clone()));
                if let Some(delay) = retry_wait(&ctx.opts, failures, &e) {
                    cancellable(&ctx.cancel, async {
                        tokio::time::sleep(delay).await;
                        Ok(())
                    })
                    .await?;
                }
            }
        }
    }
}

/// `Range: bytes=<from>-<from + len - 1>`.
pub(super) fn range_value(from: u64, len: u64) -> HeaderValue {
    HeaderValue::from_str(&format!("bytes={from}-{}", from + len - 1)).expect("ascii")
}

/// Where connection `id`'s next request starting at `from` ends: its segment's end, or sooner
/// under `request_cap` (HTTP). None: the segment is no longer its own.
pub(super) fn request_end(ctx: &Ctx, id: WorkerId, from: u64) -> Option<u64> {
    let end = from + ctx.shared.with_planner(|p| p.room(id, from))?;
    Some(match ctx.opts.cap().filter(|_| ctx.ftp.is_none()) {
        Some(cap) => end.min(from.saturating_add(cap)),
        None => end,
    })
}

/// Connection `id`'s part from `from`: `body` (the first answer) or a request for `[from, to)`;
/// then, under `request_cap`, one request after another on the same connection until its
/// segment is complete. As [`receive`]: the segment's end, or None when it was taken over.
#[allow(clippy::too_many_arguments)]
pub(super) async fn receive_all(
    ctx: &Ctx,
    id: WorkerId,
    target: &Target,
    body: Option<Body>,
    from: u64,
    to: u64,
    overlap: u64,
    whole: bool,
) -> Result<Option<u64>, Error> {
    let (mut from, mut to, mut overlap, mut whole) = (from, to, overlap, whole);
    let mut body = body;
    loop {
        let b = match body.take() {
            Some(b) => {
                ctx.note_served(from, to, None);
                b
            }
            None => {
                let (source, mirror) = ctx.source(target);
                // A mirror's bytes are checked by the file's hashes, not against the disk: its
                // copy may be another server's, byte for byte the same file.
                if mirror.is_some() {
                    overlap = 0;
                }
                // The bytes re-read to check the resume (`overlap`) count in the cap too.
                if let Some(cap) = ctx.opts.cap().filter(|c| overlap < *c && ctx.ftp.is_none()) {
                    to = to.min(from - overlap + cap);
                }
                ctx.note_served(from - overlap, to, mirror);
                request(ctx, id, &source, mirror, from - overlap, to).await?
            }
        };
        match receive(ctx, id, b, from, to, overlap, whole).await? {
            Received::Complete(end) => return Ok(Some(end)),
            Received::Lost => return Ok(None),
            // The capped request came whole, its segment goes on: the next piece of it.
            Received::More(pos) => {
                let Some(end) = request_end(ctx, id, pos) else {
                    return Ok(None);
                };
                (from, to, overlap, whole) = (pos, end, 0, false);
            }
        }
    }
}

/// `GET` bytes `[from, to)` of `target` on connection `id` (`mirror`: which Metalink mirror
/// it is, None the main address, asked on the connection's own client, `Lanes`); checks the
/// server returned exactly that range of the same file. FTP: `REST from` and `RETR`.
pub(super) async fn request(
    ctx: &Ctx,
    id: WorkerId,
    target: &Target,
    mirror: Option<usize>,
    from: u64,
    to: u64,
) -> Result<Body, Error> {
    if let Some(i) = mirror {
        return mirror_request(ctx, target, i, from, to).await;
    }
    if let Some(site) = &ctx.ftp {
        return match cancellable(&ctx.cancel, site.open_at(from, Some(ctx.size))).await {
            Ok(t) => {
                if from > 0 {
                    ctx.learn_ranges(true);
                }
                Ok(Body::Ftp(t))
            }
            // REST refused ("not implemented", "syntax error") where it wasn't known to work.
            Err(Error::FtpReply {
                code: 500 | 501 | 502 | 504,
                ..
            }) if from > 0 && ctx.ranges.load(Ordering::SeqCst) != RANGES_YES => {
                ctx.learn_ranges(false);
                Err(Error::NoRanges)
            }
            Err(e) => Err(e),
        };
    }
    // The main address on this connection's own client (`Lanes`).
    let client = ctx.lanes.client(id);
    let if_range = ctx
        .validators
        .lock()
        .unwrap()
        .if_range(target.url.host_str())
        .and_then(|v| HeaderValue::from_str(v).ok());
    let range = format!("bytes={from}-{}", to - 1);
    let (head, body) = match &ctx.browser {
        // The browser-like connection's own (#668): its redirects followed as `get` does.
        Some(b) => {
            let mut headers = target.headers.clone();
            headers.insert(header::RANGE, HeaderValue::from_str(&range).expect("ascii"));
            if let Some(v) = &if_range {
                headers.insert(header::IF_RANGE, v.clone());
            }
            let (head, body, _) = b
                .get(target.url.clone(), headers, true, &ctx.cancel)
                .await?;
            (head, body)
        }
        None => {
            let mut req = build(
                &client,
                target.url.clone(),
                target.post.as_ref(),
                &target.headers,
                ctx.opts.site_agent(),
            )
            .header(header::RANGE, range);
            if let Some(v) = &if_range {
                req = req.header(header::IF_RANGE, v);
            }
            let asked = std::time::Instant::now();
            // A lane's client has no read timeout (`Lanes`): the answer gets as long as one would.
            let wait = ctx.opts.read_timeout;
            let resp = cancellable(&ctx.cancel, async {
                match tokio::time::timeout(wait, req.send()).await {
                    Ok(sent) => Ok(sent?),
                    Err(_) => Err(silent_error()),
                }
            })
            .await?;
            if resp.status() == StatusCode::PARTIAL_CONTENT {
                ctx.shared
                    .range_ttfb
                    .fetch_min(asked.elapsed().as_millis() as u64, Ordering::Relaxed);
            }
            (Head::of(&resp), Body::Http(resp))
        }
    };
    if head.status.is_success() && !ctx.web_page_ok && is_web_page(&head.headers) {
        return Err(Error::GotWebPage);
    }
    match head.status {
        StatusCode::PARTIAL_CONTENT => {
            ctx.learn_ranges(true);
            ctx.shared.partial_seen.store(true, Ordering::Relaxed);
            ctx.range_answered.notify_one();
        }
        // The same file under another ETag (each CDN node has its own): go on with the date.
        StatusCode::OK if per_node_etag(ctx, if_range.as_ref(), &head) => {
            return Err(Error::RangesUnavailable);
        }
        // A full body for a ranged request once ranges are known to work: If-Range failed (the
        // file changed), unless it is still the same file and this answer just lacks the range.
        StatusCode::OK if ctx.ranges.load(Ordering::SeqCst) == RANGES_YES => {
            return Err(if same_file(ctx, &head) {
                Error::RangesUnavailable
            } else {
                Error::FileChanged
            });
        }
        StatusCode::OK => {
            ctx.learn_ranges(false);
            return Err(Error::NoRanges);
        }
        s if s == StatusCode::TOO_MANY_REQUESTS || s == StatusCode::SERVICE_UNAVAILABLE => {
            return Err(Error::Throttled(s, retry_after(&head.headers)));
        }
        s => return Err(Error::Status(s)),
    }
    let (start, _, total) = parse_content_range(&head.headers)?;
    if start != from || total.is_some_and(|t| t != ctx.size) {
        return Err(Error::BadRange(format!(
            "asked {from}-{}, got {start}..",
            to - 1
        )));
    }
    Ok(body)
}

pub(super) fn header_str(h: &HeaderMap, name: header::HeaderName) -> Option<&str> {
    h.get(name).and_then(|v| v.to_str().ok())
}

/// Stream a response into the file at `from`. `Some(end)` when the segment is complete,
/// `None` when the segment was taken over by another connection.
///
/// Every chunk is claimed from the planner as soon as it arrives (not when the buffer is
/// flushed), so a split can never fall inside data this connection has already received.
pub(super) async fn receive(
    ctx: &Ctx,
    id: WorkerId,
    mut body: Body,
    from: u64,
    to: u64,
    mut verify: u64,
    whole: bool,
) -> Result<Received, Error> {
    ctx.shared.conn(id, |c| c.status = ConnStatus::Receiving);
    // Written and committed up to `pos`; `buf` holds claimed bytes not yet written.
    let mut pos = from;
    let mut buf = Vec::with_capacity(BATCH);
    let mut flushed_at = std::time::Instant::now();
    let started = std::time::Instant::now();
    // Time spent waiting for the disk (`flush`): a connection isn't slow for that (#361), it
    // wasn't reading the network then.
    let mut on_disk = Duration::ZERO;
    let mut received = 0u64;
    let mut checked = started;
    // When this connection last got a byte (or came back from the disk): a stall's clock; and
    // what the whole download had received then (`Shared::got`).
    let mut heard = started;
    let mut got_then = ctx.shared.got.load(Ordering::Relaxed);
    let watchdog = tokio::time::sleep(STALL_TICK);
    tokio::pin!(watchdog);
    loop {
        let next = {
            let chunk = body.chunk();
            tokio::pin!(chunk);
            loop {
                tokio::select! {
                    _ = ctx.cancel.cancelled() => {
                        // Paused or stopped: keep what arrived (it is claimed, so it is ours to
                        // write).
                        flush(ctx, id, &mut pos, &mut buf).await?;
                        return Err(Error::Cancelled);
                    }
                    n = &mut chunk => break n,
                    () = &mut watchdog => {
                        watchdog.as_mut().reset(tokio::time::Instant::now() + STALL_TICK);
                        let silent = heard.elapsed();
                        // The read timeout, the waits for the disk left out (`Lanes`).
                        if silent >= ctx.opts.read_timeout && ctx.lanes.timed_here() {
                            flush(ctx, id, &mut pos, &mut buf).await?;
                            return Err(silent_error());
                        }
                        if stalled(ctx, silent, got_then) {
                            flush(ctx, id, &mut pos, &mut buf).await?;
                            ctx.shared.event(format!(
                                "Connection {}: no data for {:.1} s while the others receive; \
                                 reconnecting",
                                id + 1,
                                silent.as_secs_f64()
                            ));
                            return Err(Error::SlowConnection);
                        }
                    }
                }
            }
        };
        if let Ok(Some(chunk)) = &next {
            heard = std::time::Instant::now();
            got_then = ctx
                .shared
                .got
                .fetch_add(chunk.len() as u64, Ordering::Relaxed)
                + chunk.len() as u64;
        }
        let (ended, err) = match next {
            Ok(Some(mut chunk)) => {
                // The response starts `verify` bytes before `from`: they must equal the disk.
                if verify > 0 {
                    let k = verify.min(chunk.len() as u64);
                    let (file, at) = (ctx.file.clone(), from - verify);
                    let disk = tokio::task::spawn_blocking(move || read_at(&file, at, k as usize))
                        .await
                        .expect("reader panicked")?;
                    if disk[..] != chunk[..k as usize] {
                        return Err(Error::FileChanged);
                    }
                    verify -= k;
                    chunk = chunk.slice(k as usize..);
                    if chunk.is_empty() {
                        continue;
                    }
                }
                let want = chunk.len() as u64;
                // Pay first: the wait is when other connections may split this segment, and
                // the claim below then grants only what still belongs to it. Stop ends the
                // wait (under a low limit it is seconds long; Stop used to wait it out).
                let paid = async {
                    ctx.shared.limiter.acquire(want).await;
                    for l in ctx.opts.shared_limits() {
                        l.acquire(want).await;
                    }
                };
                tokio::select! {
                    _ = ctx.cancel.cancelled() => {
                        // Not claimed yet: this chunk isn't ours to keep; what is, is written.
                        flush(ctx, id, &mut pos, &mut buf).await?;
                        return Err(Error::Cancelled);
                    }
                    _ = paid => {}
                }
                let at = pos + buf.len() as u64;
                // An answer that runs to the end of the file (`whole`: the probe's plain GET),
                // while ranges aren't known to work: at its segment's end it takes over the
                // split-off segments nothing has arrived for yet, rather than dropping bytes it
                // is already receiving for a range request the server may ignore.
                let keep_reading = whole && ctx.ranges.load(Ordering::SeqCst) != RANGES_YES;
                let claimed = ctx.shared.with_planner(|p| {
                    let Claim::Granted(mut n) = p.claim(id, at, want) else {
                        return None;
                    };
                    while keep_reading && n < want && p.absorb_next(id) {
                        match p.claim(id, at + n, want - n) {
                            Claim::Granted(m) => n += m,
                            Claim::Lost => break,
                        }
                    }
                    // Ending exactly at the boundary: take the next segment over too.
                    let full =
                        p.room(id, at + n) == Some(0) && !(keep_reading && p.absorb_next(id));
                    Some((n, full))
                });
                // Claimed bytes can't be taken over, so a lost segment has an empty buffer.
                let refund = |n: u64| {
                    ctx.shared.limiter.refund(n);
                    for l in ctx.opts.shared_limits() {
                        l.refund(n);
                    }
                };
                let Some((n, full)) = claimed else {
                    refund(want);
                    return Ok(Received::Lost);
                };
                if n < want {
                    refund(want - n);
                }
                if received == 0 && n > 0 {
                    // This connection's first data: tell the host's learned cap how many are
                    // receiving now.
                    let now = ctx.shared.with_planner(|p| p.receiving_workers()) as u32;
                    ctx.host_limits()
                        .working(ctx.target().url.host_str().unwrap_or(""), now);
                }
                buf.extend_from_slice(&chunk[..n as usize]);
                ctx.shared.held.fetch_add(n, Ordering::Relaxed);
                received += n;
                if checked.elapsed() >= Duration::from_millis(500) {
                    checked = std::time::Instant::now();
                    let on_net = started.elapsed().saturating_sub(on_disk);
                    let speed = (received as f64 / on_net.as_secs_f64()) as u64;
                    ctx.shared.conn(id, |c| c.speed = speed);
                    note_best(ctx, received, on_net);
                    if should_shed(ctx, id, speed, on_net, pos + buf.len() as u64) {
                        flush(ctx, id, &mut pos, &mut buf).await?;
                        ctx.shared.event(format!(
                            "Connection {}: {}",
                            id + 1,
                            Error::SlowConnection
                        ));
                        return Err(Error::SlowConnection);
                    }
                }
                if !full && buf.len() < hold_cap(ctx) && ctx.shared.flushing.load(Ordering::Acquire)
                {
                    continue; // the disk is busy flushing: this waits in memory (#361)
                }
                if buf.len() < BATCH && !full && flushed_at.elapsed() < FLUSH_EVERY {
                    continue;
                }
                (false, None)
            }
            Err(e) => (true, Some(e)),
            Ok(None) => (true, None),
        };
        let waiting = std::time::Instant::now();
        let flushed = flush(ctx, id, &mut pos, &mut buf).await?;
        flushed_at = std::time::Instant::now();
        on_disk += flushed_at - waiting;
        // Waiting for the disk isn't silence on the network.
        if flushed_at > heard {
            heard = flushed_at;
            got_then = ctx.shared.got.load(Ordering::Relaxed);
        }
        match flushed {
            Some(end) => {
                note_best(ctx, received, started.elapsed().saturating_sub(on_disk));
                // A segment another connection shortened leaves an HTTP body far from its end
                // (dropped); an FTP session is kept either way.
                if err.is_none() && (pos >= to || matches!(body, Body::Ftp(_))) {
                    // Not past a stop (`stop_connections` waits for this connection), nor longer
                    // than a silence may last (a lane's client has no read timeout).
                    tokio::select! {
                        _ = ctx.cancel.cancelled() => {}
                        _ = tokio::time::timeout(ctx.opts.read_timeout, body.done(ended)) => {}
                    }
                }
                return Ok(Received::Complete(end));
            }
            // A capped request (`request_cap`) answered in full: the next one goes on.
            None if ended && err.is_none() && pos >= to && ctx.opts.cap().is_some() => {
                return Ok(Received::More(pos));
            }
            None if ended => return Err(err.unwrap_or(Error::ShortBody)),
            None => {}
        }
    }
}

/// How much a connection may hold in memory while the part file is flushed (`FLUSH_HOLD_MAX`,
/// shared by the connections this download may open).
pub(super) fn hold_cap(ctx: &Ctx) -> usize {
    FLUSH_HOLD_MAX / ctx.opts.connections().max(1) as usize
}

/// Remember the speed of a response that carried enough (256 KiB) to say what this server can
/// do over one connection (`on_net`: the time it took, waits for the disk not counted).
pub(super) fn note_best(ctx: &Ctx, received: u64, on_net: Duration) {
    let secs = on_net.as_secs_f64();
    if received >= 256 * 1024 && secs > 0.0 {
        ctx.shared
            .best_speed
            .fetch_max((received as f64 / secs) as u64, Ordering::Relaxed);
    }
}

/// The smallest half a split may leave now (`Planner::set_split_floor`): what one connection
/// receives (the best speed one sustained) while a range request waits for its answer (the
/// quickest one seen). A smaller half would arrive sooner where it is: splitting it only costs
/// the server a request and, over HTTP/1.1, the shortened connection. 0 until both are known
/// (the start, FTP).
pub(super) fn split_floor(ctx: &Ctx) -> u64 {
    let ttfb = ctx.shared.range_ttfb.load(Ordering::Relaxed);
    let best = ctx.shared.best_speed.load(Ordering::Relaxed);
    if ttfb == u64::MAX {
        return 0;
    }
    best.saturating_mul(ttfb) / 1000
}

/// A connection that heard nothing for `Options::read_timeout` (its waits for the disk left
/// out): as reqwest words its own timeout.
pub(super) fn silent_error() -> Error {
    Error::Net {
        kind: NetKind::Timeout,
        cause: String::new(),
    }
}

/// A speed limit makes this download's connections slow on purpose: the limiter, not the
/// connection, holds them back.
pub(super) fn limited(ctx: &Ctx) -> bool {
    ctx.shared.limiter.get().is_some() || ctx.opts.shared_limits().any(|l| l.get().is_some())
}

/// Whether a connection silent for `silent` is stalled: nothing for `STALL_AFTER` (or four of
/// the quickest range answer's time, when longer: a slow server's own pace) while the download's
/// other connections got `OTHERS_GOT` bytes since its last (`got_then`: the download's total
/// then; they may have finished since, and this one holds the end). Then the path or the
/// server's slot this connection got is stuck, not the line: a new connection asks for the rest
/// of its part at once instead of after `Options::read_timeout`. When every connection went
/// silent together (the line, the server) none reconnects, and the read timeout decides. Never
/// under a speed limit, which holds connections back on purpose.
pub(super) fn stalled(ctx: &Ctx, silent: Duration, got_then: u64) -> bool {
    if !ctx.opts.shed_slow || limited(ctx) {
        return false;
    }
    let ttfb = ctx.shared.range_ttfb.load(Ordering::Relaxed);
    let wait = match ttfb {
        u64::MAX => STALL_AFTER,
        ms => STALL_AFTER.max(Duration::from_millis(ms.saturating_mul(4))),
    };
    let others_got = ctx
        .shared
        .got
        .load(Ordering::Relaxed)
        .saturating_sub(got_then);
    silent >= wait && others_got >= OTHERS_GOT
}

/// Whether connection `id` (at `speed` for `elapsed` on this response, next byte `at`) should
/// reconnect: far slower than the best a connection of this download sustained (comparing
/// with the others fails once they are all stuck on slow paths), with a lot left at its pace.
/// Never while a speed limit makes connections slow on purpose, nor under `request_cap`: each
/// request ends soon by itself, and the server's pace varies from one to the next (YouTube's,
/// #769: reconnecting cost more than it won).
pub(super) fn should_shed(ctx: &Ctx, id: WorkerId, speed: u64, elapsed: Duration, at: u64) -> bool {
    if !ctx.opts.shed_slow || limited(ctx) || ctx.opts.cap().is_some() || elapsed < SHED_GRACE {
        return false;
    }
    let best = ctx.shared.best_speed.load(Ordering::Relaxed);
    let left = ctx.shared.with_planner(|p| p.room(id, at)).unwrap_or(0);
    speed.saturating_mul(SHED_RATIO) < best
        && left as f64 / speed.max(1) as f64 > SHED_LEFT.as_secs_f64()
}

/// Write the claimed bytes in `buf` at `pos` and commit them. `Some(end)` when the segment is
/// now complete.
pub(super) async fn flush(
    ctx: &Ctx,
    id: WorkerId,
    pos: &mut u64,
    buf: &mut Vec<u8>,
) -> Result<Option<u64>, Error> {
    let data = std::mem::replace(buf, Vec::with_capacity(BATCH));
    let n = data.len() as u64;
    if n > 0 {
        let file = ctx.file.clone();
        let at = *pos;
        let slow = ctx.opts.slow_flush.clone();
        let written = tokio::task::spawn_blocking(move || {
            let _waits_for_flush = slow.as_ref().map(|s| s.gate.read().unwrap());
            write_all_at(&file, at, &data)
        })
        .await
        .expect("writer panicked");
        ctx.shared.held.fetch_sub(n, Ordering::Relaxed);
        written?;
    }
    *pos += n;
    ctx.shared.conn(id, |c| c.downloaded += n);
    Ok(ctx.shared.with_planner(|p| p.commit(id, *pos)))
}

/// Servers without range support: one connection, written in order, under the download's own
/// limit and those it shares (`Options::shared_limits`). Returns the size.
pub(super) async fn single_stream(
    mut body: Body,
    file: &Arc<File>,
    shared: &Arc<Shared>,
    opts: &Options,
    cancel: &CancellationToken,
) -> Result<u64, Error> {
    shared.conn(0, |c| c.status = ConnStatus::Receiving);
    let expected = body.len();
    let mut pos = 0u64;
    loop {
        let next = tokio::select! {
            _ = cancel.cancelled() => return Err(Error::Cancelled),
            n = body.chunk() => n,
        };
        let Some(chunk) = next? else { break };
        // Stop ends a limiter's wait too (under a low limit it is seconds long).
        let paid = async {
            shared.limiter.acquire(chunk.len() as u64).await;
            for l in opts.shared_limits() {
                l.acquire(chunk.len() as u64).await;
            }
        };
        tokio::select! {
            _ = cancel.cancelled() => return Err(Error::Cancelled),
            _ = paid => {}
        }
        let (f, at, n) = (file.clone(), pos, chunk.len() as u64);
        tokio::task::spawn_blocking(move || write_all_at(&f, at, &chunk))
            .await
            .expect("writer panicked")?;
        pos += n;
        let mut v = shared.view.lock().unwrap();
        v.downloaded = pos;
        if let Some(c) = v.connections.first_mut() {
            c.downloaded = pos;
        }
    }
    if expected.is_some_and(|e| e != pos) {
        return Err(Error::ShortBody);
    }
    shared.conn(0, |c| c.status = ConnStatus::Finished);
    Ok(pos)
}

pub(super) fn read_at(file: &File, mut at: u64, len: usize) -> io::Result<Vec<u8>> {
    let mut buf = vec![0u8; len];
    let mut got = 0;
    while got < len {
        #[cfg(windows)]
        let n = std::os::windows::fs::FileExt::seek_read(file, &mut buf[got..], at)?;
        #[cfg(unix)]
        let n = std::os::unix::fs::FileExt::read_at(file, &mut buf[got..], at)?;
        if n == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        got += n;
        at += n as u64;
    }
    Ok(buf)
}

pub(super) fn write_all_at(file: &File, mut at: u64, mut data: &[u8]) -> io::Result<()> {
    while !data.is_empty() {
        #[cfg(windows)]
        let n = std::os::windows::fs::FileExt::seek_write(file, data, at)?;
        #[cfg(unix)]
        let n = std::os::unix::fs::FileExt::write_at(file, data, at)?;
        if n == 0 {
            return Err(io::ErrorKind::WriteZero.into());
        }
        at += n as u64;
        data = &data[n..];
    }
    Ok(())
}

/// `Content-Range: bytes a-b/total` → `(a, b, total)`; `total` is `None` for `*`.
pub(super) fn parse_content_range(h: &HeaderMap) -> Result<(u64, u64, Option<u64>), Error> {
    let raw = h
        .get(header::CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let bad = || Error::BadRange(raw.to_owned());
    let rest = raw.strip_prefix("bytes ").ok_or_else(bad)?;
    let (range, total) = rest.split_once('/').ok_or_else(bad)?;
    let (a, b) = range.split_once('-').ok_or_else(bad)?;
    let a = a.trim().parse().map_err(|_| bad())?;
    let b = b.trim().parse().map_err(|_| bad())?;
    let total = match total.trim() {
        "*" => None,
        t => Some(t.parse().map_err(|_| bad())?),
    };
    Ok((a, b, total))
}

/// `Retry-After`: delay in seconds, or an HTTP date.
pub(crate) fn retry_after(h: &HeaderMap) -> Option<Duration> {
    let v = h.get(header::RETRY_AFTER)?.to_str().ok()?.trim();
    if let Ok(secs) = v.parse::<u64>() {
        return Some(Duration::from_secs(secs));
    }
    let when = httpdate::parse_http_date(v).ok()?;
    Some(
        when.duration_since(std::time::SystemTime::now())
            .unwrap_or_default(),
    )
}

pub(crate) async fn cancellable<T>(
    cancel: &CancellationToken,
    fut: impl Future<Output = Result<T, Error>>,
) -> Result<T, Error> {
    tokio::select! {
        _ = cancel.cancelled() => Err(Error::Cancelled),
        r = fut => r,
    }
}
