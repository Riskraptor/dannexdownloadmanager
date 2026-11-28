//! A fresh download handed to the planner: its destination opened (a free name, the part file
//! made), a single stream or segments, FTP's sessions.

use super::*;

/// Runs a download body and records its final state.
pub(super) async fn run(
    body: impl Future<Output = Result<Outcome, Error>>,
    shared: Arc<Shared>,
) -> Result<Outcome, Error> {
    let result = body.await;
    shared.event(match &result {
        Ok(o) => format!("Complete: {}", o.path.display()),
        Err(Error::Cancelled) => "Stopped".to_owned(),
        Err(e) => format!("Failed: {e}"),
    });
    let mut v = shared.view.lock().unwrap();
    v.state = match &result {
        Ok(o) => {
            // Where it really is: `finish` numbers the name when another file took it meanwhile,
            // and whoever reads the view (the app's row) goes by this.
            v.file = Some(o.path.clone());
            State::Done
        }
        Err(Error::Cancelled) => State::Cancelled,
        Err(e) => {
            v.error = Some(e.to_string());
            v.failure = Some(e.clone());
            State::Failed
        }
    };
    drop(v);
    result
}

pub(super) async fn run_fresh(
    req: Request,
    opts: Options,
    shared: Arc<Shared>,
    cancel: CancellationToken,
    kept: Option<KeptAnswer>,
) -> Result<Outcome, Error> {
    let mut opts = opts;
    // The User-Agent the first request got its answer with (#372, `Asked`).
    let mut asked_as: Asked;
    shared.limiter.set(opts.speed_limit);
    let (shared, cancel) = (&shared, &cancel);
    shared.conn(0, |_| {});
    // Probe on connection 0: a plain GET (or the form's POST) whose body becomes the segment
    // starting at 0; or the look's answer kept for it (#28), not asked for again.
    // A web page where the download expects another kind of file is an error (#533): the look's
    // answer, asked before the file's name was settled, is asked for again then (`get` says so).
    let expects = expects_other_file(
        opts.expected_type.as_deref(),
        dest_name(&req.dest),
        &req.url,
    );
    let kept = kept.filter(|k| {
        k.url == req.url
            && k.at.elapsed() < KEEP_ANSWER_FOR
            && !(expects && is_web_page(&k.head.headers) && !named(&k.head.headers))
    });
    let mut unlocked = kept.as_ref().and_then(|k| k.unlocked.clone());
    // Where the first request went: a service's (or a helper's, #769) direct link when one gave
    // it, which isn't a redirect.
    let mut first_at = req.url.clone();
    // The browser-like connection the download goes on with, when only it got through (#668).
    let mut browser: Option<Arc<crate::browserlike::Browser>> = None;
    let (as_get, client0, (head, body), headers, ttfb) = match kept {
        Some(k) => {
            shared.event(
                "Connection 1: continuing from the answer the file's information window got",
            );
            let _ = shared.seen.set(k.seen);
            browser = k.browser;
            // The look's answer came with this User-Agent: the other connections ask with it.
            k.asked.apply(&mut opts);
            asked_as = k.asked;
            (k.as_get, k.client, (k.head, k.body), k.headers, k.took)
        }
        None => {
            // Through a service account (#44): its direct link, without the host's cookies
            // or logins. The host's link stays the download's origin, asked again (through the
            // service) when the direct link dies.
            unlocked = unlock_origin(&opts, &req.url, Some(shared), cancel).await?;
            let (mut target, mut asked) = match &unlocked {
                Some(u) => (u.url.clone(), u.headers.clone()),
                None => (req.url.clone(), req.headers.clone()),
            };
            // A server that slows long answers down (`request_cap`): the first request is a
            // range of that size too, its total the file's size.
            if let Some(cap) = opts.cap() {
                asked.insert(header::RANGE, range_value(0, cap));
            }
            first_at = target.clone();
            let as_get = AsGet::default();
            let seen = shared.seen.get_or_init(Default::default).clone();
            let as_given = opts.clone();
            // The download's own User-Agent, which the browser-like connection carries (#668),
            // even when DDM's connection had to give it up (#372).
            let given_agent = opts.user_agent.clone();
            let ask = FirstAsk {
                as_get: &as_get,
                seen: &seen,
                expects,
                shared,
                cancel,
            };
            let (mut client0, mut first, as_what) = ask.at(&mut opts, &target, &asked).await?;
            asked_as = as_what;
            // A Metalink's alternate locations can rescue a primary that is down before it
            // sends its first byte.  Do this only for a manifest that supplied a whole-file
            // checksum: without one, changing hosts would make a failed URL a data-integrity
            // risk rather than a reliable fallback.
            if first.is_err()
                && unlocked.is_none()
                && opts.post.is_none()
                && (opts.checksum.is_some() || opts.pieces.is_some())
            {
                for mirror in opts.mirrors.clone() {
                    if !matches!(mirror.scheme(), "http" | "https") || mirror == target {
                        continue;
                    }
                    let mirror_headers = scoped_headers(&asked, &target, &mirror);
                    shared.event(format!(
                        "{} did not answer; trying Metalink mirror {}",
                        target.host_str().unwrap_or("?"),
                        mirror.host_str().unwrap_or("?")
                    ));
                    let mut mirror_opts = as_given.clone();
                    match ask.at(&mut mirror_opts, &mirror, &mirror_headers).await {
                        Ok((client, Ok(answer), as_what)) => {
                            (target, asked, opts, client0, first, asked_as) = (
                                mirror.clone(),
                                mirror_headers,
                                mirror_opts,
                                client,
                                Ok(answer),
                                as_what,
                            );
                            first_at = target.clone();
                            break;
                        }
                        Ok((_, Err(Error::Cancelled), _)) | Err(Error::Cancelled) => {
                            return Err(Error::Cancelled);
                        }
                        Ok((_, Err(error), _)) | Err(error) => {
                            shared.event(format!(
                                "Metalink mirror {}: {error}",
                                mirror.host_str().unwrap_or("?")
                            ));
                        }
                    }
                }
            }
            // #630: the link refused, where the browser's own download of it ended is asked
            // once, as the link was (the browser's User-Agent first). The link's refusal is
            // the one told if that fails too.
            if let Err(e) = &first
                && link_refused(e)
                && unlocked.is_none()
                && opts.post.is_none()
                && let Some(other) = opts.browser_final.clone().filter(|o| *o != target)
            {
                shared.event(format!(
                    "{}: {e}; asking where the browser's download ended ({})",
                    target.host_str().unwrap_or("?"),
                    other.host_str().unwrap_or("?"),
                ));
                let mut again = as_given;
                let headers = scoped_headers(&asked, &target, &other);
                match ask.at(&mut again, &other, &headers).await {
                    Ok((client1, Ok(answer), as_what)) => {
                        (opts, client0, first, asked_as) = (again, client1, Ok(answer), as_what);
                    }
                    Ok((_, Err(Error::Cancelled), _)) | Err(Error::Cancelled) => {
                        return Err(Error::Cancelled);
                    }
                    Ok((_, Err(e), _)) | Err(e) => {
                        shared.event(format!("{}: {e}", other.host_str().unwrap_or("?")));
                    }
                }
            }
            // #668: the site refused DDM's connection as a program's (its bot check, or a 412):
            // asked once more, the way a browser connects, at the same address with the same
            // headers. Its refusal stays the one told if that fails too.
            match first {
                Ok(((resp, headers), ttfb)) => {
                    seen.answered(&resp);
                    let head = Head::of(&resp);
                    (as_get, client0, (head, Body::Http(resp)), headers, ttfb)
                }
                Err(e) if refused_as_a_program(&e) && unlocked.is_none() && opts.post.is_none() => {
                    if asked_as != Asked::Given {
                        opts.user_agent = given_agent;
                        asked_as = Asked::Given;
                    }
                    let (b, head, body, headers, ttfb) =
                        browser_first(&opts, &target, &asked, e, expects, shared, cancel).await?;
                    browser = Some(b);
                    (as_get, client0, (head, body), headers, ttfb)
                }
                Err(e) => return Err(e),
            }
        }
    };
    // One connection, as a browser downloads: the site let this one through.
    if browser.is_some() {
        opts.max_connections = 1;
    }
    // The service's own cap on connections to its link (Real-Debrid's "chunks").
    if let Some(cap) = unlocked
        .as_ref()
        .and_then(|u| u.max_connections)
        .filter(|c| *c >= 1)
    {
        opts.max_connections = opts.max_connections.min(cap);
    }
    let preempt_after = (ttfb * PREEMPT_TTFB).max(PREEMPT_MIN);
    let size = match head.status {
        StatusCode::OK => head.len,
        // Some servers answer partially even without Range; accept it if it starts at 0.
        StatusCode::PARTIAL_CONTENT => match parse_content_range(&head.headers)? {
            (0, _, total) => total,
            (start, ..) => return Err(Error::BadRange(format!("probe started at {start}"))),
        },
        s => return Err(Error::Status(s)),
    };
    let ranges = if head.status == StatusCode::PARTIAL_CONTENT {
        RANGES_YES
    } else {
        RANGES_UNKNOWN
    };
    // The first answer says the server takes ranges (`Accept-Ranges: bytes`).
    let advertised = ranges == RANGES_YES
        || header_str(&head.headers, header::ACCEPT_RANGES)
            .is_some_and(|v| v.split(',').any(|t| t.trim().eq_ignore_ascii_case("bytes")));
    let final_url = head.url.clone();
    shared.event(format!(
        "Connected to {}{}: {}, size {}",
        final_url.host_str().unwrap_or("?"),
        if final_url != first_at {
            " (redirected)"
        } else {
            ""
        },
        head.status,
        size.map_or("unknown".to_owned(), disk::human),
    ));
    // Credentials stay with the host they were given for, with the cookies pages set on the way
    // (`get`); the first request's own range (`request_cap`) isn't theirs.
    let mut headers = headers;
    headers.remove(header::RANGE);
    // A POST stays one if its redirects kept it (307, 308); it is sent again only once the
    // server has answered a range (206): `Accept-Ranges` alone isn't proof, and a form may do
    // something each time it is posted (an export, an order).
    let post = post_for(&opts, as_get.contains(&final_url), &mut headers);
    let wait_for_ranges = (post.is_some() || !advertised) && ranges != RANGES_YES;
    let h = &head.headers;
    // The expected hash: the user's, else what a full (200) answer announces for its content.
    let checksum = opts.checksum.clone().or_else(|| {
        (head.status == StatusCode::OK)
            .then(|| crate::verify::Hash::from_headers(h))
            .flatten()
    });
    let validators = Validators::from_response(
        header_str(h, header::ETAG),
        header_str(h, header::LAST_MODIFIED),
        header_str(h, header::DATE),
        final_url.host_str(),
    );
    // The server's date for the finished file (Options::server_time): its Last-Modified as
    // sent, whether or not it is strong enough to resume by.
    let stamp = opts
        .server_time
        .then(|| header_str(h, header::LAST_MODIFIED))
        .flatten()
        .and_then(|lm| httpdate::parse_http_date(lm).ok());

    let content_type = head
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let (dest, _reservation, file, part) = open_destination(&req.dest, size, &opts, || {
        let cd = header_text(&head.headers, header::CONTENT_DISPOSITION);
        name_for(cd.as_deref(), &final_url, unlocked.as_ref())
    })
    .await?;
    downloading(shared, &dest, size, ranges);
    let file = Arc::new(file);

    let size = if let Some(size) = size {
        let mut planner = Planner::new(size, opts.min_split);
        planner.set_preempt_after(preempt_after);
        *shared.planner.lock().unwrap() = Some(planner);
        let mut saved = ResumeState::new(
            req.url.clone(),
            final_url.clone(),
            dest.clone(),
            size,
            validators.clone(),
        );
        saved.set_part(&part);
        saved.content_type = content_type.clone();
        saved.checksum = checksum.clone();
        saved.final_get = opts.post.is_some() && post.is_none();
        saved.asked_as_mdm = asked_as == Asked::Fallback;
        saved.asked_as_own = asked_as == Asked::Own;
        saved.browser_like = browser.is_some();
        let mirror_sources = mirror_targets(
            &opts,
            checksum.is_some() || opts.pieces.is_some(),
            &req.url,
            &final_url,
            &req.headers,
            &as_get,
            &shared.seen.get_or_init(Default::default).clone(),
        );
        let lanes = lanes_for(
            &opts,
            &final_url,
            &as_get,
            &shared.seen.get_or_init(Default::default).clone(),
            client0.clone(),
            browser.is_some(),
        );
        let ctx = Arc::new(Ctx {
            validators: Mutex::new(validators),
            target: Mutex::new(Target {
                client: client0.clone(),
                url: final_url.clone(),
                headers: headers.clone(),
                post: post.clone(),
                generation: 0,
            }),
            mirror_out: mirror_sources
                .iter()
                .map(|_| AtomicBool::new(false))
                .collect(),
            mirrors: mirror_sources,
            mirror_cursor: AtomicUsize::new(0),
            primary_out: AtomicBool::new(false),
            served: Default::default(),
            // Asked again through DDM's own connection, which the site's bot check refuses: a
            // browser-like download keeps the address it has (#668).
            origin: (req.url != final_url && browser.is_none())
                .then(|| (req.url.clone(), req.headers.clone())),
            refreshed: Default::default(),
            size,
            file: file.clone(),
            opts: opts.clone(),
            shared: shared.clone(),
            // A child token: the supervisor stops its own connections without cancelling the
            // download (which may still fall back to one connection).
            cancel: cancel.child_token(),
            ranges: AtomicU8::new(ranges),
            advertised,
            wait_for_ranges,
            range_answered: Default::default(),
            as_get,
            web_page_ok: expects_web_page(&dest, content_type.as_deref()),
            ftp: None,
            browser: browser.clone(),
            lanes: Arc::new(lanes),
            ttfb,
        });
        match segmented(ctx.clone(), saved.clone(), Some(body)).await {
            Ok(()) => {
                let repair = Repair {
                    ctx: &ctx,
                    saved: &saved,
                    cancel,
                };
                let pieces = opts.pieces.as_ref();
                verify_and_repair(shared, &part, size, checksum.as_ref(), pieces, Some(repair))
                    .await?;
                // Its hold on the part file goes before the rename (`finish` takes it alone).
                drop(ctx);
                return finish(shared, file, &part, &dest, size, stamp);
            }
            // The server ignores ranges and the probe's data can't be kept: start over on
            // one connection.
            Err(Error::NoRanges) => {
                *shared.planner.lock().unwrap() = None;
                shared.view.lock().unwrap().resumable = Some(false);
                let body = match &browser {
                    Some(b) => {
                        let (head, body, _) = b.get(final_url, headers, false, cancel).await?;
                        crate::browserlike::check_first(&head, expects)?;
                        body
                    }
                    None => {
                        let agent = opts.site_agent();
                        let (resp, ..) = get(
                            &client0,
                            final_url,
                            headers,
                            post.as_ref(),
                            agent,
                            expects,
                            cancel,
                        )
                        .await?;
                        Body::Http(resp)
                    }
                };
                single_stream(body, &file, shared, &opts, cancel).await?
            }
            Err(e) => return Err(e),
        }
    } else {
        single_stream(body, &file, shared, &opts, cancel).await?
    };
    // On one connection (no ranges): a bad piece can only be told, not fetched again.
    let pieces = opts.pieces.as_ref();
    verify_and_repair(shared, &part, size, checksum.as_ref(), pieces, None).await?;
    finish(shared, file, &part, &dest, size, stamp)
}

