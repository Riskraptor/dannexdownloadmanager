//! The HTTP clients: one per connection (a lane, `Lanes`), each with its own DNS, proxy and route,
//! the redirect policy that keeps a scoped header to its host, and the requests built with the
//! download's headers.

use super::*;

/// The addresses a POST download's redirects turned into GETs (301, 302, 303, as browsers
/// do), recorded by its client's redirect policy: requests to them afterwards are GETs too.
#[derive(Clone, Default)]
pub(super) struct AsGet(pub(super) Arc<Mutex<HashSet<Url>>>);

impl AsGet {
    pub(super) fn contains(&self, url: &Url) -> bool {
        self.0.lock().unwrap().contains(url)
    }

    /// Following up to `MAX_REDIRECTS`, noting the method each hop leaves with.
    pub(super) fn policy(&self) -> reqwest::redirect::Policy {
        let seen = self.0.clone();
        reqwest::redirect::Policy::custom(move |attempt| {
            let mut seen = seen.lock().unwrap();
            // `previous` ends with the address that redirected; the first is the one asked.
            let came_as_get = attempt.previous().len() > 1
                && attempt.previous().last().is_some_and(|u| seen.contains(u));
            if came_as_get || matches!(attempt.status().as_u16(), 301..=303) {
                seen.insert(attempt.url().clone());
            } else {
                seen.remove(attempt.url());
            }
            drop(seen);
            reqwest::redirect::Policy::limited(MAX_REDIRECTS).redirect(attempt)
        })
    }
}

/// Most HTTP redirects (301, 302, 303, 307, 308) a request follows, as browsers do (20; reqwest
/// stops at 10): download links often go through a tracker, a mirror picker and a CDN.
pub(super) const MAX_REDIRECTS: usize = 20;

/// After asking the original address didn't help, how long until it may be asked again.
pub(super) const REFRESH_EVERY: Duration = Duration::from_secs(120);

/// One HTTP client per connection (its lane), so that each connection of a download is a TCP
/// connection of its own, whatever the protocol. hyper's pool carries every request to a host
/// over one shared HTTP/2 connection: with one client for the whole download, an HTTP/2 server
/// (GitHub's, Google's, OVH's...) got all of a download's connections as streams of a single TCP
/// connection, which went at one connection's speed (measured 2026-11-27: a lab server capping
/// each TCP connection at 1 MB/s took 8.2 s for 8 MiB on one connection, 1.3 s on eight; on a
/// real line, 8 streams on one connection made 0.8-2.0 MiB/s where 8 connections made 1.6-2.8).
/// On a connection of its own a lane keeps HTTP/2's one gain for a download: a part cut short by
/// a split ends its stream and the connection, warm, serves the lane's next request, where
/// HTTP/1.1 has to close it and open another (TCP and TLS handshakes, slow start).
///
/// A lane's client has no read timeout of its own: `receive` and `request` time the silence
/// themselves, the waits for the disk left out. reqwest's timer runs from the last chunk read,
/// so a write held up longer than `Options::read_timeout` (a busy or slow disk, NTFS holding
/// writes during a flush) failed every connection whose data was already waiting (measured
/// 2026-11-27: the eight connections of a 1 GiB download cut off together after a 20 s write,
/// and asked again).
///
/// Connection `id` uses lane `id % count`. A lane whose connection was too slow or went silent
/// is renewed (`renew`): its next request opens a new connection, as over HTTP/1.1 (on HTTP/2 a
/// new request would otherwise go to the same stuck connection).
pub(super) struct Lanes {
    pub(super) count: u32,
    pub(super) clients: Mutex<std::collections::HashMap<u32, Client>>,
    /// Makes a lane's client (as `client` would, sharing one resolver); None: every connection
    /// on `single`.
    #[allow(clippy::type_complexity)]
    pub(super) make: Option<Box<dyn Fn() -> Result<Client, Error> + Send + Sync>>,
    pub(super) single: Client,
}

impl Lanes {
    /// Every connection on `client` (FTP, the browser-like connection, a single connection).
    pub(super) fn single(client: Client) -> Lanes {
        Lanes {
            count: 1,
            clients: Default::default(),
            make: None,
            single: client,
        }
    }

    /// `count` lanes for an HTTP download, each made by `make` when first used; `fallback` if
    /// one can't be made.
    pub(super) fn new(
        count: u32,
        fallback: Client,
        make: impl Fn() -> Result<Client, Error> + Send + Sync + 'static,
    ) -> Lanes {
        Lanes {
            count: count.max(1),
            clients: Default::default(),
            make: Some(Box::new(make)),
            single: fallback,
        }
    }

    pub(super) fn lane(&self, id: WorkerId) -> u32 {
        id % self.count
    }

