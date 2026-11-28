//! Resuming: the saved state loaded, the part file checked, the file again from its start when the
//! server stopped sending parts, and the state saved as the connections write.

use super::*;

/// While downloading, the part file is flushed (and the state rewritten) only after 1/16 of
/// the file (1-64 MiB) has arrived or this long, at most once per save tick, and always when
/// the connections stop. Flushing every tick stalled HDDs and churned antivirus; IDM saves
/// "several times per minute" [IDM-doc].
///
/// A flush holds every write to the file for as long as it runs (NTFS, measured, #361), so on a
/// disk where it takes seconds, flushing as often as the file grows left the download frozen
/// most of the time. The next flush therefore waits nine times as long as the last flush during
/// the transfer took (`forced_gap`), at most `SYNC_GAP_MAX`: at most a tenth of the time goes
/// to flushing, and on a fast disk (a flush of a few milliseconds) nothing changes. What it
/// gives up: after a power cut or a crash the bytes since the last flush are downloaded again,
/// so on a slow disk up to `SYNC_GAP_MAX` of them (at 1 Gbit/s about 7 GB) instead of about 2 s
/// of them. The saved positions still never claim bytes that weren't flushed.
pub(super) const SYNC_BYTES_MAX: u64 = 64 << 20;

pub(super) const SYNC_BYTES_MIN: u64 = 1 << 20;

pub(super) const SYNC_EVERY: Duration = Duration::from_secs(30);

pub(super) const SYNC_GAP_MAX: Duration = Duration::from_secs(60);

/// When the part file was last flushed, how many bytes were on disk then, and how long it took.
pub(super) struct Synced {
    pub(super) at: std::time::Instant,
    pub(super) bytes: u64,
    pub(super) took: Duration,
}

impl Synced {
    /// Is a flush due with `bytes` on disk now, `elapsed` since the last one ended?
    pub(super) fn due(&self, bytes: u64, size: u64, elapsed: Duration) -> bool {
        elapsed >= flush_gap(self.took)
            && (bytes >= self.bytes.saturating_add(sync_bytes(size)) || elapsed >= SYNC_EVERY)
    }
}

/// Whether a resume state's part file is gone (moved or deleted by another program, #5): in
/// neither place it can be (beside the file, or Options' temporary directory, as `run_resume`
/// looks), its folder still there (a removed drive or offline share is no vanished file: the
/// data is still on it), and the download not already whole (a crash between the final rename
/// and removing the state, which `run_resume` completes).
pub(super) fn part_vanished(saved: &ResumeState) -> bool {
    let was = saved
        .part
        .clone()
        .unwrap_or_else(|| naming::beside_part_path(&saved.dest));
    let want = naming::part_path(&saved.dest);
    let folder_there = was.parent().is_none_or(Path::is_dir);
    let whole = std::fs::metadata(&saved.dest).is_ok_and(|m| m.len() == saved.size);
    folder_there && !was.exists() && !want.exists() && !whole
}

/// The resume state in `state_file`, for `Download::resume` and `Download::begin`. A removed
/// drive or offline share is said as such (the data is still there for later).
pub(super) fn load_resume(state_file: &Path) -> Result<ResumeState, Error> {
    if let Some(dir) = state_file.parent().filter(|d| !d.as_os_str().is_empty())
        && !dir.is_dir()
    {
        return Err(Error::DestinationUnavailable(dir.to_owned()));
    }
    ResumeState::load(state_file).map_err(Error::BadState)
}