/// Prepare the destination of a new download of `size` bytes (if known), named `name()` in a
/// chosen folder: the folder exists, the file fits on the drive, the name is reserved in this
/// process, and the part file is created (exclusively, for a chosen folder: another program
/// may have taken the name meanwhile) at its full size, and shown to `opts.on_file_created`.
pub(super) async fn open_destination(
    dest: &Destination,
    size: Option<u64>,
    opts: &Options,
    name: impl FnOnce() -> String,
) -> Result<(PathBuf, naming::Reservation, File, PathBuf), Error> {
    // The folder must exist: we create it, but a removed drive or offline share can't be.
    let folder = match dest {
        Destination::File(p) | Destination::Free(p) => {
            p.parent().unwrap_or(Path::new("")).to_owned()
        }
        Destination::Dir(dir) => dir.clone(),
    };
    std::fs::create_dir_all(&folder).map_err(|_| Error::DestinationUnavailable(folder.clone()))?;
    if let Destination::File(p) | Destination::Free(p) = dest
        && !naming::path_fits(p)
    {
        return Err(Error::PathTooLong(p.clone()));
    }
    // Will it fit? Refuse now rather than fail half-way (a drive we can't identify is skipped).
    if let Some(size) = size {
        // Off the async threads, and bounded: a stale network drive can hang the volume probe.
        if let Some(volume) = disk::probe(folder.clone()).await {
            disk::check(&volume, size)?;
        }
        // Options → Save to → Temporary directory: the part file's drive too, when it is
        // another folder (it holds the whole file until the end).
        if let Some(temp) = naming::part_path(&folder.join("x")).parent()
            && temp != folder.as_path()
        {
            std::fs::create_dir_all(temp)
                .map_err(|_| Error::DestinationUnavailable(temp.to_owned()))?;
            if let Some(volume) = disk::probe(temp.to_owned()).await {
                disk::check(&volume, size)?;
            }
        }
    }
    let (dest, reservation, file, part) = match dest {
        Destination::File(p) => {
            let r = naming::reserve(p).ok_or_else(|| Error::InUse(p.clone()))?;
            let part = naming::part_path(p);
            if let Some(dir) = part.parent() {
                std::fs::create_dir_all(dir)
                    .map_err(|_| Error::DestinationUnavailable(dir.to_owned()))?;
            }
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(&part)?;
            (p.clone(), r, file, part)
        }
        Destination::Dir(dir) => {
            let name = name();
            // Shortened to fit Windows' path limit; a folder too deep for any name is an error.
            let name =
                naming::fit_name(dir, &name).ok_or_else(|| Error::PathTooLong(dir.join(&name)))?;
            claim_free(dir, &name)?
        }
        Destination::Free(p) => {
            let dir = p.parent().unwrap_or(Path::new(""));
            let name = p.file_name().unwrap_or_default().to_string_lossy();
            claim_free(dir, &name)?
        }
    };
    // Before any data: a mark the hook sets (Mark of the Web, quarantine) is on the file from
    // its first byte, and the final rename keeps it.
    if let Some(hook) = &opts.on_file_created {
        (hook.0)(&part);
    }
    if let Some(size) = size {
        // Sparse first: writes far into a merely extended file make NTFS zero-fill up to them.
        if size >= sparse::MIN_SIZE {
            sparse::set(&file, true);
        }
        file.set_len(size)?;
    }
    Ok((dest, reservation, file, part))
}