    /// Connection `id`'s client.
    pub(super) fn client(&self, id: WorkerId) -> Client {
        let Some(make) = &self.make else {
            return self.single.clone();
        };
        let lane = self.lane(id);
        let mut clients = self.clients.lock().unwrap();
        if let Some(c) = clients.get(&lane) {
            return c.clone();
        }
        match make() {
            Ok(c) => {
                clients.insert(lane, c.clone());
                c
            }
            // Made once already with the same options (the first request's): not expected.
            Err(_) => self.single.clone(),
        }
    }

    /// Whether the lanes time their own silence (their clients have no read timeout).
    pub(super) fn timed_here(&self) -> bool {
        self.make.is_some()
    }

    /// Connection `id`'s next request opens a new connection: its lane gets a new client (the
    /// old one's connection closes once its last answer is dropped).
    pub(super) fn renew(&self, id: WorkerId) {
        let Some(make) = &self.make else { return };
        if let Ok(c) = make() {
            self.clients.lock().unwrap().insert(self.lane(id), c);
        }
    }
}

/// How long an idle keep-alive socket is kept: shorter than most servers' keep-alive, so a
/// paused download doesn't come back to sockets the server already closed.
pub(super) const POOL_IDLE: Duration = Duration::from_secs(15);

/// The request headers to send to `to` when they were given for `from`: credentials
/// (cookies, authorization) never follow a redirect to another host or from https down to
/// http — as reqwest does while it follows the redirect itself — except between one site's own
/// hosts (`sites::same_party`: a Drive link's cookies go to Drive's file host).
pub(crate) fn scoped_headers(headers: &HeaderMap, from: &Url, to: &Url) -> HeaderMap {
    let mut h = headers.clone();
    let same_host = (from.host_str() == to.host_str()
        && from.port_or_known_default() == to.port_or_known_default())
        || sites::same_party(from, to);
    let downgrade = from.scheme() == "https" && to.scheme() != "https";
    if !same_host || downgrade {
        for name in [
            header::COOKIE,
            header::AUTHORIZATION,
            header::PROXY_AUTHORIZATION,
        ] {
            h.remove(name);
        }
    }
    h
}

/// A GET's redirects, followed up to `MAX_REDIRECTS`, except one between two of a site's own
/// hosts (`sites::same_party`: Drive's `open?id=` to Docs): reqwest would drop the user's cookies
/// there, so it is left to `get`, which follows it with them.
pub(super) fn get_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        let party_hop = attempt.previous().last().is_some_and(|from| {
            from.host_str() != attempt.url().host_str() && sites::same_party(from, attempt.url())
        });
        if party_hop {
            attempt.stop()
        } else {
            reqwest::redirect::Policy::limited(MAX_REDIRECTS).redirect(attempt)
        }
    })
}

/// The body a POST download sends to an address reached with or without a redirect to a GET
/// (`as_get`); for a GET, the body's headers go too, as browsers drop them. None: a GET.
pub(super) fn post_for(opts: &Options, as_get: bool, headers: &mut HeaderMap) -> Option<Bytes> {
    let body = opts.post.clone()?;
    if !as_get {
        return Some(body);
    }
    for name in [
        header::CONTENT_TYPE,
        header::CONTENT_LENGTH,
        header::CONTENT_ENCODING,
    ] {
        headers.remove(name);
    }
    None
}

/// A request for `url` with `headers`: a POST with `post` as its body, else a GET. The client
/// sends the download's User-Agent (`Options::user_agent`); a site that wants a browser's gets
/// it instead when `site_agent` (`Options::site_agent`: the agent is DDM's automatic one).
pub(super) fn build(
    client: &Client,
    url: Url,
    post: Option<&Bytes>,
    headers: &HeaderMap,
    site_agent: bool,
) -> RequestBuilder {
    let mut headers = headers.clone();
    if let Some(site) = request_agent(site_agent, &url)
        && !headers.contains_key(header::USER_AGENT)
    {
        headers.insert(header::USER_AGENT, HeaderValue::from_static(site));
    }
    let req = match post {
        Some(body) => client.post(url).body(body.clone()),
        None => client.get(url),
    };
    req.headers(headers)
}

/// The User-Agent a request to `url` carries instead of the download's own: a site's browser
/// agent (`sites::user_agent`: Google Drive's hosts), only when `site_agent` (the download's is
/// DDM's automatic one, `Options::site_agent`). A browser's agent handed over with the download,
/// or one the user set, always wins.
pub(super) fn request_agent(site_agent: bool, url: &Url) -> Option<&'static str> {
    site_agent.then(|| sites::user_agent(url)).flatten()
}

