use {
    crate::{
        addr::Address,
        body::{Boxed, prelude::*},
        client::Client,
    },
    futures_lite::prelude::*,
    http::{HeaderMap, Method, StatusCode, Uri, Version, request, response},
    std::{convert::Infallible, error, fmt, io},
};

#[cfg(feature = "rtn")]
pub use crate::proto_rtn::HandshakeWith;

/// The communication session between a client and a host.
#[derive(Clone, Debug)]
pub struct Session<I> {
    pub addr: Address,
    pub io: I,
}

/// The trait to establish a client session over an asynchronous connection.
pub trait Handshake<I, B> {
    /// The client type returned by the handshake process.
    type Client: Client<B>;

    /// Perform a handshake to establish a client session.
    async fn handshake(
        self,
        se: Session<I>,
    ) -> Result<(Self::Client, impl Future<Output = ()>), Error>;
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

impl From<Error> for io::Error {
    fn from(e: Error) -> Self {
        e.try_into_io().unwrap_or_else(Self::other)
    }
}

impl From<Infallible> for Error {
    fn from(e: Infallible) -> Self {
        match e {}
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

#[derive(Debug)]
pub struct Request<B = Boxed<'static>> {
    head: request::Parts,
    body: B,
}

impl<B> Request<B> {
    pub fn new(method: Method, uri: Uri, body: B) -> Self {
        let (mut head, body) = http::Request::new(body).into_parts();
        head.method = method;
        head.uri = uri;
        Self { head, body }
    }

    pub fn get(uri: Uri, body: B) -> Self {
        Self::new(Method::GET, uri, body)
    }

    pub fn head(uri: Uri, body: B) -> Self {
        Self::new(Method::HEAD, uri, body)
    }

    pub fn post(uri: Uri, body: B) -> Self {
        Self::new(Method::POST, uri, body)
    }

    pub fn put(uri: Uri, body: B) -> Self {
        Self::new(Method::PUT, uri, body)
    }

    pub fn delete(uri: Uri, body: B) -> Self {
        Self::new(Method::DELETE, uri, body)
    }

    pub fn options(uri: Uri, body: B) -> Self {
        Self::new(Method::OPTIONS, uri, body)
    }

    pub fn patch(uri: Uri, body: B) -> Self {
        Self::new(Method::PATCH, uri, body)
    }

    #[cfg(any(feature = "http1", feature = "http2"))]
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

    pub fn map_body<F, C>(self, f: F) -> Request<C>
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

impl<B> IntoBody for Request<B>
where
    B: IntoBody,
{
    type Chunk = B::Chunk;
    type Body = B::Body;

    fn into_body(self) -> Self::Body {
        self.body.into_body()
    }
}

#[derive(Debug)]
pub struct Response<B = Boxed<'static>> {
    head: response::Parts,
    body: B,
}

impl<B> Response<B> {
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

impl<B> IntoBody for Response<B>
where
    B: IntoBody,
{
    type Chunk = B::Chunk;
    type Body = B::Body;

    fn into_body(self) -> Self::Body {
        self.body.into_body()
    }
}
