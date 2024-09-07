use {
    crate::{
        body::Empty,
        conn::Connection,
        io::{AsyncIo, Io},
        proto::{Error, Fetch, Protocol, Request, Responce, Security, Session, Spawn},
    },
    hyper::client::conn::http1,
    std::{
        future::Future,
        pin::Pin,
        task::{Context, Poll},
    },
};

pub struct Http1(());

impl Http1 {
    #[expect(dead_code)]
    const ALPN: &'static str = "http/1.1";

    pub fn new() -> Self {
        Self(())
    }
}

impl Protocol for Http1 {
    type Fetch = FetchHttp1;

    const SECURITY: Security = Security::No;

    async fn connect<'ex, S, I>(&self, spawn: &S, se: Session<I>) -> Result<Connection<Self>, Error>
    where
        S: Spawn<'ex>,
        I: AsyncIo + Send + 'ex,
    {
        let (conn, handle) = {
            let Session { io, host, port } = se;
            let (send, conn) = http1::handshake(Io(io)).await?;
            let fetch = FetchHttp1 { send };

            let host_string = if port == const { Self::SECURITY.default_port() } {
                host.to_string()
            } else {
                format!("{host}:{port}")
            };

            let host_header = host_string.parse().map_err(|_| Error::InvalidHost)?;
            (Connection { fetch, host_header }, Handle(conn))
        };

        spawn.spawn(handle);
        Ok(conn)
    }
}

pub struct FetchHttp1 {
    send: http1::SendRequest<Empty>,
}

impl Fetch for FetchHttp1 {
    async fn fetch(&mut self, req: Request) -> Result<Responce, Error> {
        self.send.ready().await?;
        let res = self.send.send_request(req.0).await?;
        Ok(Responce(res))
    }
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
