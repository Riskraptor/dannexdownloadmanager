//! What every connection needs to fetch a segment (`Ctx`), the body of an answer coming in, and the
//! connections run over the planner's segments: started, ramped up, supervised and stopped.

use super::*;

/// What every connection needs to fetch a segment.
pub(super) struct Ctx {
    /// The file's version; each range request's `If-Range` is chosen from them.
    pub(super) validators: Mutex<Validators>,
    /// Where ranges come from now (after redirects); switched when it expires.
    pub(super) target: Mutex<Target>,
    /// Independently configured clients for verified Metalink mirrors.  A client is tied to its
    /// mirror's DNS, proxy and selected network adapter, rather than accidentally inheriting
    /// the primary's resolver port or connection pool.
    pub(super) mirrors: Vec<Target>,
    pub(super) mirror_cursor: AtomicUsize,
    /// The mirrors left out (`drop_mirror`, `blame`), one flag each.
    pub(super) mirror_out: Vec<AtomicBool>,
    /// The main address left out while a mirror is in: it sent pieces that failed their check
    /// (`blame`).
    pub(super) primary_out: AtomicBool,
    /// Which source each range was asked of (`[from, to)`, 0 the main address, `i + 1` mirror
    /// `i`), while piece hashes can tell a bad one (`blame`).
    pub(super) served: Mutex<Vec<(u64, u64, usize)>>,
    /// The address the user gave and its headers, when it redirected: asked again when the
    /// target expires.
    pub(super) origin: Option<(Url, HeaderMap)>,
    /// When asking the origin last failed to help (one refresh at a time; after a failure,
    /// none for `REFRESH_EVERY`).
    pub(super) refreshed: tokio::sync::Mutex<Option<std::time::Instant>>,
    pub(super) size: u64,
    pub(super) file: Arc<File>,
    pub(super) opts: Options,
    pub(super) shared: Arc<Shared>,
    pub(super) cancel: CancellationToken,
    /// `RANGES_*`: learned from the first range answer.
    pub(super) ranges: AtomicU8,
    /// The first answer advertised ranges (`Accept-Ranges: bytes`, or a 206): a full answer to
    /// a range request with the same ETag is then an edge that missed the range this time, not
    /// a server without ranges (#136).
    pub(super) advertised: bool,
    /// Further connections wait for a range answer (`range_answered`): a POST whose server
    /// hasn't shown yet that it takes ranges (`Options::post`), or a GET whose first answer
    /// didn't offer them (one range request asks first, `supervise`).
    pub(super) wait_for_ranges: bool,
    /// Signalled by every 206 answer.
    pub(super) range_answered: tokio::sync::Notify,
    /// Where a POST download's redirects turned into GETs (refreshing the target).
    pub(super) as_get: AsGet,
    /// The file itself is a web page; otherwise an HTML answer is a sign-in wall or an error.
    pub(super) web_page_ok: bool,
    /// An FTP download's server and its idle sessions (else HTTP through `client`).
    pub(super) ftp: Option<Arc<crate::ftp::Site>>,
    /// The browser-like connection every request goes through instead of `client`, when only it
    /// got past the site's bot check (#668).
    pub(super) browser: Option<Arc<crate::browserlike::Browser>>,
    /// Each connection's own client for the primary address (`Lanes`).
    pub(super) lanes: Arc<Lanes>,
    /// How long the first request took to answer (zero when unknown: a resume): what opening a
    /// connection costs (`ramp`).
    pub(super) ttfb: Duration,
}

/// An answer's body coming in: an HTTP response, or an FTP transfer.
pub(crate) enum Body {
    Http(Response),
    Ftp(crate::ftp::Transfer),
    /// An answer to the browser-like connection (#668, `crate::browserlike`).
    #[cfg_attr(not(feature = "browser-connection"), allow(dead_code))]
    Browser(crate::browserlike::Stream),
}

impl Body {
    /// The next bytes; None at the end.
    pub(super) async fn chunk(&mut self) -> Result<Option<bytes::Bytes>, Error> {
        match self {
            Body::Http(r) => Ok(r.chunk().await?),
            Body::Ftp(t) => t.chunk().await,
            Body::Browser(b) => b.chunk().await,
        }
    }

    /// The bytes it will carry, if known.
    pub(super) fn len(&self) -> Option<u64> {
        match self {
            Body::Http(r) => r.content_length(),
            Body::Ftp(t) => t.expected(),
            Body::Browser(b) => b.len(),
        }
    }