pub(super) async fn run_resume(
    mut saved: ResumeState,
    state_file: PathBuf,
    mut headers: HeaderMap,
    opts: Options,
    shared: Arc<Shared>,
    cancel: CancellationToken,
) -> Result<Outcome, Error> {
    let mut opts = opts;
    if saved.asked_as_own && opts.agent_fallback.is_some() {
        Asked::Own.apply(&mut opts);
    } else if saved.asked_as_mdm {
        Asked::Fallback.apply(&mut opts);
    }
    shared.limiter.set(opts.speed_limit);
    // A state file under its former name (`<file>.mdm.json`, Dannex up to 0.6.5): it and its
    // part file take today's names first, and the download goes on from there.
    let state_file = if crate::state::is_legacy_state(&saved.dest, &state_file)
        && crate::state::migrate_legacy(&saved.dest)
    {
        state_path(&saved.dest)
    } else {
        state_file
    };
    let bad = |what: &str| Error::BadState(format!("{}: {what}", state_file.display()));
    if state_file != state_path(&saved.dest) {
        return Err(bad("does not belong to the file it names"));
    }
    let _reservation =
        naming::reserve(&saved.dest).ok_or_else(|| Error::InUse(saved.dest.clone()))?;
    // Where the part is (beside the file, or where the state says), and where Options → Save to
    // → Temporary directory wants it now: brought there when that changed, kept where it is
    // when it can't be moved.
    let was = saved
        .part
        .clone()
        .unwrap_or_else(|| naming::beside_part_path(&saved.dest));
    let want = naming::part_path(&saved.dest);
    let part = if want != was && !want.exists() && was.exists() {
        let (from, to) = (was.clone(), want.clone());
        match tokio::task::spawn_blocking(move || naming::move_part(&from, &to))
            .await
            .expect("part mover panicked")
        {
            Ok(()) => {
                shared.event("Part file moved to the temporary directory set in Options");
                want
            }
            Err(e) => {
                shared.event(format!("Part file left where it is ({e})"));
                was
            }
        }
    } else if want.exists() {
        want
    } else {
        was
    };
    saved.set_part(&part);
    // A crash between the final rename and removing the state file: the download is complete.
    if !part.exists() && std::fs::metadata(&saved.dest).is_ok_and(|m| m.len() == saved.size) {
        let _ = std::fs::remove_file(&state_file);
        shared.set_state(State::Completing);
        return Ok(Outcome {
            path: saved.dest.clone(),
            size: saved.size,
        });
    }
    // Signed links from a redirect expire: ask the original address again for today's target
    // (it must still be the same file), else keep the stored one.
    let as_get = AsGet::default();
    let seen = shared.seen.get_or_init(Default::default).clone();
    let client = client(&opts, &saved.url, &as_get, &seen)?;
    // Only the browser-like connection got past the site's bot check (#668): it goes on with
    // it, on one connection, at the address it had (DDM's own connection would be refused).
    let browser = match saved.browser_like && crate::browserlike::AVAILABLE {
        true => Some(Arc::new(crate::browserlike::Browser::new(
            &opts,
            &saved.final_url,
            &headers,
        )?)),
        false => None,
    };
    if browser.is_some() {
        opts.max_connections = 1;
        shared.event("Going on the way a browser connects, as the site's bot check asked");
    }
    // Through a service account (#44), the service is asked for today's link even when the
    // state holds none of its links (the download went directly until now).
    let refreshed =
        if browser.is_none() && (saved.url != saved.final_url || opts.unlocker.is_some()) {
            let post = opts.post.as_ref();
            let agent = opts.site_agent();
            refresh_final_url(
                &client,
                &saved.url,
                &headers,
                post,
                agent,
                saved.size,
                &cancel,
                opts.unlocker.as_ref(),
                &shared,
            )
            .await
        } else {
            None
        };
    // A POST download's target is asked with GET when its redirects turned it into one.
    let (url, final_get, refreshed_headers) = match refreshed {
        Some((url, h)) => (url.clone(), as_get.contains(&url), Some(h)),
        None => (saved.final_url.clone(), saved.final_get, None),
    };
    let origin =
        (saved.url != url && browser.is_none()).then(|| (saved.url.clone(), headers.clone()));
    // Credentials stay with the host they were given for (the refresh's own way there scoped
    // them as it went).
    headers = refreshed_headers.unwrap_or_else(|| scoped_headers(&headers, &saved.url, &url));
    let post = post_for(&opts, final_get, &mut headers);
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&part)
        .map_err(|e| bad(&format!("part file: {e}")))?;
    if file.metadata()?.len() != saved.size {
        return Err(bad("part file has the wrong size"));
    }
    // FTP: the server's size and modification time must still be those of the data on disk.
    let ftp = if crate::ftp::is_ftp(&url) {
        let site = Arc::new(crate::ftp::Site::new(&url, &headers, &opts)?);
        match cancellable(&cancel, site.check()).await {
            Ok(facts) => {
                let modified = facts.modified.map(httpdate::fmt_http_date);
                let time_changed = !opts.ignore_mtime
                    && modified.is_some()
                    && saved.validators.last_modified.is_some()
                    && modified != saved.validators.last_modified;
                if time_changed || facts.size.is_some_and(|n| n != saved.size) {
                    return Err(Error::FileChanged);
                }
            }
            // Not answering now: the connections retry, and each checks its overlap.
            Err(e) if e.transient() => {}
            Err(e) => return Err(e),
        }
        Some(site)
    } else {
        None
    };
    shared.event(format!(
        "Resuming: {} of {} on disk, from {}",
        disk::human(saved.downloaded()),
        disk::human(saved.size),
        url.host_str().unwrap_or("?"),
    ));
    let mut planner = Planner::resume(saved.size, opts.min_split, &saved.segments)
        .map_err(|_| bad("segments do not cover the file"))?;
    planner.set_preempt_after(PREEMPT_MIN);
    *shared.planner.lock().unwrap() = Some(planner);
    {
        let mut v = shared.view.lock().unwrap();
        v.state = State::Downloading;
        v.file = Some(saved.dest.clone());
        v.size = Some(saved.size);
        v.resumable = Some(true);
    }
    let file = Arc::new(file);
    let (dest, size) = (saved.dest.clone(), saved.size);
    let checksum = opts.checksum.clone().or_else(|| saved.checksum.clone());
    let web_page_ok = expects_web_page(&dest, saved.content_type.as_deref());
    let lanes = lanes_for(
        &opts,
        &url,
        &as_get,
        &seen,
        client.clone(),
        browser.is_some() || ftp.is_some(),
    );
    let mirrors = mirror_targets(
        &opts,
        opts.checksum.is_some() || saved.checksum.is_some() || opts.pieces.is_some(),
        &saved.url,
        &url,
        &headers,
        &as_get,
        &seen,
    );
    let ctx = Arc::new(Ctx {
        validators: Mutex::new({
            let mut v = saved.validators.clone();
            if opts.ignore_mtime {
                v.last_modified = None;
            }
            v
        }),
        // A POST's ranges may never have been tried (it came on one connection): one
        // connection asks first, the others follow its 206.
        wait_for_ranges: post.is_some() && ftp.is_none(),
        target: Mutex::new(Target {
            client: client.clone(),
            url: url.clone(),
            headers: headers.clone(),
            post,
            generation: 0,
        }),
        mirror_out: mirrors.iter().map(|_| AtomicBool::new(false)).collect(),
        mirrors,
        mirror_cursor: AtomicUsize::new(0),
        primary_out: AtomicBool::new(false),
        served: Default::default(),
        origin,
        refreshed: Default::default(),
        size,
        file: file.clone(),
        opts,
        shared: shared.clone(),
        cancel: cancel.child_token(),
        // Ranges worked before: a 200 now is a changed file or a range-less edge.
        ranges: AtomicU8::new(RANGES_YES),
        advertised: true,
        range_answered: Default::default(),
        as_get,
        web_page_ok,
        ftp,
        browser,
        lanes: Arc::new(lanes),
        ttfb: Duration::ZERO,
    });
    let stamp = ctx
        .opts
        .server_time
        .then(|| ctx.validators.lock().unwrap().last_modified.clone())
        .flatten()
        .and_then(|lm| httpdate::parse_http_date(&lm).ok());
    let whole_again = match segmented(ctx.clone(), saved.clone(), None).await {
        Ok(()) => false,
        // The server no longer sends parts of the file (#4: a server restarted without range
        // support, a CDN that dropped it): every range request of this run came back with the
        // whole, same file, none with its part, through the connections' retries. Nothing on the
        // disk can be continued, and resuming again would fail the same way every time: the file
        // comes again from its start, on one connection, written over the part file.
        Err(Error::RangesUnavailable | Error::NoRanges)
            if !shared.partial_seen.load(Ordering::Relaxed)
                && ctx.ftp.is_none()
                && ctx.browser.is_none() =>
        {
            whole_again(&ctx, &state_file, &cancel).await?;
            true
        }
        Err(e) => return Err(e),
    };
    let repair = Repair {
        ctx: &ctx,
        saved: &saved,
        cancel: &cancel,
    };
    let pieces = ctx.opts.pieces.clone();
    verify_and_repair(
        &shared,
        &part,
        size,
        checksum.as_ref(),
        pieces.as_ref(),
        // Its ranges are what a repair asks for.
        (!whole_again).then_some(repair),
    )
    .await?;
    // Its hold on the part file goes before the rename (`finish` takes it alone).
    drop(ctx);
    finish(&shared, file, &part, &dest, size, stamp)
}

