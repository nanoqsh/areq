use {
    crate::{client::Client, io::Io, proto::Serve, Error, Protocol, Request, Responce, Session},
    bytes::Bytes,
    futures_lite::{AsyncRead, AsyncWrite, Stream},
    h2::client,
    http::{header, HeaderValue, Version},
    std::{
        future::Future,
        pin::Pin,
        task::{Context, Poll},
    },
};

#[derive(Default)]
pub struct H2 {
    build: client::Builder,
}

impl Protocol for H2 {
    type Serve<B> = ServeH2<B>
    where
        B: areq_h1::Body;

    async fn handshake<I, B>(&self, se: Session<I>) -> Result<(Client<Self, B>, impl Future), Error>
    where
        I: AsyncRead + AsyncWrite,
        B: areq_h1::Body,
    {
        let Session { io, .. } = se;
        let io = Io(Box::pin(io));
        let (send, conn) = self.build.handshake(io).await?;
        let client = Client(ServeH2 { send });
        let conn = async {
            _ = conn.await;
        };

        Ok((client, conn))
    }
}

pub struct ServeH2<B>
where
    B: areq_h1::Body,
{
    send: client::SendRequest<B::Data>,
}

impl<B> Clone for ServeH2<B>
where
    B: areq_h1::Body,
{
    fn clone(&self) -> Self {
        Self {
            send: self.send.clone(),
        }
    }
}

impl<B> Serve<B> for ServeH2<B>
where
    B: areq_h1::Body,
{
    type Body = BodyH2;

    fn prepare(&self, req: &mut Request<B>) {
        *req.version_mut() = Version::HTTP_2;

        // insert default accept header if it's missing
        let default_accept = const { HeaderValue::from_static("*/*") };
        req.headers_mut()
            .entry(header::ACCEPT)
            .or_insert(default_accept);
    }

    async fn serve(&mut self, req: Request<B>) -> Result<Responce<Self::Body>, Error> {
        let mut send = self.send.clone().ready().await?;
        let req: http::Request<_> = req.into();
        let (resfu, stream) = send.send_request(req.map(|_| ()), true)?;

        _ = stream;

        let res = resfu.await?.map(BodyH2);
        Ok(Responce::new(res))
    }
}

pub struct BodyH2(h2::RecvStream);

impl Stream for BodyH2 {
    type Item = Result<Bytes, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.0.poll_data(cx).map_err(Error::from)
    }
}
