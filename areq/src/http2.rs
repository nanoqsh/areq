use {
    crate::{client::Client, io::Io, proto::Serve, Error, Protocol, Request, Responce, Session},
    bytes::{Buf, Bytes},
    futures_lite::{AsyncRead, AsyncWrite, Stream, StreamExt},
    h2::client,
    http::{header, HeaderValue, Version},
    std::{
        future::{self, Future},
        pin::{self, Pin},
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
    send: client::SendRequest<Flow<B::Data>>,
}

impl<B> ServeH2<B>
where
    B: areq_h1::Body,
{
    async fn ready(&mut self) -> Result<(), h2::Error> {
        future::poll_fn(|cx| self.send.poll_ready(cx)).await
    }
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
        use areq_h1::Chunk;

        let (head, body) = http::Request::from(req).into_parts();
        let header_req = http::Request::from_parts(head, ());

        self.ready().await?;
        let (resfu, mut send_body) = self.send.send_request(header_req, true)?;

        match body.chunk() {
            Chunk::Full(data) => send_body.send_data(Flow::Next(data), true)?,
            Chunk::Stream(stream) => {
                let mut stream = pin::pin!(stream);
                while let Some(c) = stream.next().await {
                    send_body.send_data(Flow::Next(c), false)?;
                }

                send_body.send_data(Flow::End, true)?;
            }
        }

        let res = resfu.await?.map(BodyH2);
        Ok(Responce::new(res))
    }
}

enum Flow<B> {
    Next(B),
    End,
}

impl<B> Buf for Flow<B>
where
    B: Buf,
{
    fn remaining(&self) -> usize {
        match self {
            Self::Next(buf) => buf.remaining(),
            Self::End => 0,
        }
    }

    fn chunk(&self) -> &[u8] {
        match self {
            Self::Next(buf) => buf.chunk(),
            Self::End => &[],
        }
    }

    fn advance(&mut self, cnt: usize) {
        match self {
            Self::Next(buf) => buf.advance(cnt),
            Self::End => assert_eq!(cnt, 0, "can't advance further than end"),
        }
    }
}

pub struct BodyH2(h2::RecvStream);

impl Stream for BodyH2 {
    type Item = Result<Bytes, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.0.poll_data(cx).map_err(Error::from)
    }
}
