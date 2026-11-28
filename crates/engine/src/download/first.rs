//! A download's first request: redirects followed, a waiting page's refresh, cookies, the
//! browser-like connection when a site refuses a program (#668), and the refusals told apart.

use super::*;

/// What a download's first request needs besides its address, headers and options.
pub(super) struct FirstAsk<'a> {
    pub(super) as_get: &'a AsGet,
    pub(super) seen: &'a Arc<crate::server::ServerSeen>,
    pub(super) expects: bool,
    pub(super) shared: &'a Shared,
    pub(super) cancel: &'a CancellationToken,
}

/// A first request's client, its answer (or why not) and the User-Agent it went with.
pub(super) type FirstAnswer = (
    Client,
    Result<((Response, HeaderMap), Duration), Error>,
    Asked,
);

/// The User-Agent a first request (a download's, a look's) got its answer with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Asked {
    /// The download's own (`Options::user_agent`).
    Given,
    /// `Options::agent_fallback`: the server refused the browser's with 403 (#372).
    Fallback,
    /// DDM's own name (`agent::OWN_USER_AGENT`): the server hung up on the browser's without
    /// answering (`hung_up`).
    Own,
}

impl Asked {
    /// `opts` as the rest of the download asks, with the User-Agent this answer came with. Only
    /// a download whose agent may change (`Options::agent_fallback`): a User-Agent the user set
    /// stays.
    pub(super) fn apply(self, opts: &mut Options) {
        match self {
            Asked::Given => {}
            Asked::Fallback => {
                if let Some(agent) = opts.agent_fallback.take() {
                    opts.user_agent = agent;
                }
            }
            Asked::Own => {
                if opts.agent_fallback.take().is_some() {
                    opts.user_agent = crate::agent::OWN_USER_AGENT.to_owned();
                }
            }
        }
    }
}

impl FirstAsk<'_> {
    /// `first_get` at `url` with its client; asked once more as DDM when the browser's
    /// User-Agent is refused (#372) or hung up on (`hung_up`); `opts` then goes on with that
    /// agent, the third value says which.
    pub(super) async fn at(
        &self,
        opts: &mut Options,
        url: &Url,
        headers: &HeaderMap,
    ) -> Result<FirstAnswer, Error> {
        let (shared, cancel, expects) = (self.shared, self.cancel, self.expects);
        let client0 = client(opts, url, self.as_get, self.seen)?;
        match first_get(&client0, url, headers, opts, expects, shared, cancel).await {
            Err(e) if agent_refused(&e) && opts.agent_fallback.is_some() => {
                // #372: the rest of the download goes as DDM too.
                Asked::Fallback.apply(opts);
                shared.event(
                    "The server refused the browser's User-Agent; asking as Dannex Download Manager",
                );
                let client1 = client(opts, url, self.as_get, self.seen)?;
                let first = first_get(&client1, url, headers, opts, expects, shared, cancel).await;
                Ok((client1, first, Asked::Fallback))
            }
            Err(e) if hung_up(&e) && opts.agent_fallback.is_some() => {
                // The rest of the download goes by DDM's own name too.
                Asked::Own.apply(opts);
                shared.event(
                    "The server hung up on the browser's User-Agent without answering; asking \
                     as Dannex Download Manager",
                );
                let client1 = client(opts, url, self.as_get, self.seen)?;
                let first = first_get(&client1, url, headers, opts, expects, shared, cancel).await;
                Ok((client1, first, Asked::Own))
            }
            first => Ok((client0, first, Asked::Given)),
        }
    }
}

/// Most redirects in a row that pages make themselves (`get`).
pub(super) const MAX_PAGE_REDIRECTS: usize = 5;

/// How much of a web page is read to find its redirect.
pub(super) const PAGE_PEEK: usize = 256 << 10;

/// The longest wait a page asks for before it leads on that is kept.
pub(super) const PAGE_WAIT: Duration = Duration::from_secs(10);

/// A refusal of the address asked for that the browser's own download got past (#630): 401
/// and 403 (a token bound to the browser's session), 404 and 410 (a one-time link the browser
/// already spent, which some servers then forget, others call gone), 412 (FileHippo's answer
/// to whatever isn't a browser). A site's bot check stays the site's call (#376).
pub(super) fn link_refused(e: &Error) -> bool {
    matches!(e, Error::Status(s) if matches!(s.as_u16(), 401 | 403 | 404 | 410 | 412))
}

