use {
    crate::body::IntoBody,
    bytes::Bytes,
    futures_lite::{AsyncRead, Stream, StreamExt},
    http::{request, response, uri::Scheme, HeaderMap, Method, StatusCode, Uri, Version},
    std::{
        borrow::Cow,
        error, fmt,
        future::Future,
        io::{self, ErrorKind},
        pin::Pin,
        task::{Context, Poll},
    },
    url::Host,
};

/// The communication session between a client and a host.
pub struct Session<I> {
    pub addr: Address,
    pub io: I,
}

/// The network address.
pub struct Address {
    pub host: Host,
    pub port: u16,
    pub secure: bool,
}

impl Address {
    /// Creates new address from [uri](Uri).
    ///
    /// # Errors
    /// Returns [`InvalidUri`] if url is not valid.
    pub fn from_uri(uri: &Uri) -> Result<Self, InvalidUri> {
        let scheme = uri.scheme().ok_or(InvalidUri::NoScheme)?;
        if scheme != &Scheme::HTTP && scheme != &Scheme::HTTPS {
            return Err(InvalidUri::NonHttpScheme);
        }

        let host = uri
            .host()
            .and_then(|host| Host::parse(host).ok())
            .ok_or(InvalidUri::Host)?;

        let secure = scheme == &Scheme::HTTPS;
        let port = uri
            .port()
            .map(|port| port.as_u16())
            .unwrap_or(default_port(secure));

        Ok(Self { host, port, secure })
    }

    /// Returns a representation of the host and port based on the security protocol
    pub fn repr(&self) -> Cow<'_, str> {
        if self.port == default_port(self.secure) {
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
}

fn default_port(secure: bool) -> u16 {
    const HTTP: u16 = 80;
    const HTTPS: u16 = 443;

    if secure {
        HTTPS
    } else {
        HTTP
    }
}

#[derive(Debug)]
pub enum InvalidUri {
    NoScheme,
    NonHttpScheme,
    Host,
}

impl From<InvalidUri> for io::Error {
    fn from(e: InvalidUri) -> Self {
        Self::new(ErrorKind::InvalidInput, e)
    }
}

impl fmt::Display for InvalidUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoScheme => write!(f, "no scheme"),
            Self::NonHttpScheme => write!(f, "non http(s) scheme"),
            Self::Host => write!(f, "invalid host"),
        }
    }
}

impl error::Error for InvalidUri {}

/// The trait to establish a client session over an asynchronous connection.
pub trait Handshake<I> {
    /// The client type returned by the handshake process.
    type Client<B>: Client<B>
    where
        B: IntoBody;

    /// Perform a handshake to establish a client session.
    #[expect(async_fn_in_trait)]
    async fn handshake<B>(self, se: Session<I>) -> Result<(Self::Client<B>, impl Future), Error>
    where
        B: IntoBody;
}

/// The [handshake](Handshake) error type.
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    InvalidHost,
    UnsupportedProtocol(Box<[u8]>),
}

impl Error {
    pub fn try_into_io(self) -> Result<io::Error, Self> {
        match self {
            Self::Io(e) => Ok(e),
            e => Err(e),
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::InvalidHost => write!(f, "invalid host"),
            Self::UnsupportedProtocol(proto) => {
                write!(f, "unsupported protocol: ")?;
                for chunk in proto.utf8_chunks() {
                    write!(f, "{}", chunk.valid())?;
                    if !chunk.invalid().is_empty() {
                        write!(f, "{}", char::REPLACEMENT_CHARACTER)?;
                    }
                }

                Ok(())
            }
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::InvalidHost => None,
            Self::UnsupportedProtocol(_) => None,
        }
    }
}

pub trait Client<B> {
    type Body: BodyStream;

    #[expect(async_fn_in_trait)]
    async fn send(&mut self, req: Request<B>) -> Result<Response<Self::Body>, Error>;
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
    pub fn new<U>(method: Method, uri: U, body: B) -> Self
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

    pub fn method(&self) -> &Method {
        &self.head.method
    }

    pub fn method_mut(&mut self) -> &mut Method {
        &mut self.head.method
    }

    pub fn uri(&self) -> &Uri {
        &self.head.uri
    }

    pub fn headers(&self) -> &HeaderMap {
        &self.head.headers
    }

    pub fn headers_mut(&mut self) -> &mut HeaderMap {
        &mut self.head.headers
    }

    pub fn map<F, C>(self, f: F) -> Request<C>
    where
        F: FnOnce(B) -> C,
    {
        Request {
            head: self.head,
            body: f(self.body),
        }
    }
}

impl<B> From<Request<B>> for http::Request<B> {
    fn from(Request { head, body }: Request<B>) -> Self {
        Self::from_parts(head, body)
    }
}

