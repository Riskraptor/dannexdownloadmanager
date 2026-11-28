//! A server that hangs up on a browser's User-Agent without answering (as Hetzner's speed
//! servers do to one on a connection that isn't a browser's): the download asks once more by
//! DDM's own name, and goes on with it.

use std::sync::{Arc, Mutex};

use mdm_engine::{Destination, Download, Options, Request};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Serves `body` (HTTP/1.1, no ranges) to any request whose User-Agent doesn't start with
/// "Mozilla/"; closes the connection unanswered otherwise. Every User-Agent asked with goes in
/// `agents`.
async fn hangs_up_on_browsers(body: &'static [u8], agents: Arc<Mutex<Vec<String>>>) -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut tcp, _)) = listener.accept().await else {
                return;
            };
            let agents = agents.clone();
            tokio::spawn(async move {
                let mut head = Vec::new();
                let mut buf = [0u8; 1024];
                while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                    match tcp.read(&mut buf).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => head.extend_from_slice(&buf[..n]),
                    }
                }
                let head = String::from_utf8_lossy(&head).to_string();
                let agent = head
                    .lines()
                    .find_map(|l| {
                        l.split_once(':')
                            .filter(|(k, _)| k.eq_ignore_ascii_case("user-agent"))
                            .map(|(_, v)| v.trim().to_owned())
                    })
                    .unwrap_or_default();
                agents.lock().unwrap().push(agent.clone());
                if agent.starts_with("Mozilla/") {
                    return; // hung up: no answer at all
                }
                let answer = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\n\r\n",
                    body.len()
                );
                let _ = tcp.write_all(answer.as_bytes()).await;
                let _ = tcp.write_all(body).await;
                let _ = tcp.shutdown().await;
            });
        }
    });
    port
}

#[tokio::test]
async fn a_server_that_hangs_up_on_a_browser_is_asked_by_ddms_own_name() {
    static BODY: &[u8] = b"the file's bytes, served to a program's name only";
    let agents = Arc::new(Mutex::new(Vec::new()));
    let port = hangs_up_on_browsers(BODY, agents.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let out = Download::start(
        Request {
            url: format!("http://127.0.0.1:{port}/file.bin").parse().unwrap(),
            headers: Default::default(),
            dest: Destination::Dir(dir.path().to_owned()),
        },
        Options {
            // A browser's User-Agent, and IE 11's to fall back on (both "Mozilla/…": hung up on).
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/131.0 Safari/537.36"
                .to_owned(),
            agent_fallback: Some(mdm_engine::agent::IE11_USER_AGENT.to_owned()),
            retry_delay: std::time::Duration::from_millis(50),
            ..Options::default()
        },
    )
    .wait()
    .await
    .expect("asked by DDM's own name, the server answers");
    assert_eq!(std::fs::read(&out.path).unwrap(), BODY);
    let agents = agents.lock().unwrap().clone();
    assert!(agents[0].starts_with("Mozilla/5.0 (Windows"), "{agents:?}");
    assert!(
        agents[1..]
            .iter()
            .all(|a| a == mdm_engine::agent::OWN_USER_AGENT),
        "after the hang-up, only DDM's own name: {agents:?}"
    );
    // Hung up on once, then asked again at once, not after the retries a broken network gets.
    assert_eq!(agents.len(), 2, "{agents:?}");
}

#[tokio::test]
async fn a_user_agent_the_user_set_is_kept_even_when_hung_up_on() {
    static BODY: &[u8] = b"never served";
    let agents = Arc::new(Mutex::new(Vec::new()));
    let port = hangs_up_on_browsers(BODY, agents.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = Options {
        max_retries: 1,
        retry_delay: std::time::Duration::from_millis(10),
        ..Options::default()
    };
    // Set by the user: no fallback (the app gives none then).
    opts.speak_as("Mozilla/5.0 (my own)".to_owned());
    opts.agent_fallback = None;
    let failed = Download::start(
        Request {
            url: format!("http://127.0.0.1:{port}/file.bin").parse().unwrap(),
            headers: Default::default(),
            dest: Destination::Dir(dir.path().to_owned()),
        },
        opts,
    )
    .wait()
    .await;
    assert!(failed.is_err(), "{failed:?}");
    let agents = agents.lock().unwrap().clone();
    assert!(
        agents.iter().all(|a| a == "Mozilla/5.0 (my own)"),
        "{agents:?}"
    );
}