    /// Done with it, at its end or before: its connection may serve the next request.
    pub(super) async fn done(self, at_end: bool) {
        match self {
            // Read the end of the body so the connection goes back to the pool.
            Body::Http(mut r) => {
                if !at_end {
                    let _ = r.chunk().await;
                }
            }
            Body::Ftp(t) => t.done(at_end).await,
            // Its connection isn't pooled for another request: there is only the one.
            Body::Browser(_) => {}
        }
    }
}

/// What the engine reads of an HTTP answer's head, from either client (reqwest's, or the
/// browser-like one's, #668).
#[derive(Clone, Debug)]
pub(crate) struct Head {
    pub(crate) status: StatusCode,
    pub(crate) headers: HeaderMap,
    /// Where the answer came from, its redirects followed.
    pub(crate) url: Url,
    pub(crate) len: Option<u64>,
}

impl Head {
    pub(super) fn of(resp: &Response) -> Head {
        Head {
            status: resp.status(),
            headers: resp.headers().clone(),
            url: resp.url().clone(),
            len: resp.content_length(),
        }
    }
}

impl Ctx {
    pub(super) fn target(&self) -> Target {
        self.target.lock().unwrap().clone()
    }

    /// The table of learned connection caps: the options' (shared with other downloads), else
    /// this download's own.
    pub(super) fn host_limits(&self) -> &crate::hosts::HostLimits {
        self.opts
            .host_limits
            .as_deref()
            .unwrap_or(&self.shared.own_limits)
    }

    /// The next safe source for a ranged request, and which mirror it is (None: the main
    /// address). Mirrors are there only when the download has a whole-file checksum or piece
    /// hashes (`mirror_targets`); otherwise the primary remains the sole authority. The sources
    /// take turns; one left out (`mirror_out`, `primary_out`) is skipped.
    pub(super) fn source(&self, primary: &Target) -> (Target, Option<usize>) {
        if self.mirrors.is_empty()
            || primary.post.is_some()
            || self.ftp.is_some()
            || self.browser.is_some()
        {
            return (primary.clone(), None);
        }
        let n = self.mirrors.len() + 1;
        let any_mirror = self.mirror_out.iter().any(|o| !o.load(Ordering::Relaxed));
        // The main address is never left without another source in.
        let primary_in = !any_mirror || !self.primary_out.load(Ordering::Relaxed);
        for _ in 0..n {
            let next = self.mirror_cursor.fetch_add(1, Ordering::Relaxed) % n;
            match next {
                0 if primary_in => return (primary.clone(), None),
                0 => {}
                i if !self.mirror_out[i - 1].load(Ordering::Relaxed) => {
                    return (self.mirrors[i - 1].clone(), Some(i - 1));
                }
                _ => {}
            }
        }
        (primary.clone(), None)
    }

    /// Mirror `i` is left out for the rest of the download, for `why`.
    pub(super) fn drop_mirror(&self, i: usize, why: &str) {
        if !self.mirror_out[i].swap(true, Ordering::Relaxed) {
            let url = &self.mirrors[i].url;
            let host = url.host_str().unwrap_or("?");
            let place = match url.port() {
                Some(port) => format!("{host}:{port}"),
                None => host.to_owned(),
            };
            self.shared.event(format!(
                "Metalink mirror {place}: {why}; not used for the rest of this download"
            ));
        }
    }

    /// Notes that `[from, to)` was asked of `source` (None: the main address), while piece
    /// hashes can tell which source sent a bad piece.
    pub(super) fn note_served(&self, from: u64, to: u64, source: Option<usize>) {
        if self.opts.pieces.is_some() && !self.mirrors.is_empty() {
            let at = source.map_or(0, |i| i + 1);
            self.served.lock().unwrap().push((from, to, at));
        }
    }

