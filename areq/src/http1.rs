use {
    crate::{
        body::IntoBody,
        client::Client,
        proto::{Error, Protocol, Request, Response, Serve, Session},
    },
    areq_h1::Config,
    bytes::Bytes,
    futures_lite::{AsyncRead, AsyncWrite, Stream},
    http::{header, HeaderValue, Version},
    std::{
        future::Future,
        pin::Pin,
        task::{Context, Poll},
    },
};

#[derive(Clone, Default)]
pub struct H1 {
    conf: Config,
}

impl Protocol for H1 {
    type Serve<B>
        = ServeH1<B>
    where
        B: IntoBody;

    async fn handshake<I, B>(self, se: Session<I>) -> Result<(Client<Self, B>, impl Future), Error>
    where
        I: AsyncRead + AsyncWrite,
        B: IntoBody,
    {
        let Session { io, addr } = se;
        let (reqs, conn) = self.conf.handshake(io);
        let host = addr.repr().parse().map_err(|_| Error::InvalidHost)?;
        let client = Client(ServeH1 { reqs, host });
        Ok((client, conn))
    }
}

pub struct ServeH1<B> {
    reqs: areq_h1::Requester<B>,
    host: HeaderValue,
}

impl<B> ServeH1<B> {
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

impl<B> Serve<B> for ServeH1<B>
where
    B: IntoBody,
{
    type Body = BodyH1;

    async fn serve(&mut self, mut req: Request<B>) -> Result<Response<Self::Body>, Error> {
        self.prepare(&mut req);

        let res = self.reqs.send(req.into()).await?.map(|body| BodyH1 {
            body: body.stream(),
        });

        Ok(Response::new(res))
    }
}

pin_project_lite::pin_project! {
    pub struct BodyH1 {
        #[pin]
        body: areq_h1::BodyStream,
    }
}

impl Stream for BodyH1 {
    type Item = Result<Bytes, Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.project().body.poll_next(cx).map_err(Error::from)
    }
}