/// A resume whose server no longer sends parts of the file (`run_resume`): the file again from
/// its start, on one connection, over the part file, said in the log. The answer must still be
/// the same file (its length and validators, `same_file`), else it changed. The resume state goes
/// first: a stop or a crash meanwhile starts the download afresh, never from segments the new
/// bytes are overwriting.
pub(super) async fn whole_again(
    ctx: &Arc<Ctx>,
    state_file: &Path,
    cancel: &CancellationToken,
) -> Result<(), Error> {
    let shared = &ctx.shared;
    shared
        .event("The server no longer sends parts of the file: downloading it again from the start");
    let _ = std::fs::remove_file(state_file);
    *shared.planner.lock().unwrap() = None;
    {
        let mut v = shared.view.lock().unwrap();
        v.resumable = Some(false);
        v.downloaded = 0;
        v.connections.clear();
    }
    let target = ctx.target();
    let (resp, ..) = get(
        &target.client,
        target.url.clone(),
        target.headers.clone(),
        target.post.as_ref(),
        ctx.opts.site_agent(),
        // A web page in the file's place is refused, as a range answer's would be.
        !ctx.web_page_ok,
        cancel,
    )
    .await?;
    let head = Head::of(&resp);
    if !same_file(ctx, &head) {
        return Err(Error::FileChanged);
    }
    let got = single_stream(Body::Http(resp), &ctx.file, shared, &ctx.opts, cancel).await?;
    if got != ctx.size {
        return Err(Error::ShortBody);
    }
    Ok(())
}

