use {
    crate::{
        client::Client,
        proto::{Error, Protocol, Request, Responce, Serve, Session},
    },
    areq_h1::Config,
    bytes::Bytes,
    futures_lite::{AsyncRead, AsyncWrite, Stream, StreamExt},
    http::{header, HeaderValue, Version},
    std::{
        future::Future,
        pin::Pin,
        task::{Context, Poll},
    },
};

#[derive(Default)]
pub struct H1 {
    conf: Config,
}

impl Protocol for H1 {
    type Serve<B> = ServeH1<B>
    where
        B: areq_h1::IntoBody;

    async fn handshake<I, B>(&self, se: Session<I>) -> Result<(Client<Self, B>, impl Future), Error>
    where
        I: AsyncRead + AsyncWrite,
        B: areq_h1::IntoBody,
    {
        let Session { io, addr } = se;
        let (reqs, conn) = self.conf.clone().handshake(io);
        let host = addr.repr().parse().map_err(|_| Error::InvalidHost)?;
        let client = Client(ServeH1 { reqs, host });
        Ok((client, conn))
    }
}

pub struct ServeH1<B> {
    reqs: areq_h1::Requester<B>,
    host: HeaderValue,
}

impl<B> Serve<B> for ServeH1<B>
where
    B: areq_h1::IntoBody,
{
    type Body = BodyH1;

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

    async fn serve(&mut self, req: Request<B>) -> Result<Responce<Self::Body>, Error> {
        let res = self
            .reqs
            .send(req.into())
            .await?
            .map(|body| BodyH1(body.body_stream()));

        Ok(Responce::new(res))
    }
}

pub struct BodyH1(areq_h1::BodyStream);

impl Stream for BodyH1 {
    type Item = Result<Bytes, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.0.poll_next(cx).map_err(Error::from)
    }
}