/// `name` in `dir`, or the next free numbered name (`naming::unique_path`): reserved in this
/// process, its part file created exclusively (another program may take the name meanwhile).
/// With the part file's path (`naming::part_path`).
pub(super) fn claim_free(
    dir: &Path,
    name: &str,
) -> Result<(PathBuf, naming::Reservation, File, PathBuf), Error> {
    let mut attempt = 0;
    loop {
        let dest = naming::unique_path(dir, name);
        if let Some(r) = naming::reserve(&dest) {
            let part = naming::part_path(&dest);
            if let Some(folder) = part.parent() {
                std::fs::create_dir_all(folder)
                    .map_err(|_| Error::DestinationUnavailable(folder.to_owned()))?;
            }
            // Read too: reconnecting connections re-read their last bytes (overlap check).
            match std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .open(&part)
            {
                Ok(file) => return Ok((dest, r, file, part)),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists && attempt < 100 => {}
                Err(e) => return Err(e.into()),
            }
        }
        attempt += 1;
    }
}

/// The download has its file and is receiving it.
pub(super) fn downloading(shared: &Shared, dest: &Path, size: Option<u64>, ranges: u8) {
    let mut v = shared.view.lock().unwrap();
    v.state = State::Downloading;
    v.file = Some(dest.to_owned());
    v.size = size;
    v.resumable = (ranges == RANGES_YES)
        .then_some(true)
        .or(size.is_none().then_some(false));
}