/// A header's text: UTF-8, else ISO-8859-1 (servers send file names either way, while
/// `HeaderValue::to_str` takes ASCII only).
pub(super) fn header_text(h: &HeaderMap, name: header::HeaderName) -> Option<String> {
    let raw = h.get(name)?.as_bytes();
    Some(match std::str::from_utf8(raw) {
        Ok(s) => s.to_owned(),
        Err(_) => raw.iter().map(|&b| char::from(b)).collect(),
    })
}

/// A client for requests to `url`'s server that aren't downloads (a debrid service's API, #44):
/// the same proxy, DNS and network adapter as `opts`' downloads.
pub fn http_client(opts: &Options, url: &Url) -> Result<Client, Error> {
    client(opts, url, &AsGet::default(), &Default::default())
}

/// A client for one request to `url`'s server, as [`http_client`]'s, that follows no redirect
/// and adds no `Referer` of its own: its caller follows each hop itself, with a client for that
/// hop's server (its proxy and adapter checks are that server's), and decides which of its
/// headers go there (the browser's quality menu reading a stream's manifest, mdm-tools'
/// `manifest`: the browser's cookie never reaches another server).
pub fn hop_client(opts: &Options, url: &Url) -> Result<Client, Error> {
    client_with(
        opts,
        url,
        &AsGet::default(),
        &Default::default(),
        false,
        None,
    )
}

/// The client for a download of `url`; a POST download's redirects are noted in `as_get`.
pub(super) fn client(
    opts: &Options,
    url: &Url,
    as_get: &AsGet,
    seen: &Arc<crate::server::ServerSeen>,
) -> Result<Client, Error> {
    client_with(opts, url, as_get, seen, true, None)
}

/// The resolver a client for `url` asks. A download's lanes (`Lanes`) share one, so that a DNS
/// chosen in Options (over HTTPS, over TLS) is set up and asked once for all its connections.
pub(super) fn resolver_for(opts: &Options, url: &Url) -> Arc<dns::Resolver> {
    let port = url.port_or_known_default().unwrap_or(443);
    Arc::new(dns::Resolver::new(
        &opts.hosts,
        port,
        &opts.dns,
        &opts.extra_roots,
        &opts.route,
    ))
}

/// The lanes of a download whose ranges come from `url` (`Lanes`): clients made as `first` (the
/// first request's) was, sharing a resolver, without a read timeout of their own. One lane only,
/// on `first`, for the browser-like connection or a single connection.
pub(super) fn lanes_for(
    opts: &Options,
    url: &Url,
    as_get: &AsGet,
    seen: &Arc<crate::server::ServerSeen>,
    first: Client,
    browser_like: bool,
) -> Lanes {
    if browser_like || opts.connections() == 1 {
        return Lanes::single(first);
    }
    let (opts, url, as_get, seen) = (opts.clone(), url.clone(), as_get.clone(), seen.clone());
    let resolver = resolver_for(&opts, &url);
    Lanes::new(opts.connections(), first, move || {
        client_with(&opts, &url, &as_get, &seen, true, Some(resolver.clone()))
    })
}

