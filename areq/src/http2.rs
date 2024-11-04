use {
    crate::{
        body::{self, Body, IntoBody, Kind},
        client::Client,
        io::Io,
        proto::Serve,
        Error, Protocol, Request, Response, Session,
    },
    bytes::{Buf, Bytes},
    futures_lite::{AsyncRead, AsyncWrite, Stream},
    h2::client,
    http::{header, HeaderValue, Version},
    std::{
        future::{self, Future},
        pin::Pin,
        task::{Context, Poll},
    },
};

#[derive(Clone, Default)]
pub struct H2 {
    build: client::Builder,
}

impl Protocol for H2 {
    type Serve<B> = ServeH2<B>
    where
        B: IntoBody;

    async fn handshake<I, B>(&self, se: Session<I>) -> Result<(Client<Self, B>, impl Future), Error>
    where
        I: AsyncRead + AsyncWrite,
        B: IntoBody,
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
    B: IntoBody,
{
    send: client::SendRequest<Flow<B::Chunk>>,
}

impl<B> ServeH2<B>
where
    B: IntoBody,
{
    async fn ready(&mut self) -> Result<(), h2::Error> {
        future::poll_fn(|cx| self.send.poll_ready(cx)).await
    }
}

impl<B> Clone for ServeH2<B>
where
    B: IntoBody,
{
    fn clone(&self) -> Self {
        Self {
            send: self.send.clone(),
        }
    }
}

impl<B> Serve<B> for ServeH2<B>
where
    B: IntoBody,
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

    async fn serve(&mut self, req: Request<B>) -> Result<Response<Self::Body>, Error> {
        let (head, body) = http::Request::from(req).into_parts();
        let header_req = http::Request::from_parts(head, ());

        let mut body = body.into_body();
        let empty = body.is_end();

        self.ready().await?;
        let (resfu, mut send_body) = self.send.send_request(header_req, empty)?;

        match B::Body::KIND {
            Kind::Empty => debug_assert!(empty, "an empty body must be empty"),
            Kind::Full => {
                debug_assert!(!empty, "a full body must not be empty");

                let full = body::take_full(body).await;
                send_body.send_data(Flow::Next(full), true)?;
            }
            Kind::Chunked => 'stream: {
                if empty {
                    break 'stream;
                }

                while let Some(chunk) = body.chunk().await {
                    let end = body.is_end();
                    send_body.send_data(Flow::Next(chunk), end)?;

                    if end {
                        break 'stream;
                    }
                }

                send_body.send_data(Flow::End, true)?;
            }
        }

        let res = resfu.await?.map(BodyH2);
        Ok(Response::new(res))
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
