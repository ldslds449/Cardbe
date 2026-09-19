use axum::serve::Listener;
use std::{
    future::Future,
    io,
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpListener, TcpStream},
    sync::{OwnedSemaphorePermit, Semaphore},
    time::{Instant, Sleep},
};

const MAX_CONNECTIONS: usize = 32;
// Vite sends heartbeat messages every 30 seconds. Allow idle HMR sockets
// between heartbeats; production only serves short HTTP requests.
const READ_TIMEOUT: Duration = Duration::from_secs(if cfg!(dev) { 60 } else { 2 });
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) struct BoundedListener {
    inner: TcpListener,
    permits: Arc<Semaphore>,
}

impl BoundedListener {
    pub(super) fn new(inner: TcpListener) -> Self {
        Self {
            inner,
            permits: Arc::new(Semaphore::new(MAX_CONNECTIONS)),
        }
    }
}

impl Listener for BoundedListener {
    type Io = BoundedStream;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let (stream, address) = Listener::accept(&mut self.inner).await;
            // Reject excess connections before spawning HTTP tasks. The permit
            // belongs to the IO, so an upgraded WebSocket keeps it until closed.
            if let Ok(permit) = self.permits.clone().try_acquire_owned() {
                return (BoundedStream::new(stream, permit), address);
            }
        }
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        self.inner.local_addr()
    }
}

pub(super) struct BoundedStream {
    inner: TcpStream,
    _permit: OwnedSemaphorePermit,
    read_deadline: Pin<Box<Sleep>>,
    write_deadline: Option<Pin<Box<Sleep>>>,
}

impl BoundedStream {
    fn new(inner: TcpStream, permit: OwnedSemaphorePermit) -> Self {
        Self {
            inner,
            _permit: permit,
            read_deadline: Box::pin(tokio::time::sleep(READ_TIMEOUT)),
            write_deadline: None,
        }
    }

    fn poll_write_timeout(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<usize>> {
        let deadline = self
            .write_deadline
            .get_or_insert_with(|| Box::pin(tokio::time::sleep(WRITE_TIMEOUT)));
        if deadline.as_mut().poll(cx).is_ready() {
            Poll::Ready(Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "LAN response write timed out",
            )))
        } else {
            Poll::Pending
        }
    }
}

impl AsyncRead for BoundedStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let before = buf.filled().len();
        match Pin::new(&mut self.inner).poll_read(cx, buf) {
            Poll::Ready(result) => {
                if buf.filled().len() > before {
                    self.read_deadline
                        .as_mut()
                        .reset(Instant::now() + READ_TIMEOUT);
                }
                Poll::Ready(result)
            }
            Poll::Pending => {
                if self.read_deadline.as_mut().poll(cx).is_ready() {
                    Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "LAN request read timed out",
                    )))
                } else {
                    Poll::Pending
                }
            }
        }
    }
}

impl AsyncWrite for BoundedStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match Pin::new(&mut self.inner).poll_write(cx, buf) {
            Poll::Ready(result) => {
                self.write_deadline = None;
                Poll::Ready(result)
            }
            Poll::Pending => self.poll_write_timeout(cx),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    async fn connection_permit_lasts_until_the_stream_is_dropped() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let mut listener = BoundedListener::new(listener);
        let permits = listener.permits.clone();
        let _client = TcpStream::connect(address).await.unwrap();
        let (stream, _) = listener.accept().await;
        assert_eq!(permits.available_permits(), MAX_CONNECTIONS - 1);
        drop(stream);
        assert_eq!(permits.available_permits(), MAX_CONNECTIONS);
    }

    #[tokio::test]
    async fn excess_connections_are_closed_and_capacity_recovers() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let mut listener = BoundedListener::new(listener);
        let permits = listener.permits.clone();
        let all = permits
            .clone()
            .acquire_many_owned(MAX_CONNECTIONS as u32)
            .await
            .unwrap();
        let accepting = tokio::spawn(async move { listener.accept().await });
        let mut rejected = TcpStream::connect(address).await.unwrap();
        let mut byte = [0];
        let result = tokio::time::timeout(Duration::from_secs(2), rejected.read(&mut byte))
            .await
            .unwrap();
        assert!(matches!(result, Ok(0)) || result.is_err());
        drop(all);
        let _client = TcpStream::connect(address).await.unwrap();
        let (stream, _) = tokio::time::timeout(Duration::from_secs(2), accepting)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(permits.available_permits(), MAX_CONNECTIONS - 1);
        drop(stream);
        assert_eq!(permits.available_permits(), MAX_CONNECTIONS);
    }

    #[tokio::test]
    async fn idle_reads_and_stalled_writes_time_out() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let _client = TcpStream::connect(listener.local_addr().unwrap())
            .await
            .unwrap();
        let (server, _) = listener.accept().await.unwrap();
        let permit = Arc::new(Semaphore::new(1)).acquire_owned().await.unwrap();
        let mut stream = BoundedStream::new(server, permit);
        stream.read_deadline.as_mut().reset(Instant::now());
        let error = stream.read(&mut [0]).await.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        stream.write_deadline = Some(Box::pin(tokio::time::sleep(Duration::ZERO)));
        let error = std::future::poll_fn(|cx| stream.poll_write_timeout(cx))
            .await
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    }
}
