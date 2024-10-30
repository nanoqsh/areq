use {
    crate::conn::Requester,
    areq_h1::FetchBody,
    futures_io::{AsyncRead, AsyncWrite},
    http::Method,
    std::{borrow::Cow, error, fmt, future::Future, io},
    url::Host,
};

/// The communication session between a client and a host.
pub struct Session<I> {
    pub io: I,
    pub addr: Address,
}

/// The network address, which includes a host,
/// a port and an indicator of security protocol.
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

    #[expect(async_fn_in_trait)]
    async fn connect<'ex, S, I>(&self, spawn: &S, se: Session<I>) -> Result<Requester<Self>, Error>
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
    fn prepare_request(&self, req: &mut Request);
    async fn fetch(&mut self, req: Request) -> Result<Responce, Error>;
}

#[derive(Debug)]
pub struct Request {
    inner: http::Request<()>,
}

impl Request {
    pub fn get(uri: &str) -> Self {
        let inner = http::Request::builder()
            .method(Method::GET)
            .uri(uri)
            .body(())
            .expect("construct a valid request");

        Self { inner }
    }

    pub(crate) fn as_mut(&mut self) -> &mut http::Request<()> {
        &mut self.inner
    }

    pub(crate) fn into_inner(self) -> http::Request<()> {
        self.inner
    }
}

#[derive(Debug)]
pub struct Responce {
    #[expect(dead_code)]
    inner: http::Response<FetchBody>,
}

impl Responce {
    pub(crate) fn new(inner: http::Response<FetchBody>) -> Self {
        Self { inner }
    }
}
