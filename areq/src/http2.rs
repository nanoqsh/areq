use {
    crate::{
        Error, Handshake, Request, Response, Session, body::prelude::*, io::Io, proto::Client,
        tls::Negotiate,
    },
    bytes::{Buf, Bytes},
    futures_lite::prelude::*,
    h2::client,
    http::{HeaderValue, Version, header},
    std::{future, io},
};

#[derive(Clone, Default)]
pub struct Http2 {
    build: client::Builder,
}

impl Http2 {
    const ALPN: &[u8] = b"h2";
}

impl<I> Handshake<I> for Http2
where
    I: AsyncRead + AsyncWrite + Unpin,
{
    type Client<B>
        = H2<B>
    where
        B: IntoBody;

    async fn handshake<B>(self, se: Session<I>) -> Result<(Self::Client<B>, impl Future), Error>
    where
        B: IntoBody,
    {
        let Session { addr, io } = se;
        let io = Io::new(io);
        let (send, conn) = self.build.handshake(io).await?;
        let host = addr.repr().parse().map_err(|_| Error::InvalidHost)?;
        let client = H2 { send, host };
        let conn = async {
            _ = conn.await;
        };

        Ok((client, conn))
    }
}

impl Negotiate for Http2 {
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

pub struct H2<B>
where
    B: IntoBody,
{
    send: client::SendRequest<Flow<B::Chunk>>,
    host: HeaderValue,
}

impl<B> H2<B>
where
    B: IntoBody,
{
    fn prepare(&self, req: &mut Request<B>) {
        debug_assert!(
            req.uri().scheme().is_some(),
            "the request must have an uri scheme for http2",
        );

        *req.version_mut() = Version::HTTP_2;

        // http/2 requires a host header
        req.headers_mut().insert(header::HOST, self.host.clone());

        // insert default accept header if it's missing
        let default_accept = const { HeaderValue::from_static("*/*") };
        req.headers_mut()
            .entry(header::ACCEPT)
            .or_insert(default_accept);
    }

    async fn ready(&mut self) -> Result<(), h2::Error> {
        future::poll_fn(|cx| self.send.poll_ready(cx)).await
    }
}

impl<B> Clone for H2<B>
where
    B: IntoBody,
{
    fn clone(&self) -> Self {
        Self {
            send: self.send.clone(),
            host: self.host.clone(),
        }
    }
}

impl<B> Client<B> for H2<B>
where
    B: IntoBody,
{
    type Body = BodyH2;

    async fn send(&mut self, mut req: Request<B>) -> Result<Response<Self::Body>, Error> {
        self.prepare(&mut req);

        let (head, body) = http::Request::from(req).into_parts();
        let header_req = http::Request::from_parts(head, ());

        let mut body = body.into_body();
        let size = body.size_hint();
        let end = size.end();

        self.ready().await?;
        let (resfu, mut send_body) = self.send.send_request(header_req, end)?;

        'body: {
            if end {
                break 'body;
            }

            match size {
                Hint::Full { .. } => {
                    let chunk = body.take_full().await?;
                    send_body.send_data(Flow::from_option(chunk), true)?;
                }
                Hint::Chunked { .. } => {
                    while let Some(chunk) = body.chunk().await {
                        let end = body.size_hint().end();
                        send_body.send_data(Flow::Next(chunk?), end)?;

                        if end {
                            break 'body;
                        }
                    }

                    send_body.send_data(Flow::End, true)?;
                }
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

impl<B> Flow<B> {
    fn from_option(opt: Option<B>) -> Self {
        match opt {
            Some(buf) => Self::Next(buf),
            None => Self::End,
        }
    }
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

impl Body for BodyH2 {
    type Chunk = Bytes;

    async fn chunk(&mut self) -> Option<Result<Self::Chunk, io::Error>> {
        let res = self.0.data().await?;
        Some(res.map_err(into_io_error))
    }

    fn size_hint(&self) -> Hint {
        Hint::Chunked {
            end: self.0.is_end_stream(),
        }
    }
}

pub(crate) fn into_io_error(e: h2::Error) -> io::Error {
    if e.is_io() {
        e.into_io().expect("the error should be IO")
    } else {
        io::Error::other(e)
    }
}
