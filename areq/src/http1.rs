use {
    crate::{
        conn::Connection,
        proto::{Error, Fetch, Protocol, Request, Responce, Security, Session, Spawn},
    },
    areq_h1::{Builder, Requester},
    futures_io::{AsyncRead, AsyncWrite},
    http::{header, HeaderValue, Version},
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
        I: AsyncRead + AsyncWrite + Send + 'ex,
    {
        let Session { io, host, port } = se;
        let (reqs, conn) = Builder::default().handshake(io);

        let host_string = if port == const { Self::SECURITY.default_port() } {
            host.to_string()
        } else {
            format!("{host}:{port}")
        };

        let conn_http = Connection {
            fetch: FetchHttp1 {
                reqs,
                host: host_string.parse().map_err(|_| Error::InvalidHost)?,
            },
        };

        spawn.spawn(conn);
        Ok(conn_http)
    }
}

pub struct FetchHttp1 {
    reqs: Requester<()>,
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
        let res = self.reqs.send(req.into_inner()).await?;
        Ok(Responce::new(res))
    }
}
