use {
    crate::client::Client,
    bytes::Bytes,
    futures_lite::{AsyncRead, AsyncWrite, Stream, StreamExt},
    http::{response::Parts, HeaderMap, Method, StatusCode, Version},
    std::{borrow::Cow, error, fmt, future::Future, io},
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
    type Fetch: Fetch<Body = Self::Body>;
    type Body;

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
    pub fn into_io(self) -> io::Error {
        use std::io::ErrorKind;

        match self {
            Self::Io(e) => e,
            Self::InvalidHost => io::Error::new(ErrorKind::InvalidInput, Self::InvalidHost),
        }
    }
}

impl<E> From<E> for Error
where
    E: Into<io::Error>,
{
    fn from(e: E) -> Self {
        Self::Io(e.into())
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
    type Error: Into<Error>;
    type Body: Stream<Item = Result<Bytes, Self::Error>>;

    fn prepare_request(&self, req: &mut Request);

    #[expect(async_fn_in_trait)]
    async fn fetch(&mut self, req: Request) -> Result<Responce<Self::Body>, Error>;
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

#[derive(Debug)]
pub struct Responce<B> {
    head: Parts,
    body: B,
}

impl<B> Responce<B> {
    pub fn new(res: http::Response<B>) -> Self {
        let (head, body) = res.into_parts();
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

    pub fn into_stream<E>(self) -> impl Stream<Item = Result<Bytes, Error>>
    where
        B: Stream<Item = Result<Bytes, E>>,
        E: Into<Error>,
    {
        self.body.map(|res| res.map_err(E::into))
    }

    pub fn into_reader<E>(self) -> impl AsyncRead
    where
        B: Stream<Item = Result<Bytes, E>>,
        E: Into<Error>,
    {
        use std::{
            pin::Pin,
            task::{Context, Poll},
        };

        struct Reader<S> {
            stream: S,
            bytes: Bytes,
        }

        impl<S> Reader<S> {
            fn project(self: Pin<&mut Self>) -> (Pin<&mut S>, &mut Bytes) {
                // SAFETY: don't move the self
                let me = unsafe { self.get_unchecked_mut() };

                // SAFETY: pin the stream back and don't move it later
                let stream = unsafe { Pin::new_unchecked(&mut me.stream) };

                (stream, &mut me.bytes)
            }
        }

        impl<S> AsyncRead for Reader<S>
        where
            S: Stream<Item = Result<Bytes, io::Error>>,
        {
            fn poll_read(
                self: Pin<&mut Self>,
                cx: &mut Context,
                buf: &mut [u8],
            ) -> Poll<Result<usize, io::Error>> {
                if buf.is_empty() {
                    return Poll::Ready(Ok(0));
                }

                let (mut stream, bytes) = self.project();

                if bytes.is_empty() {
                    match stream.as_mut().poll_next(cx) {
                        Poll::Ready(Some(Ok(b))) if b.is_empty() => {
                            // if next bytes is empty skip this iteration and reschedule
                            cx.waker().wake_by_ref();
                            return Poll::Pending;
                        }
                        Poll::Ready(Some(Ok(b))) => *bytes = b,
                        Poll::Ready(Some(Err(e))) => return Poll::Ready(Err(e)),
                        Poll::Ready(None) => return Poll::Ready(Ok(0)),
                        Poll::Pending => return Poll::Pending,
                    }
                }

                let n = usize::min(bytes.len(), buf.len());
                let head = bytes.split_to(n);
                buf[..n].copy_from_slice(&head);
                Poll::Ready(Ok(n))
            }
        }

        let stream = self.into_stream().map(|res| res.map_err(Error::into_io));
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
            res.into_stream()
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
        async_io::block_on(io::copy(res.into_reader(), &mut actual)).expect("all parts is ok");
        assert_eq!(actual, b"foobarbaz");
    }

    #[test]
    fn body_into_reader_partial() {
        let body = stream::iter(["foo", "bar", "baz"])
            .map(|part| Ok::<_, Error>(Bytes::copy_from_slice(part.as_bytes())));

        let res = Responce::new(http::Response::new(body));
        let mut reader = res.into_reader();

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
