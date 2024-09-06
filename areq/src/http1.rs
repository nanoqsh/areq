use {
    crate::{
        io::{AsyncIo, Io},
        proto::{Error, Protocol, Security, Session, Spawn},
    },
    http::{header, HeaderValue, Method, Request, Version},
    hyper::{
        body::{Body, Frame, Incoming, SizeHint},
        client::conn::http1,
    },
    std::{
        convert::Infallible,
        future::Future,
        pin::Pin,
        task::{Context, Poll},
    },
};

pub struct Http1(());

impl Http1 {
    #[expect(dead_code)]
    const ALPN: &'static str = "http/1.1";

    pub fn new() -> Self {
        Self(())
    }
}

impl Protocol for Http1 {
    type Conn = Connection;

    const SECURITY: Security = Security::No;

    async fn connect<'ex, S, I>(&self, spawn: &S, se: Session<I>) -> Result<Self::Conn, Error>
    where
        S: Spawn<'ex>,
        I: AsyncIo + Send + 'ex,
    {
        let (conn, handle) = {
            let Session { io, host, port } = se;
            let (send, conn) = http1::handshake(Io(io)).await?;

            let host_string = if port == const { Self::SECURITY.default_port() } {
                host.to_string()
            } else {
                format!("{host}:{port}")
            };

            let host_header = host_string.parse().map_err(|_| Error::InvalidHost)?;
            (Connection { send, host_header }, Handle(conn))
        };

        spawn.spawn(handle);
        Ok(conn)
    }
}

pub struct Connection {
    send: http1::SendRequest<Empty>,
    host_header: HeaderValue,
}

impl Connection {
    pub async fn get_request(&mut self, uri: &str) -> Result<Responce, Error> {
        //  let uri = &url[Position::BeforePath..];

        let req = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .version(Version::HTTP_2)
            .header(header::HOST, &self.host_header)
            .header(header::ACCEPT, "*/*")
            .body(Empty)
            .expect("construct a valid request");

        println!("request: {req:#?}");

        self.send.ready().await?;
        let res = self.send.send_request(req).await?;
        println!("response: {res:#?}");

        Ok(Responce(res))
    }
}

#[derive(Debug)]
pub struct Responce(#[expect(dead_code)] hyper::Response<Incoming>);

struct Handle<I>(http1::Connection<Io<I>, Empty>)
where
    I: AsyncIo;

impl<I> Future for Handle<I>
where
    I: AsyncIo,
{
    type Output = ();

    #[inline]
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        // poll connection and discard result when it's ready
        Pin::new(&mut self.0).poll(cx).map(drop)
    }
}

#[derive(Debug)]
struct Empty;

impl Body for Empty {
    type Data = &'static [u8];
    type Error = Infallible;

    #[inline]
    fn poll_frame(
        self: Pin<&mut Self>,
        _: &mut Context,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        Poll::Ready(None)
    }

    #[inline]
    fn is_end_stream(&self) -> bool {
        true
    }

    #[inline]
    fn size_hint(&self) -> SizeHint {
        SizeHint::with_exact(0)
    }
}