    /// Pieces `bad` failed their check: the mirrors that sent any of them are left out; when
    /// only the main address did, it is (while a mirror is still in), so that they come from
    /// elsewhere next time.
    pub(super) fn blame(&self, bad: &[usize], pieces: &crate::verify::Pieces) {
        let mut who = HashSet::new();
        for &(from, to, at) in self.served.lock().unwrap().iter() {
            if bad.iter().any(|&p| {
                let (start, end) = pieces.range(p, self.size);
                from < end && start < to
            }) {
                who.insert(at);
            }
        }
        for at in &who {
            if *at > 0 {
                self.drop_mirror(at - 1, "it sent data that failed the piece check");
            }
        }
        if who.len() == 1 && who.contains(&0) && !self.primary_out.swap(true, Ordering::Relaxed) {
            self.shared.event(
                "The main address sent data that failed the piece check; those pieces come \
                 from the mirrors"
                    .to_owned(),
            );
        }
    }

    /// The same download's context for another round of connections (`repair_pieces`): the
    /// connections of the last one stopped with its token cancelled. The sources take turns
    /// from one further on, so a piece isn't asked of the same one again by chance.
    pub(super) fn again(&self, cancel: CancellationToken) -> Ctx {
        Ctx {
            validators: Mutex::new(self.validators.lock().unwrap().clone()),
            target: Mutex::new(self.target()),
            mirrors: self.mirrors.clone(),
            mirror_cursor: AtomicUsize::new(self.mirror_cursor.load(Ordering::Relaxed) + 1),
            mirror_out: self
                .mirror_out
                .iter()
                .map(|o| AtomicBool::new(o.load(Ordering::Relaxed)))
                .collect(),
            primary_out: AtomicBool::new(self.primary_out.load(Ordering::Relaxed)),
            served: Mutex::new(Vec::new()),
            origin: self.origin.clone(),
            refreshed: Default::default(),
            size: self.size,
            file: self.file.clone(),
            opts: self.opts.clone(),
            shared: self.shared.clone(),
            cancel,
            ranges: AtomicU8::new(self.ranges.load(Ordering::SeqCst)),
            advertised: self.advertised,
            wait_for_ranges: false,
            range_answered: Default::default(),
            as_get: self.as_get.clone(),
            web_page_ok: self.web_page_ok,
            ftp: self.ftp.clone(),
            browser: self.browser.clone(),
            lanes: self.lanes.clone(),
            ttfb: self.ttfb,
        }
    }

    /// After `generation`'s address answered as a dead link: ask the original address for
    /// today's target (once for all connections, at most every `REFRESH_EVERY`) and switch to
    /// it if it is the same file. True when there is a newer target to retry with.
    pub(super) async fn refresh_target(&self, generation: u64) -> bool {
        let Some((origin, headers)) = &self.origin else {
            return false;
        };
        let mut failed_at = self.refreshed.lock().await;
        if self.target.lock().unwrap().generation != generation {
            return true; // another connection already switched
        }
        // Signed links may live minutes while the download takes hours: refresh as often as
        // that helps, but after an unhelpful refresh wait before asking again.
        if failed_at.is_some_and(|t| t.elapsed() < REFRESH_EVERY) {
            return false;
        }
        let Some((url, mut headers)) = refresh_final_url(
            &self.target().client,
            origin,
            headers,
            self.opts.post.as_ref(),
            self.opts.site_agent(),
            self.size,
            &self.cancel,
            self.opts.unlocker.as_ref(),
            &self.shared,
        )
        .await
        .filter(|(u, _)| *u != self.target.lock().unwrap().url) else {
            *failed_at = Some(std::time::Instant::now());
            return false;
        };
        *failed_at = None;
        self.shared.event(format!(
            "The address stopped working; the original address now leads to {}",
            url.host_str().unwrap_or("?")
        ));
        let post = post_for(&self.opts, self.as_get.contains(&url), &mut headers);
        let mut t = self.target.lock().unwrap();
        t.headers = headers;
        t.post = post;
        t.url = url;
        t.generation += 1;
        true
    }

    pub(super) fn learn_ranges(&self, yes: bool) {
        let v = if yes { RANGES_YES } else { RANGES_NO };
        if self
            .ranges
            .compare_exchange(RANGES_UNKNOWN, v, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            self.shared.view.lock().unwrap().resumable = Some(yes);
        }
    }
}