/// The server closed the connection (or reset it) after the request went, before any answer:
/// what some servers do to a browser's User-Agent on a connection that isn't a browser's
/// (Hetzner's speed servers, measured 2026-11-27: Chrome's, Firefox's and IE 11's names hung
/// up on, curl's and DDM's own served). Not a connection that failed to open (no request went),
/// nor one that went quiet.
pub(super) fn hung_up(e: &Error) -> bool {
    let Error::Http(h) = e else {
        return false;
    };
    if h.is_timeout() || h.is_connect() || h.status().is_some() {
        return false;
    }
    let mut next: Option<&(dyn std::error::Error + 'static)> = Some(h.as_ref());
    while let Some(err) = next {
        if err
            .downcast_ref::<hyper::Error>()
            .is_some_and(|h| h.is_incomplete_message())
        {
            return true;
        }
        if err.downcast_ref::<io::Error>().is_some_and(|io| {
            matches!(
                io.kind(),
                io::ErrorKind::ConnectionReset
                    | io::ErrorKind::ConnectionAborted
                    | io::ErrorKind::UnexpectedEof
            )
        }) {
            return true;
        }
        next = err.source();
    }
    false
}

/// A plain 403, which a browser's User-Agent may have brought on (#372). A site's bot check
/// isn't asked again as DDM: it gets one try the way a browser connects (`browser_first`).
pub(super) fn agent_refused(e: &Error) -> bool {
    matches!(e, Error::Status(s) if s.as_u16() == 403)
}

/// A first answer that refuses DDM's connection for not being a browser's (#668): the site's bot
/// check (`Error::BotCheck`, Cloudflare's), or 412 Precondition Failed, which DDM's first
/// request, sent without any precondition (no `If-…` header), can only get as such a refusal
/// (FileHippo's, #630).
pub(super) fn refused_as_a_program(e: &Error) -> bool {
    matches!(e, Error::BotCheck) || matches!(e, Error::Status(s) if s.as_u16() == 412)
}

/// #668: the first request once more, after the site refused DDM's own connection as a
/// program's (`refused_as_a_program`, its error `refused`): through the browser-like connection
/// (`crate::browserlike`), at the same address with the same headers, once, its answer checked
/// as the first one is. When that fails too, `refused` is what is told (the error users know,
/// with its way out: the browser), the reason in the event log. With the answer, the connection
/// the download goes on with.
pub(super) async fn browser_first(
    opts: &Options,
    url: &Url,
    headers: &HeaderMap,
    refused: Error,
    expects: bool,
    shared: &Shared,
    cancel: &CancellationToken,
) -> Result<
    (
        Arc<crate::browserlike::Browser>,
        Head,
        Body,
        HeaderMap,
        Duration,
    ),
    Error,
> {
    if !crate::browserlike::AVAILABLE {
        return Err(refused);
    }
    let host = url.host_str().unwrap_or("?");
    shared.event(format!(
        "{host}: {refused}; asking once more the way a browser connects"
    ));
    let (b, asked) = match browser_ask(opts, url, headers, expects, cancel).await {
        Ok(got) => got,
        Err(e @ (Error::Cancelled | Error::Adapter(_))) => return Err(e),
        Err(e) => {
            shared.event(format!("{host}, asked the way a browser connects: {e}"));
            return Err(refused);
        }
    };
    let (head, body, headers, took) = asked;
    shared.event(format!(
        "{host}: the browser-like connection got through; downloading on it, one connection"
    ));
    Ok((b, head, body, headers, took))
}

/// The browser-like connection for `url`, and its answer to the first request (`browser_first`,
/// `probe_kept`): the file's, or why not.
pub(super) async fn browser_ask(
    opts: &Options,
    url: &Url,
    headers: &HeaderMap,
    expects: bool,
    cancel: &CancellationToken,
) -> Result<
    (
        Arc<crate::browserlike::Browser>,
        (Head, Body, HeaderMap, Duration),
    ),
    Error,
> {
    let b = Arc::new(crate::browserlike::Browser::new(opts, url, headers)?);
    let asked = std::time::Instant::now();
    let (head, body, headers) = b.get(url.clone(), headers.clone(), false, cancel).await?;
    let took = asked.elapsed();
    crate::browserlike::check_first(&head, expects)?;
    Ok((b, (head, body, headers, took)))
}

/// A plain `GET` (no Range), or a POST of `post`, that must answer 200 or 206. A GET follows
/// the HTTP redirects (the client's policy) and then the redirects a page makes itself (#28):
/// a download site's "your download starts in a moment" page, sent with a `Refresh` header or a
/// `<meta http-equiv="refresh">`, leads on to the file as it does in a browser (`refresh_target`,
/// `meta_refresh`). Its wait, up to `PAGE_WAIT`, is kept, and the cookies it sets go on to its
/// own host. A page that leads on only with a script isn't followed: it stays the answer.
///
/// A site whose links aren't the file's address (`sites`, #34: Google Drive) is asked its own
/// way: its link goes to the file's address first, its pages say where to go next or why it
/// refuses (`Error::Refused`), and a page of its where the file was expected is an error, never
/// the file. So is any web page when the download `expects` another kind of file (#533,
/// `expects_other_file`): a server's refusal, a sign-in wall. A small answer without a type is
/// looked into then, and asked for again if it isn't a page.
///
/// With the answer, `headers` as the download goes on with them at the address it ended at:
/// credentials scoped to it (`scoped_headers`), the pages' cookies added; and how long the last
/// request took to answer (a page's wait and its own request not counted: the time to first byte).
#[allow(clippy::too_many_arguments)]
pub(super) async fn get(
    client: &Client,
    url: Url,
    headers: HeaderMap,
    post: Option<&Bytes>,
    site_agent: bool,
    expects: bool,
    cancel: &CancellationToken,
) -> Result<(Response, HeaderMap, Duration), Error> {
    let (mut url, mut headers) = (url, headers);
    // A site's link is asked at its file's address (a Drive link: Drive's file host), the same
    // party, so the user's cookies go too. A form's POST goes where it was given.
    if post.is_none() {
        if let Some(r) = sites::not_a_file(&url) {
            return Err(Error::Refused(r));
        }
        if let Some(next) = sites::rewrite(&url) {
            headers = scoped_headers(&headers, &url, &next);
            url = next;
        }
    }
    // After a page's redirect, a page read but leading nowhere is asked for again, as is.
    let mut hops = 0;
    // A site's refusal while it is looked at once more (`sites::second_look`), with the address
    // that refused; and whether that look was taken.
    let mut held: Option<(Refusal, Url)> = None;
    let mut looked = false;
    // Redirects between a site's own hosts followed here (`get_policy`).
    let mut party_hops = 0;
    loop {
        let req = build(client, url.clone(), post, &headers, site_agent);
        let asked = std::time::Instant::now();
        let resp = cancellable(cancel, async { Ok(req.send().await?) }).await?;
        let took = asked.elapsed();
        let from = resp.url().clone();
        // A redirect between a site's own hosts, which the client leaves (`get_policy`): followed
        // here, the user's credentials and the cookies it sets going on.
        if post.is_none()
            && resp.status().is_redirection()
            && party_hops < MAX_REDIRECTS
            && let Some(next) = header_str(resp.headers(), header::LOCATION)
                .and_then(|l| from.join(l).ok())
                .filter(|next| sites::same_party(&from, next))
        {
            headers = scoped_headers(&scoped_headers(&headers, &url, &from), &from, &next);
            if !(from.scheme() == "https" && next.scheme() != "https") {
                add_cookies(&mut headers, &set_cookies(resp.headers()));
            }
            url = next;
            party_hops += 1;
            continue;
        }
        // The second look stands only if it leads to the file another way (a Google Doc's
        // editor, whose export follows below); otherwise the refusal was the answer.
        if let Some((refusal, refused_at)) = held.take() {
            let other_way = resp.status() == StatusCode::OK
                && is_web_page(resp.headers())
                && sites::rewrite(&from).is_some_and(|n| n != refused_at);
            if !other_way {
                return Err(Error::Refused(refusal));
            }
        }
        if let Some(r) = sites::refused(&url, &from, resp.status().as_u16()) {
            return Err(Error::Refused(r));
        }
        match resp.status() {
            StatusCode::OK | StatusCode::PARTIAL_CONTENT => {}
            s if s == StatusCode::TOO_MANY_REQUESTS || s == StatusCode::SERVICE_UNAVAILABLE => {
                return Err(Error::Throttled(s, retry_after(resp.headers())));
            }
            s if is_bot_check(s, resp.headers()) => return Err(Error::BotCheck),
            s => return Err(Error::Status(s)),
        }
        // Where the HTTP redirects ended: the headers' credentials follow as reqwest's did.
        headers = scoped_headers(&headers, &url, &from);
        // A site's page, without a file name, where its file was expected: the site talking
        // (a page it changed, a wall), never the file.
        let site_page = post.is_none()
            && resp.status() == StatusCode::OK
            && is_web_page(resp.headers())
            && !named(resp.headers())
            && (sites::expects_file(&from) || expects);
        // No type said where another kind of file is expected (#533): small, it is looked into.
        let untyped = post.is_none()
            && expects
            && hops < MAX_PAGE_REDIRECTS
            && resp.status() == StatusCode::OK
            && !resp.headers().contains_key(header::CONTENT_TYPE)
            && !named(resp.headers())
            && resp.content_length().is_none_or(|n| n <= PAGE_PEEK as u64);
        // A form is never posted again, and a page's own redirects end somewhere.
        if post.is_some() || resp.status() != StatusCode::OK || hops >= MAX_PAGE_REDIRECTS {
            if site_page {
                return Err(Error::GotWebPage);
            }
            return Ok((resp, headers, took));
        }
        let cookies = set_cookies(resp.headers());
        let refresh =
            header_text(resp.headers(), header::REFRESH).and_then(|v| refresh_target(&v, &from));
        // A site's link reached through redirects (a short link to a Drive file; a Google Doc's
        // editor after Drive's `open?id=`): its file's address.
        let site_link = is_web_page(resp.headers())
            .then(|| sites::rewrite(&from))
            .flatten();
        let (wait, next) = if let Some(next) = refresh {
            next
        } else if let Some(next) = site_link {
            (Duration::ZERO, next)
        } else if (is_web_page(resp.headers()) || untyped)
            // A web page small enough to look into (larger ones aren't waiting pages).
            && resp.content_length().is_none_or(|n| n <= PAGE_PEEK as u64)
        {
            let page = read_up_to(resp, PAGE_PEEK, cancel).await?;
            // Without a type and not a page: the file, asked for again as is.
            let site_page = site_page || (untyped && sniffs_as_html(&page));
            if untyped && !site_page {
                add_cookies(&mut headers, &cookies);
                url = from;
                hops = MAX_PAGE_REDIRECTS;
                continue;
            }
            match sites::read_page(&from, &page) {
                PageSays::Next(next) => (Duration::ZERO, next),
                PageSays::Refused(r) => match sites::second_look(&from).filter(|_| !looked) {
                    Some(look) => {
                        looked = true;
                        held = Some((r, from.clone()));
                        (Duration::ZERO, look)
                    }
                    None => return Err(Error::Refused(r)),
                },
                PageSays::Nothing => match meta_refresh(&page, &from) {
                    Some(next) => next,
                    None if site_page => return Err(Error::GotWebPage),
                    // Just a page: asked for again, as its body went into looking.
                    None => {
                        add_cookies(&mut headers, &cookies);
                        url = from;
                        hops = MAX_PAGE_REDIRECTS;
                        continue;
                    }
                },
            }
        } else if site_page {
            return Err(Error::GotWebPage);
        } else {
            return Ok((resp, headers, took));
        };
        if !wait.is_zero() {
            cancellable(cancel, async {
                tokio::time::sleep(wait.min(PAGE_WAIT)).await;
                Ok(())
            })
            .await?;
        }
        headers = scoped_headers(&headers, &from, &next);
        // Its own host's (or its site's, `sites::same_party`), and never from https down to http
        // (as the user's own cookies).
        if (next.host_str() == from.host_str() || sites::same_party(&from, &next))
            && !(from.scheme() == "https" && next.scheme() != "https")
        {
            add_cookies(&mut headers, &cookies);
        }
        url = next;
        hops += 1;
    }
}

/// The first `max` bytes of `resp`'s body (all of it when shorter).
pub(super) async fn read_up_to(
    mut resp: Response,
    max: usize,
    cancel: &CancellationToken,
) -> Result<Vec<u8>, Error> {
    let mut body = Vec::new();
    while body.len() < max {
        let chunk = cancellable(cancel, async { Ok(resp.chunk().await?) }).await?;
        let Some(chunk) = chunk else { break };
        body.extend_from_slice(&chunk);
    }
    body.truncate(max);
    Ok(body)
}

/// Where a page leads on to and after how long, from its `Refresh` value or a meta refresh's
/// `content` (`5; url=next.html`, `0;URL='https://…'`, `3, https://…`), relative to `base`.
/// None unless it leads to another http(s) address: a page that only reloads itself is the
/// answer.
pub(super) fn refresh_target(value: &str, base: &Url) -> Option<(Duration, Url)> {
    let (delay, rest) = value.split_once([';', ','])?;
    let delay = delay
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|d| d.is_finite() && *d >= 0.0)?;
    let rest = rest.trim();
    // `url=` is optional; the address may be quoted.
    let target = match rest.get(..3) {
        Some(u) if u.eq_ignore_ascii_case("url") && rest[3..].trim_start().starts_with('=') => {
            rest[3..].trim_start()[1..].trim()
        }
        _ => rest,
    };
    let target = target
        .strip_prefix(['"', '\''])
        .map_or(target, |t| t.trim_end_matches(['"', '\'']));
    // A meta tag's value is HTML: `&amp;` and the like.
    let target = html::decode(target);
    let next = base.join(target.trim()).ok()?;
    let mut same = next.clone();
    same.set_fragment(base.fragment());
    (matches!(next.scheme(), "http" | "https") && same != *base).then(|| {
        (
            Duration::from_secs_f64(delay.min(PAGE_WAIT.as_secs_f64())),
            next,
        )
    })
}

