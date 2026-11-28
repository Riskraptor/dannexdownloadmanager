//! HTTP/2 transport negotiation, tested against an in-process TLS-only h2 server.

use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use bytes::Bytes;
use http_body_util::Full;
use hyper::service::service_fn;
use hyper::{Request, Response, Version};
use hyper_util::rt::{TokioExecutor, TokioIo};
use mdm_engine::Options;

#[tokio::test]
async fn https_downloads_negotiate_http2_when_the_server_offers_it() {
    let certified = rcgen::generate_simple_self_signed(vec!["h2.test".into()]).unwrap();
    let certificate = certified.cert.der().clone();
    let provider = rustls::crypto::aws_lc_rs::default_provider();
    let mut server_config = rustls::ServerConfig::builder_with_provider(Arc::new(provider))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![certificate.clone()], certified.signing_key.into())
        .unwrap();
    server_config.alpn_protocols = vec![b"h2".to_vec()];
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let saw = Arc::new(AtomicU8::new(0));
    let seen = saw.clone();
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let tls = tokio_rustls::TlsAcceptor::from(Arc::new(server_config))
            .accept(tcp)
            .await
            .unwrap();
        let service = service_fn(move |request: Request<hyper::body::Incoming>| {
            let seen = seen.clone();
            async move {
                seen.store(
                    (request.version() == Version::HTTP_2) as u8,
                    Ordering::Relaxed,
                );
                Ok::<_, Infallible>(Response::new(Full::new(Bytes::from_static(b"h2 works"))))
            }
        });
        hyper::server::conn::http2::Builder::new(TokioExecutor::new())
            .serve_connection(TokioIo::new(tls), service)
            .await
            .unwrap();
    });

    let url: url::Url = format!("https://h2.test:{port}/file.bin").parse().unwrap();
    let opts = Options {
        hosts: vec![("h2.test".into(), vec!["127.0.0.1".parse().unwrap()])],
        extra_roots: vec![certificate.as_ref().to_vec()],
        ..Options::default()
    };
    let response = mdm_engine::http_client(&opts, &url)
        .unwrap()
        .get(url)
        .send()
        .await
        .unwrap();
    assert_eq!(response.version(), Version::HTTP_2);
    assert_eq!(
        response.bytes().await.unwrap(),
        Bytes::from_static(b"h2 works")
    );
    server.await.unwrap();
    assert_eq!(saw.load(Ordering::Relaxed), 1, "the server received h2");
}

/// Over HTTP/2 too, each of a download's connections is a TCP connection of its own (the
/// engine's lanes): against a server that caps every TCP connection (1 MB/s here, all of an
/// HTTP/2 connection's streams together) the download gets several times one connection's
/// speed. With one client for the whole download, hyper's pool carried every request as a
/// stream of a single connection: 8 MiB took 8 s at one connection's 1 MB/s.
#[tokio::test]
async fn an_http2_download_opens_a_connection_per_lane() {
    use mdm_engine::{Destination, Download, Request};
    use mdm_labserver::{Config, LabServer, content};
    use std::time::{Duration, Instant};

    let s = LabServer::start(Config {
        tls: true,
        h2: true,
        conn_rate: Some(1 << 20),
        ..Config::default()
    })
    .await
    .unwrap();
    let opts = Options {
        extra_roots: vec![s.ca_der().unwrap()],
        max_connections: 8,
        ..Options::default()
    };
    // The server speaks HTTP/2 to the engine's client.
    let url: url::Url = s.url("/gen/16/h2.bin").parse().unwrap();
    let answer = mdm_engine::http_client(&opts, &url)
        .unwrap()
        .get(url)
        .send()
        .await
        .unwrap();
    assert_eq!(answer.version(), Version::HTTP_2);
    drop(answer);
    s.clear_log();

    let dir = tempfile::tempdir().unwrap();
    let started = Instant::now();
    let out = Download::start(
        Request {
            url: s.url("/gen/8M/lanes.bin?seed=4").parse().unwrap(),
            headers: Default::default(),
            dest: Destination::Dir(dir.path().to_owned()),
        },
        opts,
    )
    .wait()
    .await
    .expect("the download completes");
    let took = started.elapsed();
    let data = std::fs::read(&out.path).unwrap();
    let mut want = vec![0; 8 << 20];
    content::fill(4, 0, &mut want);
    assert!(data == want, "content mismatch");
    tokio::time::timeout(Duration::from_secs(20), s.settled())
        .await
        .expect("every request ends");
    let log = s.log();
    let connections: std::collections::HashSet<_> = log
        .iter()
        .filter(|e| e.path.ends_with("lanes.bin"))
        .map(|e| e.peer)
        .collect();
    assert!(
        connections.len() >= 4,
        "{} TCP connection(s) in {took:?}: {log:#?}",
        connections.len()
    );
    eprintln!("8 MiB on {} TCP connections in {took:?}", connections.len());
    // One connection's cap would take 8 s.
    assert!(took < Duration::from_secs(6), "{took:?}");
}
