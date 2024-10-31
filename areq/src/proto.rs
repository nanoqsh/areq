use {
    crate::client::Client,
    bytes::Bytes,
    futures_lite::{AsyncRead, AsyncWrite, Stream},
    http::{response::Parts, Method},
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

    #[expect(async_fn_in_trait)]
    async fn connect<'ex, S, I>(&self, spawn: &S, se: Session<I>) -> Result<Client<Self>, Error>
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
pub trait Spawn<'ex> {
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
    #[expect(dead_code)]
    head: Parts,
    body: B,
}

impl<B> Responce<B> {
    pub fn new(res: http::Response<B>) -> Self {
        let (head, body) = res.into_parts();
        Self { head, body }
    }

    pub fn into_stream<E>(self) -> impl Stream<Item = Result<Bytes, Error>>
    where
        B: Stream<Item = Result<Bytes, E>>,
        E: Into<Error>,
    {
        use futures_lite::StreamExt;

        self.body.map(|res| res.map_err(E::into))
    }
}