/// The redirect a page's `<meta http-equiv="refresh" content="…">` makes (`refresh_target`).
pub(super) fn meta_refresh(page: &[u8], base: &Url) -> Option<(Duration, Url)> {
    let page = String::from_utf8_lossy(page);
    html::tags(&page, "meta").find_map(|(_, tag)| {
        html::attribute(tag, "http-equiv")
            .is_some_and(|v| v.trim().eq_ignore_ascii_case("refresh"))
            .then(|| html::attribute(tag, "content"))
            .flatten()
            .and_then(|c| refresh_target(&c, base))
    })
}

/// The `name=value` pairs a response's `Set-Cookie` headers set.
pub(crate) fn set_cookies(h: &HeaderMap) -> Vec<String> {
    h.get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .filter_map(|v| v.split(';').next())
        .map(str::trim)
        .filter(|c| c.contains('='))
        .map(str::to_owned)
        .collect()
}

/// `cookies` added to the request's `Cookie` header, each replacing one of the same name.
pub(crate) fn add_cookies(headers: &mut HeaderMap, cookies: &[String]) {
    if cookies.is_empty() {
        return;
    }
    let name = |c: &str| c.split('=').next().unwrap_or_default().trim().to_owned();
    let new: HashSet<String> = cookies.iter().map(|c| name(c)).collect();
    let mut all: Vec<String> = header_str(headers, header::COOKIE)
        .unwrap_or_default()
        .split(';')
        .map(str::trim)
        .filter(|c| !c.is_empty() && !new.contains(&name(c)))
        .map(str::to_owned)
        .collect();
    all.extend(cookies.iter().cloned());
    if let Ok(v) = HeaderValue::from_str(&all.join("; ")) {
        headers.insert(header::COOKIE, v);
    }
}

