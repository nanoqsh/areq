use {
    crate::io::AsyncIo,
    std::{error, fmt, future::Future, io},
};

/// Used HTTP protocol.
pub trait Protocol {
    const SECURITY: Security;

    type Connection;

    #[allow(async_fn_in_trait)]
    async fn connect<'ex, S, I>(&self, spawn: &S, io: I) -> Result<Self::Connection, Error>
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

/// Trait for a task spawner.
pub trait Spawn<'ex> {
    fn spawn<T>(&self, task: T)
    where
        T: Task<'ex>;
}