/// Follow the original address again (with its own headers, and a POST download's body) as the
/// first request did (`get`: redirects, waiting pages, a site's own steps) and return where it
/// leads now, if that is still the same file: a ranged answer with the saved size; with the
/// headers to go on with there. None keeps the stored address (no redirect any more, a different
/// file, or the site is unreachable).
#[allow(clippy::too_many_arguments)]
pub(super) async fn refresh_final_url(
    client: &Client,
    origin: &Url,
    headers: &HeaderMap,
    post: Option<&Bytes>,
    site_agent: bool,
    size: u64,
    cancel: &CancellationToken,
    unlocker: Option<&Arc<dyn crate::unlock::Unlocker>>,
    shared: &Shared,
) -> Option<(Url, HeaderMap)> {
    // Through a service account (#44): the service gives today's direct link (the host's link
    // itself leads to its web page); the host's cookies and logins stay with the host.
    let unlocked = match unlocker.filter(|_| post.is_none()) {
        Some(u) => unlock_noted(u, origin, Some(shared), cancel)
            .await
            .ok()
            .flatten(),
        None => None,
    };
    let (target, mut asked) = match &unlocked {
        Some(u) => (u.url.clone(), u.headers.clone()),
        None => (origin.clone(), headers.clone()),
    };
    asked.insert(header::RANGE, HeaderValue::from_static("bytes=0-0"));
    let (resp, mut headers, _) = get(client, target, asked, post, site_agent, false, cancel)
        .await
        .ok()?;
    headers.remove(header::RANGE);
    if resp.status() != StatusCode::PARTIAL_CONTENT {
        return None;
    }
    let (_, _, total) = parse_content_range(resp.headers()).ok()?;
    (total == Some(size)).then(|| (resp.url().clone(), headers))
}