/// Run the connections with the resume state saved every `save_interval`, and once more when
/// they stop. The state file is removed when the download completes.
pub(super) async fn segmented(
    ctx: Arc<Ctx>,
    mut saved: ResumeState,
    first: Option<Body>,
) -> Result<(), Error> {
    let path = state_path(&saved.dest);
    let mut synced = Synced {
        at: std::time::Instant::now(),
        bytes: 0,
        took: Duration::ZERO,
    };
    save_state(&ctx, &mut saved, &path, &mut synced, true).await?;
    let interval = ctx.opts.save_interval;
    let result = {
        // Saves until told to stop or a save fails (resolving to that error). It stops only
        // between saves: a save's blocking write can't be cancelled, so dropping the saver
        // mid-save would leave that write running after the download has returned, writing a
        // stale state file next to a finished or already resumed download (#106).
        let stop = CancellationToken::new();
        let saver = async {
            loop {
                tokio::select! {
                    () = tokio::time::sleep(interval) => {}
                    () = stop.cancelled() => return None,
                }
                if let Err(e) = save_state(&ctx, &mut saved, &path, &mut synced, false).await {
                    return Some(e);
                }
            }
        };
        let supervisor = supervise(ctx.clone(), first);
        tokio::pin!(saver, supervisor);
        tokio::select! {
            r = &mut supervisor => {
                // The connections stopped: let a save under way finish. Its error, if any, is
                // the final save's to report (it writes the same file, forced, just below).
                stop.cancel();
                let _ = saver.await;
                r
            }
            e = &mut saver => Err(e.expect("the saver only stops when told to")),
        }
    };
    // The whole run failed on the network or the link (#4: a network gone, a server restarting,
    // a link answering 403): the refusals its connections met said that, not a connection limit,
    // and what they taught would have the resume go on with one connection.
    if let Err(e) = &result
        && (e.network_lost() || e.link_dead())
        && let Some(host) = ctx.target().url.host_str()
    {
        ctx.host_limits().forget(host);
    }
    match &result {
        // Not resumable on one connection: the state is useless.
        Err(Error::NoRanges) => {
            let _ = std::fs::remove_file(&path);
        }
        // Complete: the final state stays until `finish` has renamed the part file, so a
        // crash in between still resumes (to an immediate rename).
        Ok(()) | Err(_) => save_state(&ctx, &mut saved, &path, &mut synced, true).await?,
    }
    result
}

