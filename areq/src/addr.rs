use {
    crate::proto::Error,
    http::{
        Uri,
        uri::{Authority, Scheme},
    },
    std::{
        borrow::Cow,
        error, fmt,
        io::{self, ErrorKind},
        net::{IpAddr, Ipv4Addr, Ipv6Addr},
    },
    url::Host,
};

/// The network address.
#[derive(Clone, Debug)]
pub struct Address {
    pub host: Host,
    pub port: u16,
    pub secure: bool,
}

impl Address {
    pub fn new(scheme: &Scheme, authority: &Authority) -> Result<Self, InvalidUri> {
        if scheme != &Scheme::HTTP && scheme != &Scheme::HTTPS {
            return Err(InvalidUri::NonHttpScheme);
        }

        let host = Host::parse(authority.host()).map_err(|_| InvalidUri::InvalidHost)?;

        let secure = scheme == &Scheme::HTTPS;
        let port = authority
            .port()
            .map_or(default_port(secure), |port| port.as_u16());

        Ok(Self { host, port, secure })
    }

    pub fn http<H>(host: H) -> Self
    where
        H: IntoHost,
    {
        Self {
            host: host.into_host(),
            port: default_port(false),
            secure: false,
        }
    }

    pub fn https<H>(host: H) -> Self
    where
        H: IntoHost,
    {
        Self {
            host: host.into_host(),
            port: default_port(true),
            secure: true,
        }
    }

    /// Creates new address from [uri](Uri).
    ///
    /// # Errors
    /// Returns [`InvalidUri`] if url is not valid.
    pub fn from_uri(uri: &Uri) -> Result<Self, InvalidUri> {
        let scheme = uri.scheme().ok_or(InvalidUri::NoScheme)?;
        let authority = uri.authority().ok_or(InvalidUri::InvalidHost)?;
        Self::new(scheme, authority)
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

impl TryFrom<&Uri> for Address {
    type Error = InvalidUri;

    fn try_from(uri: &Uri) -> Result<Self, Self::Error> {
        Self::from_uri(uri)
    }
}

impl TryFrom<Uri> for Address {
    type Error = InvalidUri;

    fn try_from(uri: Uri) -> Result<Self, Self::Error> {
        Self::from_uri(&uri)
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

impl From<InvalidUri> for Error {
    fn from(e: InvalidUri) -> Self {
        Self::Io(e.into())
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

pub trait IntoHost {
    fn into_host(self) -> Host;
}

impl IntoHost for Host {
    fn into_host(self) -> Self {
        self
    }
}

impl IntoHost for String {
    fn into_host(self) -> Host {
        Host::Domain(self)
    }
}

impl IntoHost for &str {
    fn into_host(self) -> Host {
        Host::Domain(self.to_owned())
    }
}

impl IntoHost for Ipv4Addr {
    fn into_host(self) -> Host {
        Host::Ipv4(self)
    }
}

impl IntoHost for Ipv6Addr {
    fn into_host(self) -> Host {
        Host::Ipv6(self)
    }
}

impl IntoHost for IpAddr {
    fn into_host(self) -> Host {
        match self {
            Self::V4(ip4) => ip4.into_host(),
            Self::V6(ip6) => ip6.into_host(),
        }
    }
}
