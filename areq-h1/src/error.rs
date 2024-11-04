use {
    http::Version,
    std::{
        error, fmt,
        io::{self, ErrorKind},
    },
};

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Parse(httparse::Error),
    TooLargeInput,
    UnsupportedVersion(Version),
    Closed,
}

impl Error {
    #[inline]
    pub fn invalid_input() -> Self {
        Self::Io(ErrorKind::InvalidInput.into())
    }

    #[inline]
    pub fn unexpected_eof() -> Self {
        Self::Io(ErrorKind::UnexpectedEof.into())
    }

    #[inline]
    pub fn try_into_io(self) -> Result<io::Error, Self> {
        match self {
            Self::Io(e) => Ok(e),
            e => Err(e),
        }
    }
}

impl From<io::Error> for Error {
    #[inline]
    fn from(v: io::Error) -> Self {
        Self::Io(v)
    }
}

impl From<httparse::Error> for Error {
    #[inline]
    fn from(v: httparse::Error) -> Self {
        Self::Parse(v)
    }
}

impl From<Error> for io::Error {
    #[inline]
    fn from(e: Error) -> Self {
        e.try_into_io().unwrap_or_else(Self::other)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::Parse(e) => write!(f, "parse error: {e}"),
            Self::TooLargeInput => write!(f, "too large input"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported http version: {v:?}"),
            Self::Closed => write!(f, "connection closed"),
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Parse(e) => Some(e),
            Self::TooLargeInput => None,
            Self::UnsupportedVersion(_) => None,
            Self::Closed => None,
        }
    }
}