/// Run connections until the planner is complete. Respawns a connection when work is left
/// and none is running (e.g. the others retired).
pub(super) async fn supervise(ctx: Arc<Ctx>, first: Option<Body>) -> Result<(), Error> {
    let mut set = JoinSet::new();
    let mut next_id: WorkerId = 0;
    if let Some(body) = first {
        // The probe's body is byte 0 onward: reserve that segment for connection 0 before any
        // other connection can split or take it.
        ctx.shared.with_planner(|p| p.assign(0, None));
        set.spawn(worker(ctx.clone(), 0, Some(body)));
        next_id = 1;
    }
    // No more connections than the host is known to allow.
    let host = ctx.target().url.host_str().unwrap_or("").to_owned();
    let learned = ctx.host_limits().cap(&host);
    let wanted = ctx.opts.connections();
    if let Some(cap) = learned.filter(|c| *c < wanted) {
        ctx.shared
            .event(format!("{host} allows {cap} connection(s) (learned)"));
    }
    let max = learned.map_or(wanted, |c| c.min(wanted));
    // A small file starts with fewer connections (one per INITIAL_PIECE): opened all at once
    // they would cut it into slivers and fetch some twice (benchmark 2026-10-30: 256 KiB took
    // 13-19 requests; IDM, which ramps up in waves, took 4). Later splits still go down to
    // `min_split` as connections free up.
    let size = ctx.shared.with_planner(|p| p.size());
    let by_size = u32::try_from(size.div_ceil(INITIAL_PIECE)).unwrap_or(u32::MAX);
    let max = max.min(by_size.max(1)).max(1);
    // A POST the server hasn't shown to take ranges: one connection, until a 206 says the
    // others may follow (a second POST may do the form's work twice). A GET whose first answer
    // didn't offer ranges (no `Accept-Ranges: bytes`): one range request beside it first, not
    // all of them at once, each answered with the whole file until it is dropped (Cloudflare's
    // speed test, 2026-11-27: 7 such answers competed with the first one at the start).
    let mut held = ctx.wait_for_ranges;
    let first_wave = if ctx.target().post.is_some() { 1 } else { 2 };
    // A small file: the first connection alone until its pace is known (`ramp`). Not with
    // Metalink mirrors: their sources take turns from the start, so that each is tried (and
    // one that fails is left out, `drop_mirror`) whatever the file's size.
    let started = tokio::time::Instant::now();
    let ramps = next_id == 1 && !held && max > 1 && size <= RAMP_SIZE && ctx.mirrors.is_empty();
    let mut ramp_at = ramps.then(|| started + (ctx.ttfb / 2).clamp(RAMP_LOOK_MIN, RAMP_LOOK_MAX));
    let first_count = match (held, ramp_at) {
        (true, _) => first_wave.min(max),
        (false, Some(_)) => 1,
        (false, None) => max,
    };
    while next_id < first_count {
        set.spawn(worker(ctx.clone(), next_id, None));
        next_id += 1;
    }
    let mut last_err = None;
    loop {
        let far = started + Duration::from_secs(86_400);
        let joined = tokio::select! {
            joined = set.join_next() => match joined {
                Some(joined) => joined,
                None => break,
            },
            () = ctx.range_answered.notified(), if held => {
                held = false;
                while next_id < max {
                    set.spawn(worker(ctx.clone(), next_id, None));
                    next_id += 1;
                }
                continue;
            }
            () = tokio::time::sleep_until(ramp_at.unwrap_or(far)), if ramp_at.is_some() => {
                ramp_at = None;
                let want = ramp(&ctx, max, started.elapsed());
                if want > 1 {
                    ctx.shared.event(format!("Opening {} more connection(s)", want - 1));
                } else {
                    ctx.shared.event(
                        "One connection: it finishes sooner than another would start",
                    );
                }
                while next_id < want {
                    set.spawn(worker(ctx.clone(), next_id, None));
                    next_id += 1;
                }
                continue;
            }
        };
        match joined.expect("worker panicked") {
            Err(e) if e.is_fatal() => {
                stop_connections(&ctx, &mut set).await;
                return Err(e);
            }
            Err(e) => last_err = Some(e),
            Ok(()) => {}
        }
        let complete = ctx.shared.with_planner(|p| p.is_complete());
        // Every byte is in: the connections still asking (a retry under way, an answer late on
        // a busy machine) have nothing left to fetch. Stopped now, a late answer can't start
        // the download over (#136: a server without ranges sent the whole file twice).
        if complete {
            stop_connections(&ctx, &mut set).await;
            break;
        }
        if set.is_empty() {
            if let Some(e) = last_err.take() {
                return Err(e); // the last connection exhausted its retries
            }
            set.spawn(worker(ctx.clone(), next_id, None));
            next_id += 1;
        }
    }
    Ok(())
}

/// How many connections a small file gets once its first connection has received for `looked`
/// (`RAMP_SIZE`): one while the first connection, at its pace, finishes the rest within two of
/// its times to first byte (a new connection would only start by then; measured 2026-11-27 by
/// the video work: TikTok's CDN answers a new connection in 1.3 s, and 1.3 MB took 1.6 s on one
/// connection, 3.7 s on eight); else one for each part of the rest that a connection would
/// receive while a new one waits for its first byte, at most `max`. Nothing received yet (a
/// first connection that is stuck): all of them.
pub(super) fn ramp(ctx: &Ctx, max: u32, looked: Duration) -> u32 {
    let got = ctx.shared.got.load(Ordering::Relaxed);
    let rest = ctx.size.saturating_sub(got);
    let pace = got as f64 / looked.as_secs_f64().max(1e-3);
    let setup = ctx.ttfb.as_secs_f64();
    if rest == 0 {
        return 1;
    }
    if pace <= 0.0 {
        return max;
    }
    if rest as f64 / pace <= 2.0 * setup {
        return 1;
    }
    let each = (pace * setup).max(1.0);
    ((rest as f64 / each).ceil() as u32).clamp(1, max)
}

/// Stop the connections still running, and wait until each has. They are told to stop, not
/// aborted: a connection aborted in a blocking read or write of the part file (the check of
/// its overlap, a write) leaves that call running, with the file open, after the download has
/// ended. A finished download then failed with "part file still in use", the rename's handle
/// still taken (#201: a connection taken over in its overlap check, on a busy machine), and a
/// stopped one went on writing after it had returned. Every wait of a connection ends on its
/// token, and a blocking call only takes as long as one read or write.
pub(super) async fn stop_connections(ctx: &Ctx, set: &mut JoinSet<Result<(), Error>>) {
    ctx.cancel.cancel();
    while set.join_next().await.is_some() {}
}
