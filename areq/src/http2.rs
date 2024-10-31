use {
    crate::{
        client::Client, io::Io, proto::Fetch, Error, Protocol, Request, Responce, Session, Spawn,
    },
    bytes::Bytes,
    futures_core::Stream,
    futures_io::{AsyncRead, AsyncWrite},
    h2::client,
    http::{header, HeaderValue, Version},
    std::{
        io,
        pin::Pin,
        task::{Context, Poll},
    },
};

#[derive(Default)]
pub struct Http2 {
    build: client::Builder,
}

impl Protocol for Http2 {
    type Fetch = FetchHttp2;
    type Body = <FetchHttp2 as Fetch>::Body;

    async fn connect<'ex, S, I>(&self, spawn: &S, se: Session<I>) -> Result<Client<Self>, Error>
    where
        S: Spawn<'ex>,
        I: AsyncRead + AsyncWrite + Send + 'ex,
    {
        let Session { io, .. } = se;
        let io = Io(Box::pin(io));
        let (send, conn) = self.build.handshake(io).await.map_err(into_io_error)?;
        let reqs = Client(FetchHttp2 { send });

        spawn.spawn(async {
            _ = conn.await;
        });

        Ok(reqs)
    }
}

fn into_io_error(e: h2::Error) -> io::Error {
    if e.is_io() {
        e.into_io().expect("the error should be IO")
    } else {
        io::Error::other(e)
    }
}

#[derive(Clone)]
pub struct FetchHttp2 {
    send: client::SendRequest<&'static [u8]>,
}

impl Fetch for FetchHttp2 {
    type Error = Error;
    type Body = BodyStream;

    fn prepare_request(&self, req: &mut Request) {
        let req = req.as_mut();
        *req.version_mut() = Version::HTTP_2;

        // insert default accept header if it's missing
        let default_accept = const { HeaderValue::from_static("*/*") };
        req.headers_mut()
            .entry(header::ACCEPT)
            .or_insert(default_accept);
    }

    async fn fetch(&mut self, req: Request) -> Result<Responce<Self::Body>, Error> {
        let mut send = self.send.clone().ready().await.map_err(into_io_error)?;
        let (resfu, stream) = send
            .send_request(req.into_inner(), true)
            .map_err(into_io_error)?;

        _ = stream;

        let res = resfu.await.map_err(into_io_error)?;
        Ok(Responce(res.map(BodyStream)))
    }
}

pub struct BodyStream(h2::RecvStream);

impl Stream for BodyStream {
    type Item = Result<Bytes, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.0
            .poll_data(cx)
            .map_err(into_io_error)
            .map_err(Error::Io)
    }
}
