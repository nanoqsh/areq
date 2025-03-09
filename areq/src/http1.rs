//! The http/1.1 client.

use {
    crate::{
        body::prelude::*,
        client::Client,
        negotiate::Negotiate,
        proto::{Error, Handshake, Request, Response, Session},
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

impl<I, B> Handshake<I, B> for Http1
where
    I: AsyncRead + AsyncWrite,
    B: IntoBody,
{
    type Client = H1<B>;

    async fn handshake(self, se: Session<I>) -> Result<(Self::Client, impl Future), Error> {
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

pub struct H1<B>
where
    B: IntoBody,
{
    reqs: areq_h1::Requester<B::Body>,
    host: HeaderValue,
}

impl<B> H1<B>
where
    B: IntoBody,
{
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
        let req = req.map(B::into_body).into();
        let res = self.reqs.send(req).await?;
        Ok(Response::new(res))
    }
}

impl From<areq_h1::Error> for Error {
    fn from(e: areq_h1::Error) -> Self {
        Self::Io(e.into())
    }
}