/// The first request of a download, retried as a connection is (`retry_wait`) when it fails
/// in a way a later try may not: a server busy at the moment the download starts no longer
/// fails the whole download. With the answer (and the headers to go on with, `get`), how long its
/// last request waited for it: a waiting page's delay isn't the server's time to first byte.
pub(super) async fn first_get(
    client: &Client,
    url: &Url,
    headers: &HeaderMap,
    opts: &Options,
    expects: bool,
    shared: &Shared,
    cancel: &CancellationToken,
) -> Result<((Response, HeaderMap), Duration), Error> {
    let ((resp, headers, took), _) = first_try(opts, shared, cancel, || {
        get(
            client,
            url.clone(),
            headers.clone(),
            opts.post.as_ref(),
            opts.site_agent(),
            expects,
            cancel,
        )
    })
    .await?;
    Ok(((resp, headers), took))
}

/// `attempt` retried as `first_get` does; with its result, how long the try that got it took.
pub(super) async fn first_try<T, F, Fut>(
    opts: &Options,
    shared: &Shared,
    cancel: &CancellationToken,
    mut attempt: F,
) -> Result<(T, Duration), Error>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, Error>>,
{
    let mut failures = 0;
    loop {
        let asked = std::time::Instant::now();
        match cancellable(cancel, attempt()).await {
            Ok(got) => return Ok((got, asked.elapsed())),
            // Asked again at once with another User-Agent (`FirstAsk::at`), not this one again.
            Err(e) if hung_up(&e) && opts.agent_fallback.is_some() => return Err(e),
            Err(e) if e.transient() && failures < opts.max_retries => {
                failures += 1;
                shared.event(format!("Connection 1: {e}; retrying"));
                shared.conn(0, |c| c.status = ConnStatus::Retrying(e.clone()));
                if let Some(delay) = retry_wait(opts, failures, &e) {
                    cancellable(cancel, async {
                        tokio::time::sleep(delay).await;
                        Ok(())
                    })
                    .await?;
                }
            }
            Err(e) => return Err(e),
        }
    }
}
