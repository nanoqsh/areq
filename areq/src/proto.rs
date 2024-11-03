use {
    crate::client::Client,
    bytes::Bytes,
    futures_lite::{AsyncRead, AsyncWrite, Stream, StreamExt},
    http::{request, response, HeaderMap, Method, StatusCode, Uri, Version},
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
    pub secure: bool,
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

        if self.secure {
            HTTPS
        } else {
            HTTP
        }
    }
}

/// Used HTTP protocol.
pub trait Protocol {
    type Serve<B>: Serve<B>
    where
        B: areq_h1::Body;

    #[expect(async_fn_in_trait)]
    async fn handshake<I, B>(
        &self,
        se: Session<I>,
    ) -> Result<(Client<Self, B>, impl Future), Error>
    where
        I: AsyncRead + AsyncWrite,
        B: areq_h1::Body;
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

pub trait Serve<B> {
    type Body: BodyStream;

    fn prepare(&self, req: &mut Request<B>);

    #[expect(async_fn_in_trait)]
    async fn serve(&mut self, req: Request<B>) -> Result<Responce<Self::Body>, Error>;
}

/// A body streaming trait alias.
pub trait BodyStream: Stream<Item = Result<Bytes, Error>> + 'static {}
impl<B> BodyStream for B where B: Stream<Item = Result<Bytes, Error>> + 'static {}

#[derive(Debug)]
pub struct Request<B> {
    head: request::Parts,
    body: B,
}

impl<B> Request<B> {
    pub fn new<U>(uri: U, method: Method, body: B) -> Self
    where
        U: Into<Uri>,
    {
        let (mut head, body) = http::Request::new(body).into_parts();
        head.method = method;
        head.uri = uri.into();
        Self { head, body }
    }

    pub(crate) fn version_mut(&mut self) -> &mut Version {
        &mut self.head.version
    }

    pub fn headers(&self) -> &HeaderMap {
        &self.head.headers
    }

    pub fn headers_mut(&mut self) -> &mut HeaderMap {
        &mut self.head.headers
    }
}

impl<B> From<Request<B>> for http::Request<B> {
    fn from(Request { head, body }: Request<B>) -> Self {
        Self::from_parts(head, body)
    }
}

pub struct Body(Pin<Box<dyn BodyStream>>);

impl fmt::Debug for Body {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_tuple("Body").field(&"..").finish()
    }
}

impl Stream for Body {
    type Item = Result<Bytes, Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        match self.get_mut() {
            Self(stream) => Pin::new(stream).poll_next(cx),
        }
    }
}

#[derive(Debug)]
pub struct Responce<B = Body> {
    head: response::Parts,
    body: B,
}

impl<B> Responce<B> {
    pub fn new(res: http::Response<B>) -> Self
    where
        B: BodyStream,
    {
        let (head, body) = res.into_parts();
        Self { head, body }
    }

    pub fn boxed(self) -> Responce
    where
        B: BodyStream,
    {
        Responce {
            head: self.head,
            body: Body(Box::pin(self.body)),
        }
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

    pub fn body_stream(self) -> B
    where
        B: BodyStream,
    {
        self.body
    }

    pub fn body_reader(self) -> impl AsyncRead
    where
        B: BodyStream,
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

        let stream = self.body_stream().map(|res| res.map_err(Error::into));
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