pub(super) fn sync_bytes(size: u64) -> u64 {
    (size / 16).clamp(SYNC_BYTES_MIN, SYNC_BYTES_MAX)
}

/// How long to wait after a flush that took `took` before the next.
pub(super) fn flush_gap(took: Duration) -> Duration {
    took.saturating_mul(9).min(SYNC_GAP_MAX)
}

/// Save the resume state if it is due (`force`: always). `saved` is the state last written; it
/// never claims bytes that weren't flushed (snapshot -> flush -> write).
pub(super) async fn save_state(
    ctx: &Ctx,
    saved: &mut ResumeState,
    path: &Path,
    synced: &mut Synced,
    force: bool,
) -> Result<(), Error> {
    let segments = ctx.shared.with_planner(|p| p.save());
    let validators = ctx.validators.lock().unwrap().clone();
    let target = ctx.target();
    let (final_url, final_get) = (target.url, ctx.opts.post.is_some() && target.post.is_none());
    let meta_changed = validators != saved.validators
        || final_url != saved.final_url
        || final_get != saved.final_get;
    let bytes: u64 = segments.iter().map(|s| s.cursor - s.start).sum();
    let due = force || synced.due(bytes, ctx.size, synced.at.elapsed());
    let file = ctx.file.clone();
    let path = path.to_owned();
    if !due {
        // Between flushes only a new validator or address is written (with segments on disk).
        if meta_changed {
            saved.validators = validators;
            saved.final_url = final_url;
            saved.final_get = final_get;
            let s = saved.clone();
            tokio::task::spawn_blocking(move || s.write(&path))
                .await
                .expect("state writer panicked")?;
        }
        return Ok(());
    }
    if segments == saved.segments && !meta_changed && path.exists() {
        // Nothing new since the last write.
        *synced = Synced {
            at: std::time::Instant::now(),
            bytes,
            took: Duration::ZERO,
        };
        return Ok(());
    }
    let mut s = saved.clone();
    s.validators = validators;
    s.final_url = final_url;
    s.final_get = final_get;
    // Tests' slow disk (not for the forced saves at the start and the end).
    let slow = if force {
        None
    } else {
        ctx.opts.slow_flush.clone()
    };
    ctx.shared.flushing.store(true, Ordering::Release);
    let began = std::time::Instant::now();
    let done = tokio::task::spawn_blocking(move || {
        if let Some(slow) = slow {
            let _writes_wait = slow.gate.write().unwrap();
            std::thread::sleep(slow.takes);
        }
        s.save(&path, segments, &file).map(|()| s)
    })
    .await
    .expect("state writer panicked");
    ctx.shared.flushing.store(false, Ordering::Release);
    *saved = done?;
    *synced = Synced {
        at: std::time::Instant::now(),
        bytes,
        took: forced_gap(force, began.elapsed()),
    };
    Ok(())
}

/// The flush time that spaces out the next flush. A forced save runs while no connection
/// writes (before they start, after they stop), so it holds no transfer up and doesn't push the
/// next save back: the first one at the start, which also pays for creating the part file, once
/// delayed the first save of progress past the end of a short download (#518).
pub(super) fn forced_gap(force: bool, took: Duration) -> Duration {
    if force { Duration::ZERO } else { took }
}
