use std::{
    io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use axum::serve::Listener;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, ReadBuf},
    net::TcpStream,
    sync::{Semaphore, mpsc},
    task::{JoinHandle, JoinSet},
};
use tokio_rustls::{TlsAcceptor, server::TlsStream};

use super::TlsManager;
use crate::{
    client_ip::{ClientConnection, ClientIpListener},
    proxy_protocol::PrefixedStream,
};

type Stream = PrefixedStream<PrefixedStream<TcpStream>>;

pub enum ConnectionIo {
    Plain(Stream),
    Encrypted(Box<TlsStream<Stream>>),
}

impl AsyncRead for ConnectionIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match &mut *self {
            Self::Plain(stream) => Pin::new(stream).poll_read(cx, buf),
            Self::Encrypted(stream) => Pin::new(stream.as_mut()).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for ConnectionIo {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match &mut *self {
            Self::Plain(stream) => Pin::new(stream).poll_write(cx, buf),
            Self::Encrypted(stream) => Pin::new(stream.as_mut()).poll_write(cx, buf),
        }
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut *self {
            Self::Plain(stream) => Pin::new(stream).poll_flush(cx),
            Self::Encrypted(stream) => Pin::new(stream.as_mut()).poll_flush(cx),
        }
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut *self {
            Self::Plain(stream) => Pin::new(stream).poll_shutdown(cx),
            Self::Encrypted(stream) => Pin::new(stream.as_mut()).poll_shutdown(cx),
        }
    }
}

/// PPv2 is consumed before TLS, preserving both the real TCP peer and client IP.
/// Plain HTTP is only used for the router's redirect and local health endpoint
/// while TLS is enabled. Each handshake has a deadline and a concurrency bound.
pub struct TlsListener {
    address: ClientConnection,
    ready: mpsc::Receiver<(ConnectionIo, ClientConnection)>,
    acceptor: JoinHandle<()>,
}

impl TlsListener {
    pub fn new(mut listener: ClientIpListener, manager: TlsManager) -> io::Result<Self> {
        let address = listener.local_addr()?;
        let (sender, ready) = mpsc::channel(64);
        let acceptor = tokio::spawn(async move {
            let permits = Arc::new(Semaphore::new(64));
            let mut tasks = JoinSet::new();
            loop {
                let Ok(permit) = permits.clone().acquire_owned().await else {
                    return;
                };
                let (mut stream, mut connection) = listener.accept().await;
                if sender.is_closed() {
                    return;
                }
                while tasks.try_join_next().is_some() {}
                let sender = sender.clone();
                let manager = manager.clone();
                tasks.spawn(async move {
                    let _permit = permit;
                    let result = tokio::time::timeout(Duration::from_secs(5), async {
                        let first = stream.read_u8().await?;
                        let stream = PrefixedStream::with_prefix(stream, vec![first]);
                        let io = if first == 0x16 {
                            let identity = manager
                                .current()
                                .ok_or_else(|| io::Error::other("TLS disabled"))?;
                            let encrypted = TlsAcceptor::from(identity.config.clone())
                                .accept(stream)
                                .await?;
                            connection.tls = true;
                            ConnectionIo::Encrypted(Box::new(encrypted))
                        } else {
                            ConnectionIo::Plain(stream)
                        };
                        Ok::<_, io::Error>((io, connection))
                    })
                    .await;
                    if let Ok(Ok(connection)) = result {
                        let _ = sender.send(connection).await;
                    }
                });
            }
        });
        Ok(Self {
            address,
            ready,
            acceptor,
        })
    }
}

impl Drop for TlsListener {
    fn drop(&mut self) {
        self.acceptor.abort();
    }
}

impl Listener for TlsListener {
    type Io = ConnectionIo;
    type Addr = ClientConnection;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        match self.ready.recv().await {
            Some(connection) => connection,
            None => std::future::pending().await,
        }
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        Ok(self.address)
    }
}