impl<B> From<http::Request<B>> for Request<B> {
    fn from(req: http::Request<B>) -> Self {
        let (head, body) = req.into_parts();
        Self { head, body }
    }
}

pub struct BoxedBody(Pin<Box<dyn BodyStream>>);

impl fmt::Debug for BoxedBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Body").field(&"..").finish()
    }
}

impl Stream for BoxedBody {
    type Item = Result<Bytes, Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.get_mut() {
            Self(stream) => Pin::new(stream).poll_next(cx),
        }
    }
}

#[derive(Debug)]
pub struct Response<B = BoxedBody> {
    head: response::Parts,
    body: B,
}

impl<B> Response<B> {
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

    pub fn map<F, C>(self, f: F) -> Response<C>
    where
        F: FnOnce(B) -> C,
    {
        Response {
            head: self.head,
            body: f(self.body),
        }
    }
}

impl<B> Response<B>
where
    B: BodyStream,
{
    pub fn new(res: http::Response<B>) -> Self {
        let (head, body) = res.into_parts();
        Self { head, body }
    }

    pub fn boxed(self) -> Response {
        Response {
            head: self.head,
            body: BoxedBody(Box::pin(self.body)),
        }
    }

    pub fn body(self) -> B {
        self.body
    }

    pub fn body_reader(self) -> impl AsyncRead {
        use std::{
            pin::Pin,
            task::{Context, Poll},
        };

        pin_project_lite::pin_project! {
            struct Reader<S> {
                #[pin]
                stream: S,
                bytes: Bytes,
            }
        }

        impl<S> AsyncRead for Reader<S>
        where
            S: Stream<Item = Result<Bytes, io::Error>>,
        {
            fn poll_read(
                self: Pin<&mut Self>,
                cx: &mut Context<'_>,
                buf: &mut [u8],
            ) -> Poll<Result<usize, io::Error>> {
                if buf.is_empty() {
                    return Poll::Ready(Ok(0));
                }

                let me = self.project();

                if me.bytes.is_empty() {
                    match me.stream.poll_next(cx) {
                        Poll::Ready(Some(Ok(b))) if b.is_empty() => {
                            // if next bytes is empty skip this iteration and reschedule
                            cx.waker().wake_by_ref();
                            return Poll::Pending;
                        }
                        Poll::Ready(Some(Ok(b))) => *me.bytes = b,
                        Poll::Ready(Some(Err(e))) => return Poll::Ready(Err(e)),
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

        let stream = self.body().map(|res| res.map_err(Error::into));
        let bytes = Bytes::new();
        Reader { stream, bytes }
    }
}

impl<B> From<Response<B>> for http::Response<B> {
    fn from(Response { head, body }: Response<B>) -> Self {
        Self::from_parts(head, body)
    }
}

impl<B> From<http::Response<B>> for Response<B> {
    fn from(res: http::Response<B>) -> Self {
        let (head, body) = res.into_parts();
        Self { head, body }
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        futures_lite::{future, io, stream, AsyncReadExt},
    };

    #[test]
    fn body_into_stream() {
        let body = stream::iter(["foo", "bar", "baz"])
            .map(|part| Ok::<_, Error>(Bytes::copy_from_slice(part.as_bytes())));

        let res = Response::new(http::Response::new(body));
        let actual: Vec<_> = future::block_on(
            res.body()
                .map(|res| res.expect("all parts is ok"))
                .collect(),
        );

        assert_eq!(actual, ["foo", "bar", "baz"]);
    }

    #[test]
    fn body_into_reader() {
        let body = stream::iter(["foo", "bar", "baz"])
            .map(|part| Ok::<_, Error>(Bytes::copy_from_slice(part.as_bytes())));

        let res = Response::new(http::Response::new(body));
        let mut actual = vec![];
        future::block_on(io::copy(res.body_reader(), &mut actual)).expect("all parts is ok");
        assert_eq!(actual, b"foobarbaz");
    }

    #[test]
    fn body_into_reader_partial() {
        let body = stream::iter(["foo", "bar", "baz"])
            .map(|part| Ok::<_, Error>(Bytes::copy_from_slice(part.as_bytes())));

        let res = Response::new(http::Response::new(body));
        let mut reader = res.body_reader();

        let mut buf = [0; 2];
        let n = future::block_on(reader.read(&mut buf)).expect("read body part to the buffer");
        assert_eq!(n, 2);
        assert_eq!(&buf, b"fo");

        let mut buf = [0; 2];
        let n = future::block_on(reader.read(&mut buf)).expect("read body part to the buffer");
        assert_eq!(n, 1);
        assert_eq!(&buf, b"o\0");
    }
}
