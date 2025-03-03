use {
    crate::body::{BoxedLocal, prelude::*},
    bytes::Bytes,
    futures_lite::prelude::*,
    http::{
        HeaderMap, Method, StatusCode, Uri, Version, request, response,
        uri::{Authority, Scheme},
    },
    std::{
        borrow::Cow,
        error, fmt,
        io::{self, ErrorKind},
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
        let authority = uri.authority().ok_or(InvalidUri::InvalidHost)?;
        Self::new(scheme, authority)
    }

    pub fn new(scheme: &Scheme, authority: &Authority) -> Result<Self, InvalidUri> {
        if scheme != &Scheme::HTTP && scheme != &Scheme::HTTPS {
            return Err(InvalidUri::NonHttpScheme);
        }

        let host = Host::parse(authority.host()).map_err(|_| InvalidUri::InvalidHost)?;

        let secure = scheme == &Scheme::HTTPS;
        let port = authority
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

    if secure { HTTPS } else { HTTP }
}

#[derive(Debug)]
pub enum InvalidUri {
    NoScheme,
    NonHttpScheme,
    InvalidHost,
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
            Self::InvalidHost => write!(f, "invalid host"),
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
    type Body: Body<Chunk = Bytes>;

    #[expect(async_fn_in_trait)]
    async fn send(&mut self, req: Request<B>) -> Result<Response<Self::Body>, Error>;
}

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

#[derive(Debug)]
pub struct Response<B = BoxedLocal<'static, Bytes>> {
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

    pub fn boxed(self) -> Response
    where
        B: Body<Chunk = Bytes> + 'static,
    {
        Response {
            head: self.head,
            body: self.body.boxed_local(),
        }
    }

    pub fn body(self) -> B
    where
        B: Body,
    {
        self.body
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
