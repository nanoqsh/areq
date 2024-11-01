use {
    crate::{
        client::Client,
        proto::{Error, Fetch, Protocol, Request, Responce, Session, Spawn},
    },
    areq_h1::FetchBody,
    futures_lite::{AsyncRead, AsyncWrite},
    http::{header, HeaderValue, Version},
};

#[derive(Default)]
pub struct Http1 {
    conf: areq_h1::Config,
}

impl Protocol for Http1 {
    type Fetch = FetchHttp1;

    async fn handshake<'ex, S, I>(&self, spawn: &S, se: Session<I>) -> Result<Client<Self>, Error>
    where
        S: Spawn<'ex>,
        I: AsyncRead + AsyncWrite + Send + 'ex,
    {
        let Session { io, addr } = se;
        let (reqs, conn) = self.conf.clone().handshake(io);
        let reqs = Client(FetchHttp1 {
            reqs,
            host: addr.repr().parse().map_err(|_| Error::InvalidHost)?,
        });

        spawn.spawn(conn);
        Ok(reqs)
    }
}

pub struct FetchHttp1 {
    reqs: areq_h1::Requester<()>,
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
        Ok(Responce::new(res.map(FetchBody::into_stream)))
    }
}
