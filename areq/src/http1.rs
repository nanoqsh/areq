use {
    crate::{
        client::Client,
        proto::{Error, Fetch, Protocol, Request, Responce, Session, Spawn},
    },
    areq_h1::Config,
    bytes::Bytes,
    futures_lite::{AsyncRead, AsyncWrite, Stream, StreamExt},
    http::{header, HeaderValue, Version},
    std::{
        pin::Pin,
        task::{Context, Poll},
    },
};

#[derive(Default)]
pub struct Http1 {
    conf: Config,
}

impl Protocol for Http1 {
    type Fetch<B> = FetchHttp1<B>
    where
        B: areq_h1::Body;

    async fn handshake<'ex, S, I, B>(
        &self,
        spawn: &S,
        se: Session<I>,
    ) -> Result<Client<Self, B>, Error>
    where
        S: Spawn<'ex>,
        I: AsyncRead + AsyncWrite + Send + 'ex,
        B: areq_h1::Body<Buf: Send, Stream: Send> + Send + 'ex,
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

pub struct FetchHttp1<B> {
    reqs: areq_h1::Requester<B>,
    host: HeaderValue,
}

impl<B> Fetch<B> for FetchHttp1<B>
where
    B: areq_h1::Body,
{
    type Body = Http1Body;

    fn prepare_request(&self, req: &mut Request<B>) {
        *req.version_mut() = Version::HTTP_11;

        // http/1.1 requires a host header
        req.headers_mut().insert(header::HOST, self.host.clone());

        // insert default accept header if it's missing
        let default_accept = const { HeaderValue::from_static("*/*") };
        req.headers_mut()
            .entry(header::ACCEPT)
            .or_insert(default_accept);
    }

    async fn fetch(&mut self, req: Request<B>) -> Result<Responce<Self::Body>, Error> {
        let res = self
            .reqs
            .send(req.into())
            .await?
            .map(|body| Http1Body(body.body_stream()));

        Ok(Responce::new(res))
    }
}

pub struct Http1Body(areq_h1::BodyStream);

impl Stream for Http1Body {
    type Item = Result<Bytes, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.0.poll_next(cx).map_err(Error::from)
    }
}