/// A new FTP download: as `run_fresh`, with the server's `SIZE` and `MDTM` for the answer's
/// headers and `REST` for ranges.
pub(super) async fn run_fresh_ftp(
    req: Request,
    opts: Options,
    shared: Arc<Shared>,
    cancel: CancellationToken,
) -> Result<Outcome, Error> {
    shared.limiter.set(opts.speed_limit);
    let (shared, cancel) = (&shared, &cancel);
    let site = Arc::new(crate::ftp::Site::new(&req.url, &req.headers, &opts)?);
    // Connection 0 asks about the file and receives it from byte 0 (the probe).
    shared.conn(0, |_| {});
    let ((facts, first), ttfb) = first_try(&opts, shared, cancel, || site.start()).await?;
    let preempt_after = (ttfb * PREEMPT_TTFB).max(PREEMPT_MIN);
    let size = facts.size;
    shared.event(format!(
        "Connected to {} ({}), size {}",
        req.url.host_str().unwrap_or("?"),
        match req.url.scheme() {
            "ftp" => "FTP",
            _ => "FTPS",
        },
        size.map_or("unknown".to_owned(), disk::human),
    ));
    let ranges = if facts.rest {
        RANGES_YES
    } else {
        RANGES_UNKNOWN
    };
    // The modification time stands in for Last-Modified: resume checks it.
    let validators = Validators {
        last_modified: facts.modified.map(httpdate::fmt_http_date),
        ..Validators::default()
    };
    let stamp = opts.server_time.then_some(facts.modified).flatten();
    let (dest, _reservation, file, part) =
        open_destination(&req.dest, size, &opts, || naming::file_name(None, &req.url)).await?;
    downloading(shared, &dest, size, ranges);
    let file = Arc::new(file);
    let first = Body::Ftp(first);
    let size = if let Some(size) = size {
        let mut planner = Planner::new(size, opts.min_split);
        planner.set_preempt_after(preempt_after);
        *shared.planner.lock().unwrap() = Some(planner);
        // No password on disk: whoever resumes gives the login again.
        let url = crate::ftp::without_password(&req.url);
        let mut saved = ResumeState::new(
            url.clone(),
            url.clone(),
            dest.clone(),
            size,
            validators.clone(),
        );
        saved.checksum = opts.checksum.clone();
        saved.set_part(&part);
        // FTP's connections are its own (`ftp`): this client is only kept so Target has one
        // uniform shape with an HTTP target.
        let unused = client(&opts, &req.url, &AsGet::default(), &Default::default())?;
        let ctx = Arc::new(Ctx {
            validators: Mutex::new(validators),
            target: Mutex::new(Target {
                client: unused.clone(),
                url,
                headers: HeaderMap::new(),
                post: None,
                generation: 0,
            }),
            mirrors: Vec::new(),
            mirror_cursor: AtomicUsize::new(0),
            mirror_out: Vec::new(),
            primary_out: AtomicBool::new(false),
            served: Default::default(),
            origin: None,
            refreshed: Default::default(),
            size,
            file: file.clone(),
            opts: opts.clone(),
            shared: shared.clone(),
            cancel: cancel.child_token(),
            ranges: AtomicU8::new(ranges),
            // FTP's REST is asked for, not advertised.
            advertised: true,
            wait_for_ranges: false,
            range_answered: Default::default(),
            as_get: AsGet::default(),
            web_page_ok: true,
            ftp: Some(site.clone()),
            browser: None,
            lanes: Arc::new(Lanes::single(unused)),
            ttfb,
        });
        match segmented(ctx.clone(), saved.clone(), Some(first)).await {
            Ok(()) => {
                let repair = Repair {
                    ctx: &ctx,
                    saved: &saved,
                    cancel,
                };
                let (checksum, pieces) = (opts.checksum.as_ref(), opts.pieces.as_ref());
                verify_and_repair(shared, &part, size, checksum, pieces, Some(repair)).await?;
                // Its hold on the part file goes before the rename (`finish` takes it alone).
                drop(ctx);
                return finish(shared, file, &part, &dest, size, stamp);
            }
            // REST refused: the probe's data can't be kept; the whole file on one connection.
            Err(Error::NoRanges) => {
                *shared.planner.lock().unwrap() = None;
                shared.view.lock().unwrap().resumable = Some(false);
                let t = cancellable(cancel, site.open_at(0, Some(size))).await?;
                single_stream(Body::Ftp(t), &file, shared, &opts, cancel).await?
            }
            Err(e) => return Err(e),
        }
    } else {
        single_stream(first, &file, shared, &opts, cancel).await?
    };
    let (checksum, pieces) = (opts.checksum.as_ref(), opts.pieces.as_ref());
    verify_and_repair(shared, &part, size, checksum, pieces, None).await?;
    finish(shared, file, &part, &dest, size, stamp)
}
