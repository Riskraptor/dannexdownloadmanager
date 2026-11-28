//! The probe before a download: size, ranges, validators and name, an answer kept for the download
//! that follows (`KeptAnswer`), and a link a debrid account unlocks.

use super::*;

/// What a quick look at a URL tells before downloading (for "Download File Info").
#[derive(Clone, Debug)]
pub struct ProbeInfo {
    pub final_url: Url,
    pub size: Option<u64>,
    /// Suggested local name (Content-Disposition, else URL).
    pub file_name: String,
    /// Where the server is, as the look saw it (#140).
    pub server: Option<crate::Server>,
}

/// The open answer of a look (`probe_kept`), for the download that follows it to read (#28): a
/// one-time link, or a host that allows one download at a time, refuses the download asking
/// again. Dropping it closes the connection.
pub struct KeptAnswer {
    /// The address asked: only a download of the same address takes it.
    pub(super) url: Url,
    pub(super) client: Client,
    pub(super) as_get: AsGet,
    pub(super) head: Head,
    pub(super) body: Body,
    /// The browser-like connection it came through, when only that one got past the site's
    /// bot check (#668): the download goes on with it.
    pub(super) browser: Option<Arc<crate::browserlike::Browser>>,
    pub(super) headers: HeaderMap,
    pub(super) took: Duration,
    pub(super) at: std::time::Instant,
    /// The service's direct link it was asked at (#44), with its connection cap and name.
    pub(super) unlocked: Option<crate::unlock::Unlocked>,
    /// What its client learnt of the server (#140): the download goes on with it.
    pub(super) seen: Arc<crate::server::ServerSeen>,
    /// The User-Agent it was asked with: the download's other connections ask with it too.
    pub(super) asked: Asked,
}

impl std::fmt::Debug for KeptAnswer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeptAnswer")
            .field("url", &self.url.as_str())
            .field("status", &self.head.status)
            .field("browser_like", &self.browser.is_some())
            .finish_non_exhaustive()
    }
}

/// How long a kept answer is worth taking: a server ends an idle connection after a while,
/// and the download then asks again (as when it isn't kept).
pub(super) const KEEP_ANSWER_FOR: Duration = Duration::from_secs(15 * 60);

/// One plain `GET` (or `Options::post`'s POST) to learn the final URL, size and name, through
/// the redirects `get` follows (a waiting page's look reads up to `PAGE_PEEK` of it and keeps its
/// wait); the file's own body is not read.
pub async fn probe(url: Url, headers: HeaderMap, opts: &Options) -> Result<ProbeInfo, Error> {
    probe_kept(url, headers, opts).await.map(|(info, _)| info)
}

