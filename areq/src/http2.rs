use {
    crate::{
        client::Client, io::Io, proto::Fetch, Error, Protocol, Request, Responce, Session, Spawn,
    },
    bytes::Bytes,
    futures_lite::{AsyncRead, AsyncWrite, Stream},
    h2::client,
    http::{header, HeaderValue, Version},
    std::{
        pin::Pin,
        task::{Context, Poll},
    },
};

#[derive(Default)]
pub struct Http2 {
    build: client::Builder,
}

impl Protocol for Http2 {
    type Fetch<B> = FetchHttp2<B>
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
        let Session { io, .. } = se;
        let io = Io(Box::pin(io));
        let (send, conn) = self.build.handshake(io).await?;
        let reqs = Client(FetchHttp2 { send });

        spawn.spawn(async {
            _ = conn.await;
        });

        Ok(reqs)
    }
}

#[derive(Clone)]
pub struct FetchHttp2<B>
where
    B: areq_h1::Body,
{
    send: client::SendRequest<B::Buf>,
}

impl<B> Fetch<B> for FetchHttp2<B>
where
    B: areq_h1::Body,
{
    type Body = Http2Body;

    fn prepare_request(&self, req: &mut Request<B>) {
        *req.version_mut() = Version::HTTP_2;

        // insert default accept header if it's missing
        let default_accept = const { HeaderValue::from_static("*/*") };
        req.headers_mut()
            .entry(header::ACCEPT)
            .or_insert(default_accept);
    }

    async fn fetch(&mut self, req: Request<B>) -> Result<Responce<Self::Body>, Error> {
        let mut send = self.send.clone().ready().await?;
        let req: http::Request<_> = req.into();
        let (resfu, stream) = send.send_request(req.map(|_| ()), true)?;

        _ = stream;

        let res = resfu.await?.map(Http2Body);
        Ok(Responce::new(res))
    }
}

pub struct Http2Body(h2::RecvStream);

impl Stream for Http2Body {
    type Item = Result<Bytes, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.0.poll_data(cx).map_err(Error::from)
    }
}
