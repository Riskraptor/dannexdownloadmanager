//! A connection whose writes go at most so many bytes a second (`Config::conn_rate`): a path,
//! or a server, that caps each TCP connection whatever it carries, so that all of an HTTP/2
//! connection's streams share the one cap while separate connections get one each.

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll, ready};
use std::time::{Duration, Instant};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// `inner`, its writes paced by a token bucket of `rate` bytes/s holding at most a tenth of a
/// second's worth (an idle connection doesn't save up a burst); unpaced without a rate.
pub(crate) struct Paced<S> {
    inner: S,
    rate: Option<u64>,
    tokens: f64,
    last: Instant,
    wait: Option<Pin<Box<tokio::time::Sleep>>>,
}

impl<S> Paced<S> {
    pub(crate) fn new(inner: S, rate: Option<u64>) -> Self {
        Paced {
            inner,
            rate,
            tokens: 0.0,
            last: Instant::now(),
            wait: None,
        }
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for Paced<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_read(cx, buf)
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for Paced<S> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        let Some(rate) = this.rate else {
            return Pin::new(&mut this.inner).poll_write(cx, buf);
        };
        loop {
            if let Some(wait) = &mut this.wait {
                ready!(wait.as_mut().poll(cx));
                this.wait = None;
            }
            let now = Instant::now();
            let depth = (rate as f64 / 10.0).max(1024.0);
            this.tokens = (this.tokens + (now - this.last).as_secs_f64() * rate as f64).min(depth);
            this.last = now;
            if this.tokens < 1024.0_f64.min(buf.len() as f64) {
                let short = 1024.0_f64.min(buf.len() as f64) - this.tokens;
                let wait =
                    Duration::from_secs_f64(short / rate as f64).max(Duration::from_millis(1));
                this.wait = Some(Box::pin(tokio::time::sleep(wait)));
                continue;
            }
            let n = buf.len().min(this.tokens as usize).max(1);
            let written = ready!(Pin::new(&mut this.inner).poll_write(cx, &buf[..n]))?;
            this.tokens -= written as f64;
            return Poll::Ready(Ok(written));
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn writes_go_at_the_rate() {
        let (a, mut b) = tokio::io::duplex(1 << 20);
        let mut paced = Paced::new(a, Some(400_000));
        let reader = tokio::spawn(async move {
            let mut sink = Vec::new();
            tokio::io::AsyncReadExt::read_to_end(&mut b, &mut sink)
                .await
                .unwrap();
            sink.len()
        });
        let started = Instant::now();
        paced.write_all(&vec![7u8; 200_000]).await.unwrap();
        paced.shutdown().await.unwrap();
        drop(paced);
        assert_eq!(reader.await.unwrap(), 200_000);
        let took = started.elapsed();
        // 200 KB at 400 KB/s: half a second (less the first tenth's bucket, more a scheduler's).
        assert!(took >= Duration::from_millis(350), "{took:?}");
        assert!(took < Duration::from_secs(3), "{took:?}");
    }
}
