use {
    crate::{
        io::{AsyncIo, Io},
        proto::{Error, Protocol, Security, Spawn},
    },
    hyper::{
        body::{Body, Frame, SizeHint},
        client::conn::http1,
    },
    std::{
        convert::Infallible,
        future::Future,
        pin::Pin,
        task::{Context, Poll},
    },
};

pub struct Http1(());

impl Http1 {
    const ALPN: &'static str = "http/1.1";

    pub fn new() -> Self {
        Self(())
    }

    async fn connect<I>(io: I) -> Result<(Connection, Handle<I>), Error>
    where
        I: AsyncIo,
    {
        let (send, conn) = http1::handshake(Io(io)).await?;
        Ok((Connection { send }, Handle(conn)))
    }
}

impl Protocol for Http1 {
    const SECURITY: Security = Security::No;

    type Connection = Connection;

    async fn connect<'ex, S, I>(&self, spawn: &S, io: I) -> Result<Self::Connection, Error>
    where
        S: Spawn<'ex>,
        I: AsyncIo + Send + 'ex,
    {
        let (conn, handle) = Self::connect(io).await?;
        spawn.spawn(handle);
        Ok(conn)
    }
}

pub struct Connection {
    #[allow(dead_code)]
    send: http1::SendRequest<Empty>,
}

struct Handle<I>(http1::Connection<Io<I>, Empty>)
where
    I: AsyncIo;

impl<I> Future for Handle<I>
where
    I: AsyncIo,
{
    type Output = ();

    #[inline]
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        // poll connection and discard result when it's ready
        Pin::new(&mut self.0).poll(cx).map(drop)
    }
}

struct Empty;

impl Body for Empty {
    type Data = &'static [u8];
    type Error = Infallible;

    #[inline]
    fn poll_frame(
        self: Pin<&mut Self>,
        _: &mut Context,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        Poll::Ready(None)
    }

    #[inline]
    fn is_end_stream(&self) -> bool {
        true
    }

    #[inline]
    fn size_hint(&self) -> SizeHint {
        SizeHint::with_exact(0)
    }
}
