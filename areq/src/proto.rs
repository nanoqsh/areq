use {
    crate::client::Client,
    bytes::Bytes,
    futures_lite::{AsyncRead, AsyncWrite, Stream, StreamExt},
    http::{response::Parts, HeaderMap, Method, StatusCode, Version},
    std::{
        borrow::Cow,
        error, fmt,
        future::Future,
        io,
        pin::Pin,
        task::{Context, Poll},
    },
    url::Host,
};

/// The communication session between a client and a host.
pub struct Session<I> {
    pub io: I,
    pub addr: Address,
}

/// The network address.
pub struct Address {
    pub host: Host,
    pub port: u16,
    pub security: bool,
}

impl Address {
    /// Returns a representation of the host and port based on the security protocol
    pub fn repr(&self) -> Cow<str> {
        if self.port == self.default_port() {
            match &self.host {
                Host::Domain(domain) => Cow::Borrowed(domain),
                Host::Ipv4(ip) => Cow::Owned(ip.to_string()),
                Host::Ipv6(ip) => Cow::Owned(ip.to_string()),
            }
        } else {
            let host = &self.host;
            let port = self.port;
            Cow::Owned(format!("{host}:{port}"))
        }
    }

    fn default_port(&self) -> u16 {
        const HTTP: u16 = 80;
        const HTTPS: u16 = 443;

        if self.security {
            HTTPS
        } else {
            HTTP
        }
    }
}

/// Used HTTP protocol.
pub trait Protocol: Sized {
    type Fetch: Fetch;

    fn handshake<'ex, S, I>(
        &self,
        spawn: &S,
        se: Session<I>,
    ) -> impl Future<Output = Result<Client<Self>, Error>> + Send
    where
        S: Spawn<'ex>,
        I: AsyncRead + AsyncWrite + Send + 'ex;
}

/// The [protocol](Protocol) error type.
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    InvalidHost,
}

impl Error {
    pub fn try_into_io(self) -> Result<io::Error, Self> {
        match self {
            Self::Io(e) => Ok(e),
            e @ Self::InvalidHost => Err(e),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<h2::Error> for Error {
    fn from(e: h2::Error) -> Self {
        if e.is_io() {
            Self::Io(e.into_io().expect("the error should be IO"))
        } else {
            Self::Io(io::Error::other(e))
        }
    }
}

impl From<areq_h1::Error> for Error {
    fn from(e: areq_h1::Error) -> Self {
        Self::Io(e.into())
    }
}

impl From<Error> for io::Error {
    fn from(e: Error) -> Self {
        e.try_into_io().unwrap_or_else(Self::other)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::InvalidHost => write!(f, "invalid host"),
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::InvalidHost => None,
        }
    }
}

/// Trait alias for a thread safe future.
pub trait Task<'ex>: Future<Output = ()> + Send + 'ex {}
impl<'ex, F> Task<'ex> for F where F: Future<Output = ()> + Send + 'ex {}

/// Trait for a [task](Task) spawner.
pub trait Spawn<'ex>: Sync {
    fn spawn<T>(&self, task: T)
    where
        T: Task<'ex>;
}

pub trait Fetch {
    fn prepare_request(&self, req: &mut Request);
    async fn fetch(&mut self, req: Request) -> Result<Responce, Error>;
}

#[derive(Debug)]
pub struct Request(http::Request<()>);

impl Request {
    pub fn get(uri: &str) -> Self {
        let inner = http::Request::builder()
            .method(Method::GET)
            .uri(uri)
            .body(())
            .expect("construct a valid request");

        Self(inner)
    }

    pub(crate) fn as_mut(&mut self) -> &mut http::Request<()> {
        &mut self.0
    }

    pub(crate) fn into_inner(self) -> http::Request<()> {
        self.0
    }
}

enum Decode {
    Stream(Pin<Box<dyn Stream<Item = Result<Bytes, Error>>>>),
}

impl fmt::Debug for Decode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Stream(_) => f.debug_tuple("Stream").finish(),
        }
    }
}

impl Stream for Decode {
    type Item = Result<Bytes, Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        match self.get_mut() {
            Self::Stream(s) => Pin::new(s).poll_next(cx),
        }
    }
}

