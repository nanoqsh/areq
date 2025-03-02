use {
    crate::{
        body::prelude::*,
        negotiate::Negotiate,
        proto::{Client, Error, Handshake, Request, Response, Session},
    },
    areq_h1::Config,
    futures_lite::prelude::*,
    http::{HeaderValue, Version, header},
};

#[derive(Clone, Default)]
pub struct Http1 {
    conf: Config,
}

impl Http1 {
    const ALPN: &[u8] = b"http/1.1";
}

impl<I> Handshake<I> for Http1
where
    I: AsyncRead + AsyncWrite,
{
    type Client<B>
        = H1<B>
    where
        B: IntoBody;

    async fn handshake<B>(self, se: Session<I>) -> Result<(Self::Client<B>, impl Future), Error>
    where
        B: IntoBody,
    {
        let Session { addr, io } = se;
        let (reqs, conn) = self.conf.handshake(io);
        let host = addr.repr().parse().map_err(|_| Error::InvalidHost)?;
        let client = H1 { reqs, host };
        Ok((client, conn))
    }
}

impl Negotiate for Http1 {
    type Handshake = Self;

    fn negotiate(self, proto: &[u8]) -> Option<Self::Handshake> {
        match proto {
            Self::ALPN => Some(self),
            _ => None,
        }
    }

    fn support(&self) -> impl Iterator<Item = &'static [u8]> {
        [Self::ALPN].into_iter()
    }
}

pub struct H1<B> {
    reqs: areq_h1::Requester<B>,
    host: HeaderValue,
}

impl<B> H1<B> {
    fn prepare(&self, req: &mut Request<B>) {
        *req.version_mut() = Version::HTTP_11;

        // http/1.1 requires a host header
        req.headers_mut().insert(header::HOST, self.host.clone());

        // insert default accept header if it's missing
        let default_accept = const { HeaderValue::from_static("*/*") };
        req.headers_mut()
            .entry(header::ACCEPT)
            .or_insert(default_accept);
    }
}

impl<B> Client<B> for H1<B>
where
    B: IntoBody,
{
    type Body = areq_h1::FetchBody;

    async fn send(&mut self, mut req: Request<B>) -> Result<Response<Self::Body>, Error> {
        self.prepare(&mut req);
        let res = self.reqs.send(req.into()).await?;
        Ok(Response::new(res))
    }
}

impl From<areq_h1::Error> for Error {
    fn from(e: areq_h1::Error) -> Self {
        Self::Io(e.into())
    }
}