/// [`client`], following redirects (`follow`) or none at all ([`hop_client`]). `lane`: for a
/// download's lane (`Lanes`), the resolver its lanes share, and no read timeout (the lane times
/// its silence itself); else a resolver of its own (`resolver_for`) and `Options::read_timeout`.
pub(super) fn client_with(
    opts: &Options,
    url: &Url,
    as_get: &AsGet,
    seen: &Arc<crate::server::ServerSeen>,
    follow: bool,
    lane: Option<Arc<dns::Resolver>>,
) -> Result<Client, Error> {
    let route = &opts.route;
    // An IP address in the URL never reaches the resolver, which drops what the chosen adapter
    // can't reach (#35): check it here.
    net::check_url(route, url)?;
    let mut builder = Client::builder();
    if !opts.extra_roots.is_empty() {
        builder = builder.tls_certs_merge(
            opts.extra_roots
                .iter()
                .filter_map(|der| reqwest::Certificate::from_der(der).ok()),
        );
    }
    // With an adapter chosen, the system's proxy is looked up now, so that it can be checked
    // (and a proxy on this computer recognised) like the others.
    let system = match (&opts.proxy, route.is_any()) {
        (Proxy::System, false) => Some(net::system_proxy(url)),
        _ => None,
    };
    // The proxy that carries every request of this client, when it's on this computer: such a
    // connection isn't bound; where the proxy sends the traffic is up to it.
    let mut through_local_proxy = false;
    let mut check_proxy = |proxy: &str, covers_all: bool| -> Result<(), Error> {
        let host = Url::parse(proxy)
            .ok()
            .and_then(|u| u.host_str().map(str::to_owned))
            .unwrap_or_default();
        if net::is_this_computer(&host) {
            through_local_proxy |= covers_all;
        } else if let Ok(ip) = host.trim_matches(['[', ']']).parse() {
            route.check(ip)?;
        }
        Ok(())
    };
    // Whether a proxy carries this download (#140: the address that answers is then the
    // proxy's): known here for each kind of setting.
    let mut proxied = false;
    let builder = match (&opts.proxy, system) {
        (Proxy::None, _) => builder.no_proxy(),
        (Proxy::System, Some(Some(proxy))) => {
            check_proxy(&proxy, true)?;
            proxied = true;
            builder.no_proxy().proxy(reqwest::Proxy::all(&proxy)?)
        }
        (Proxy::System, Some(None)) => builder.no_proxy(),
        // reqwest reads the system's proxy by default.
        (Proxy::System, None) => {
            proxied = net::system_proxy(url).is_some();
            builder
        }
        (
            Proxy::Manual {
                url: proxy,
                login,
                http,
                https,
            },
            _,
        ) => {
            let with_login = |p: reqwest::Proxy| match login {
                Some((user, pass)) => p.basic_auth(user, pass),
                None => p,
            };
            if *http || *https {
                check_proxy(proxy, *http && *https)?;
            }
            proxied = if url.scheme() == "https" {
                *https
            } else {
                *http
            };
            let mut b = builder.no_proxy();
            if *http {
                b = b.proxy(with_login(reqwest::Proxy::http(proxy.as_str())?));
            }
            if *https {
                b = b.proxy(with_login(reqwest::Proxy::https(proxy.as_str())?));
            }
            b
        }
        // The script's first route DDM can use: a proxy, or straight to the server.
        (Proxy::Pac { script }, _) => {
            let first = crate::pac::routes(script, url).ok().and_then(|routes| {
                routes.into_iter().find_map(|r| match r {
                    crate::pac::Route::Direct => Some(None),
                    crate::pac::Route::Proxy(p) => reqwest::Proxy::all(&p).ok().map(|_| Some(p)),
                })
            });
            // No usable answer: the system's proxy, as reqwest reads it (with an adapter
            // chosen, looked up now to be checked).
            let first = match first {
                None if !route.is_any() => Some(net::system_proxy(url)),
                other => other,
            };
            match first {
                Some(Some(p)) => {
                    check_proxy(&p, true)?;
                    proxied = true;
                    builder.no_proxy().proxy(reqwest::Proxy::all(&p)?)
                }
                Some(None) => builder.no_proxy(),
                None => {
                    proxied = net::system_proxy(url).is_some();
                    builder
                }
            }
        }
    };
    // A connection to this computer isn't bound (#35); its redirects may then only lead back
    // to this computer, or they would leave unbound.
    let to_this_computer = through_local_proxy
        || (net::is_this_computer(url.host_str().unwrap_or_default())
            && !matches!(opts.proxy, Proxy::Manual { .. } | Proxy::Pac { .. }));
    let builder = net::bind_client(builder, route, to_this_computer)?;
    let inner = match opts.post {
        Some(_) => as_get.policy(),
        None => get_policy(),
    };
    let builder = if !follow {
        builder
            .redirect(reqwest::redirect::Policy::none())
            .referer(false)
    } else if route.is_any() {
        builder.redirect(inner)
    } else {
        let route = route.clone();
        let stay_here = to_this_computer && !through_local_proxy;
        builder.redirect(reqwest::redirect::Policy::custom(move |attempt| {
            let refused = net::check_url(&route, attempt.url()).err().or_else(|| {
                (stay_here && !net::is_this_computer(attempt.url().host_str().unwrap_or(""))).then(
                    || net::NetError::Unbound {
                        label: route.label().unwrap_or_default().to_owned(),
                    },
                )
            });
            match refused {
                Some(e) => attempt.error(e),
                None => inner.redirect(attempt),
            }
        }))
    };
    seen.set_proxied(proxied);
    let builder = match lane {
        Some(_) => builder,
        None => builder.read_timeout(opts.read_timeout),
    };
    Ok(builder
        // Each connection's opening timed (#140): the download's own, no extra probe.
        .connector_layer(crate::server::Timed(seen.clone()))
        .dns_resolver(lane.unwrap_or_else(|| resolver_for(opts, url)))
        // Let HTTPS servers negotiate HTTP/2 by ALPN.  HTTP/1.1 remains the fallback for
        // older servers and plain HTTP.  An adaptive receive window keeps parallel ranges from
        // being throttled by HTTP/2's conservative default flow-control window.
        .http2_adaptive_window(true)
        .user_agent(&opts.user_agent)
        .connect_timeout(opts.connect_timeout)
        .pool_max_idle_per_host(opts.connections() as usize)
        .pool_idle_timeout(POOL_IDLE)
        .tcp_nodelay(true)
        .build()?)
}
