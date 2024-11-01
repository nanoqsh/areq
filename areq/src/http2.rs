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
    type Fetch = FetchHttp2;

    async fn handshake<'ex, S, I>(&self, spawn: &S, se: Session<I>) -> Result<Client<Self>, Error>
    where
        S: Spawn<'ex>,
        I: AsyncRead + AsyncWrite + Send + 'ex,
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
pub struct FetchHttp2 {
    send: client::SendRequest<&'static [u8]>,
}

impl Fetch for FetchHttp2 {
    fn prepare_request(&self, req: &mut Request) {
        let req = req.as_mut();
        *req.version_mut() = Version::HTTP_2;

        // insert default accept header if it's missing
        let default_accept = const { HeaderValue::from_static("*/*") };
        req.headers_mut()
            .entry(header::ACCEPT)
            .or_insert(default_accept);
    }

    async fn fetch(&mut self, req: Request) -> Result<Responce, Error> {
        let mut send = self.send.clone().ready().await?;
        let (resfu, stream) = send.send_request(req.into_inner(), true)?;

        _ = stream;

        let res = resfu.await?;
        Ok(Responce::new(res.map(BodyStream)))
    }
}

pub struct BodyStream(h2::RecvStream);

impl Stream for BodyStream {
    type Item = Result<Bytes, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.0.poll_data(cx).map_err(Error::from)
    }
}
