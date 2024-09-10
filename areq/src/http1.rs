use {
    crate::{
        body::Empty,
        conn::Connection,
        io::{AsyncIo, Io},
        proto::{Error, Fetch, Protocol, Request, Responce, Security, Session, Spawn},
    },
    hyper::{
        client::conn::http1,
        header,
        http::{HeaderValue, Version},
    },
    std::{
        future::Future,
        pin::Pin,
        task::{Context, Poll},
    },
};

pub struct Http1;

impl Http1 {
    #[expect(dead_code)]
    const ALPN: &'static str = "http/1.1";
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
            let (send, connio) = http1::handshake(Io(io)).await?;

            let host_string = if port == const { Self::SECURITY.default_port() } {
                host.to_string()
            } else {
                format!("{host}:{port}")
            };

            let conn = Connection {
                fetch: FetchHttp1 {
                    send,
                    host: host_string.parse().map_err(|_| Error::InvalidHost)?,
                },
            };

            let handle = Handle(connio);
            (conn, handle)
        };

        spawn.spawn(handle);
        Ok(conn)
    }
}

pub struct FetchHttp1 {
    send: http1::SendRequest<Empty>,
    host: HeaderValue,
}

impl Fetch for FetchHttp1 {
    fn prepare_request(&self, req: &mut Request) {
        let req = req.as_mut();
        *req.version_mut() = Version::HTTP_11;

        // http/1.1 requires a host header
        req.headers_mut().insert(header::HOST, self.host.clone());

        // insert default accept header if it's missing
        let default_accept = const { HeaderValue::from_static("*/*") };
        req.headers_mut()
            .entry(header::ACCEPT)
            .or_insert(default_accept);
    }

    async fn fetch(&mut self, req: Request) -> Result<Responce, Error> {
        self.send.ready().await?;
        let res = self.send.send_request(req.into_inner()).await?;
        Ok(Responce::new(res))
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
