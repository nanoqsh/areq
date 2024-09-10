use {
    crate::{body::Empty, conn::Connection, io::AsyncIo},
    hyper::{
        body::Incoming,
        http::{self, Method},
    },
    std::{error, fmt, future::Future, io},
    url::Host,
};

/// Represents a communication session between a client and a host.
pub struct Session<I> {
    pub io: I,
    pub host: Host,
    pub port: u16,
}

/// Used HTTP protocol.
pub trait Protocol {
    type Fetch: Fetch;

    const SECURITY: Security;

    #[expect(async_fn_in_trait)]
    async fn connect<'ex, S, I>(
        &self,
        spawn: &S,
        se: Session<I>,
    ) -> Result<Connection<Self>, Error>
    where
        S: Spawn<'ex>,
        I: AsyncIo + Send + 'ex;
}

/// The [protocol](Protocol) error type.
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Hyper(hyper::Error),
    InvalidHost,
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<hyper::Error> for Error {
    fn from(e: hyper::Error) -> Self {
        Self::Hyper(e)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::Hyper(e) => write!(f, "hyper error: {e}"),
            Self::InvalidHost => write!(f, "invalid host"),
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Hyper(e) => Some(e),
            Self::InvalidHost => None,
        }
    }
}

/// The property of a [protocol](Protocol) is it secure or not.
pub enum Security {
    No,
    Yes { alpn: &'static [&'static str] },
}

impl Security {
    pub const fn default_port(self) -> u16 {
        const HTTP: u16 = 80;
        const HTTPS: u16 = 443;

        match self {
            Self::No => HTTP,
            Self::Yes { .. } => HTTPS,
        }
    }

    #[expect(dead_code)]
    const fn alpn(self) -> &'static [&'static str] {
        match self {
            Self::No => panic!("this protocol must be secure"),
            Self::Yes { alpn } => alpn,
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
    inner: http::Request<Empty>,
}

impl Request {
    pub fn get(uri: &str) -> Self {
        let inner = http::Request::builder()
            .method(Method::GET)
            .uri(uri)
            .body(Empty)
            .expect("construct a valid request");

        Self { inner }
    }

    pub(crate) fn as_mut(&mut self) -> &mut http::Request<Empty> {
        &mut self.inner
    }

    pub(crate) fn into_inner(self) -> http::Request<Empty> {
        self.inner
    }
}

#[derive(Debug)]
pub struct Responce {
    #[expect(dead_code)]
    inner: http::Response<Incoming>,
}

impl Responce {
    pub(crate) fn new(inner: http::Response<Incoming>) -> Self {
        Self { inner }
    }
}