#[derive(Debug)]
pub struct Responce {
    head: Parts,
    body: Decode,
}

impl Responce {
    pub fn new<B, E>(res: http::Response<B>) -> Self
    where
        B: Stream<Item = Result<Bytes, E>> + 'static,
        E: Into<Error>,
    {
        let (head, body) = res.into_parts();
        let body = Decode::Stream(Box::pin(body.map(|res| res.map_err(E::into))));
        Self { head, body }
    }

    pub fn status(&self) -> StatusCode {
        self.head.status
    }

    pub fn version(&self) -> Version {
        self.head.version
    }

    pub fn headers(&self) -> &HeaderMap {
        &self.head.headers
    }

    pub fn headers_mut(&mut self) -> &mut HeaderMap {
        &mut self.head.headers
    }

    pub fn body_stream(self) -> impl Stream<Item = Result<Bytes, Error>> {
        self.body
    }

    pub fn body_reader(self) -> impl AsyncRead {
        use std::{
            pin::Pin,
            task::{Context, Poll},
        };

        struct Reader {
            stream: Decode,
            bytes: Bytes,
        }

        impl AsyncRead for Reader {
            fn poll_read(
                self: Pin<&mut Self>,
                cx: &mut Context,
                buf: &mut [u8],
            ) -> Poll<Result<usize, io::Error>> {
                if buf.is_empty() {
                    return Poll::Ready(Ok(0));
                }

                let me = self.get_mut();

                if me.bytes.is_empty() {
                    match me.stream.poll_next(cx) {
                        Poll::Ready(Some(Ok(b))) if b.is_empty() => {
                            // if next bytes is empty skip this iteration and reschedule
                            cx.waker().wake_by_ref();
                            return Poll::Pending;
                        }
                        Poll::Ready(Some(Ok(b))) => me.bytes = b,
                        Poll::Ready(Some(Err(e))) => return Poll::Ready(Err(e.into())),
                        Poll::Ready(None) => return Poll::Ready(Ok(0)),
                        Poll::Pending => return Poll::Pending,
                    }
                }

                let n = usize::min(me.bytes.len(), buf.len());
                let head = me.bytes.split_to(n);
                buf[..n].copy_from_slice(&head);
                Poll::Ready(Ok(n))
            }
        }

        let stream = self.body;
        let bytes = Bytes::new();
        Reader { stream, bytes }
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        futures_lite::{io, stream, AsyncReadExt},
    };

    #[test]
    fn body_into_stream() {
        let body = stream::iter(["foo", "bar", "baz"])
            .map(|part| Ok::<_, Error>(Bytes::copy_from_slice(part.as_bytes())));

        let res = Responce::new(http::Response::new(body));
        let actual: Vec<_> = async_io::block_on(
            res.body_stream()
                .map(|res| res.expect("all parts is ok"))
                .collect(),
        );

        assert_eq!(actual, ["foo", "bar", "baz"]);
    }

    #[test]
    fn body_into_reader() {
        let body = stream::iter(["foo", "bar", "baz"])
            .map(|part| Ok::<_, Error>(Bytes::copy_from_slice(part.as_bytes())));

        let res = Responce::new(http::Response::new(body));
        let mut actual = vec![];
        async_io::block_on(io::copy(res.body_reader(), &mut actual)).expect("all parts is ok");
        assert_eq!(actual, b"foobarbaz");
    }

    #[test]
    fn body_into_reader_partial() {
        let body = stream::iter(["foo", "bar", "baz"])
            .map(|part| Ok::<_, Error>(Bytes::copy_from_slice(part.as_bytes())));

        let res = Responce::new(http::Response::new(body));
        let mut reader = res.body_reader();

        let mut buf = [0; 2];
        let n = async_io::block_on(reader.read(&mut buf)).expect("read body part to the buffer");
        assert_eq!(n, 2);
        assert_eq!(&buf, b"fo");

        let mut buf = [0; 2];
        let n = async_io::block_on(reader.read(&mut buf)).expect("read body part to the buffer");
        assert_eq!(n, 1);
        assert_eq!(&buf, b"o\0");
    }
}