/// `probe`, keeping its answer open when its body is the file from byte 0 (200, or 206 from
/// 0), for `Download::start_with`. Not for FTP, whose look is a separate command.
pub async fn probe_kept(
    url: Url,
    headers: HeaderMap,
    opts: &Options,
) -> Result<(ProbeInfo, Option<KeptAnswer>), Error> {
    if crate::ftp::is_ftp(&url) {
        let site = crate::ftp::Site::new(&url, &headers, opts)?;
        let facts = site.check().await?;
        return Ok((
            ProbeInfo {
                file_name: naming::file_name(None, &url),
                final_url: url,
                size: facts.size,
                server: None,
            },
            None,
        ));
    }
    let as_get = AsGet::default();
    // Through a service account (#44): its direct link is looked at instead, without the host's
    // cookies or logins.
    let unlocked = unlock_origin(opts, &url, None, &CancellationToken::new()).await?;
    let (target, headers) = match &unlocked {
        Some(u) => (u.url.clone(), HeaderMap::new()),
        None => (url.clone(), headers),
    };
    let seen = Arc::new(crate::server::ServerSeen::default());
    let client = client(opts, &target, &as_get, &seen)?;
    let expects = expects_other_file(opts.expected_type.as_deref(), None, &url);
    // Where the look asked, and with what: asked again there if the site refuses (#668).
    let (asked_at, asked_with) = (target.clone(), headers.clone());
    // A browser's User-Agent refused (#372), or hung up on: asked once more as DDM.
    let (client, asked, asked_as) = match get(
        &client,
        target.clone(),
        headers.clone(),
        opts.post.as_ref(),
        opts.site_agent(),
        expects,
        &CancellationToken::new(),
    )
    .await
    {
        Err(e) if (agent_refused(&e) || hung_up(&e)) && opts.agent_fallback.is_some() => {
            let mut plain = opts.clone();
            let asked_as = if hung_up(&e) {
                Asked::Own
            } else {
                Asked::Fallback
            };
            asked_as.apply(&mut plain);
            let client = self::client(&plain, &target, &as_get, &seen)?;
            let asked = get(
                &client,
                target,
                headers,
                opts.post.as_ref(),
                plain.site_agent(),
                expects,
                &CancellationToken::new(),
            )
            .await;
            (client, asked, asked_as)
        }
        asked => (client, asked, Asked::Given),
    };
    let (head, body, browser, headers, took) = match asked {
        Ok((resp, headers, took)) => {
            seen.answered(&resp);
            (Head::of(&resp), Body::Http(resp), None, headers, took)
        }
        // #668: the site refused DDM's connection as a program's (`refused_as_a_program`):
        // looked at once more the way a browser connects, its answer kept for the download
        // (which then asks no more).
        Err(e) if refused_as_a_program(&e) && unlocked.is_none() && opts.post.is_none() => {
            let cancel = CancellationToken::new();
            let (b, (head, body, headers, took)) =
                browser_ask(opts, &asked_at, &asked_with, expects, &cancel)
                    .await
                    .map_err(|other| match other {
                        Error::Cancelled | Error::Adapter(_) => other,
                        _ => e,
                    })?;
            (head, body, Some(b), headers, took)
        }
        Err(e) => return Err(e),
    };
    let final_url = head.url.clone();
    let (size, from_zero) = match head.status {
        StatusCode::PARTIAL_CONTENT => {
            let (start, _, total) = parse_content_range(&head.headers)?;
            (total, start == 0)
        }
        StatusCode::OK => (head.len, true),
        _ => (head.len, false),
    };
    let cd = header_text(&head.headers, header::CONTENT_DISPOSITION);
    let file_name = name_for(cd.as_deref(), &final_url, unlocked.as_ref());
    let info = ProbeInfo {
        final_url,
        size: size.or(unlocked.as_ref().and_then(|u| u.size)),
        file_name,
        server: seen.get(),
    };
    let kept = from_zero.then(|| KeptAnswer {
        url,
        client,
        seen,
        as_get,
        head,
        body,
        browser,
        headers,
        took,
        at: std::time::Instant::now(),
        unlocked,
        asked: asked_as,
    });
    Ok((info, kept))
}

/// The file's name: the server's (Content-Disposition), else the service's (#44), else the
/// address's.
pub(super) fn name_for(
    cd: Option<&str>,
    url: &Url,
    unlocked: Option<&crate::unlock::Unlocked>,
) -> String {
    match (cd, unlocked.and_then(|u| u.file_name.as_deref())) {
        (None, Some(name)) if !name.trim().is_empty() => naming::sanitize(name),
        _ => naming::file_name(cd, url),
    }
}

/// `opts.unlocker`'s direct link for `url` (#44), noted in the event log (never the link
/// itself): None without an unlocker, for a form's POST, or when the service declines the host.
pub(super) async fn unlock_origin(
    opts: &Options,
    url: &Url,
    shared: Option<&Shared>,
    cancel: &CancellationToken,
) -> Result<Option<crate::unlock::Unlocked>, Error> {
    let Some(u) = &opts.unlocker else {
        return Ok(None);
    };
    if opts.post.is_some() || !matches!(url.scheme(), "http" | "https") {
        return Ok(None);
    }
    unlock_noted(u, url, shared, cancel).await.map_err(|fault| {
        Error::Refused(Refusal::Service {
            service: u.name().to_owned(),
            fault,
        })
    })
}

/// `u`'s link for `url`, its waits told in the event log and the list's status ("40% at
/// TorBox") meanwhile, and the link it gave noted (never the link itself).
pub(super) async fn unlock_noted(
    u: &Arc<dyn crate::unlock::Unlocker>,
    url: &Url,
    shared: Option<&Shared>,
    cancel: &CancellationToken,
) -> Result<Option<crate::unlock::Unlocked>, crate::unlock::ServiceFault> {
    let note = |s: String, progress: Option<f32>| {
        if let Some(sh) = shared {
            sh.event(s);
            sh.view.lock().unwrap().at_service = Some((u.name().to_owned(), progress));
        }
    };
    let out = u.unlock(url, &note, cancel).await;
    if let Some(sh) = shared {
        sh.view.lock().unwrap().at_service = None;
        if let Ok(Some(x)) = &out {
            sh.event(u.noted(x));
        }
    }
    out
}
