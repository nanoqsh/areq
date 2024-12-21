use {
    crate::{
        http1::Http1,
        http2::Http2,
        or::Or,
        proto::{Error, Handshake, Session},
    },
    areq_body::IntoBody,
    futures_lite::{AsyncRead, AsyncWrite},
    futures_rustls::{
        client::TlsStream,
        pki_types::ServerName,
        rustls::{ClientConfig, RootCertStore},
        TlsConnector,
    },
    std::{future::Future, io, sync::Arc},
    url::Host,
};

pub trait Negotiate<I> {
    type Handshake: Handshake<I>;
    fn negotiate(self, proto: &[u8]) -> Option<Self::Handshake>;
}

pub struct Select<L, R>(pub L, pub R);

impl<I, L, R> Negotiate<I> for Select<L, R>
where
    L: Negotiate<I>,
    R: Negotiate<I>,
{
    type Handshake = Or<L::Handshake, R::Handshake>;

    fn negotiate(self, proto: &[u8]) -> Option<Self::Handshake> {
        let Self(l, r) = self;

        l.negotiate(proto)
            .map(Or::lhs)
            .or_else(|| r.negotiate(proto).map(Or::rhs))
    }
}

pub struct Tls<H> {
    connector: TlsConnector,
    inner: H,
}

impl Tls<Select<Http1, Http2>> {
    pub fn from_cert(cert: &[u8], http1: Http1, http2: Http2) -> Result<Self, Error> {
        let conf = read_tls_config(cert, ["http/1.1", "h2"])?;
        let connector = TlsConnector::from(Arc::new(conf));
        let inner = Select(http1, http2);
        Ok(Self::with_connector(connector, inner))
    }
}

impl<H> Tls<H> {
    pub fn with_connector(connector: TlsConnector, inner: H) -> Self {
        Self { connector, inner }
    }
}

impl<I, H> Handshake<I> for Tls<H>
where
    I: AsyncRead + AsyncWrite + Unpin,
    H: Negotiate<TlsStream<I>>,
{
    type Client<B>
        = <H::Handshake as Handshake<TlsStream<I>>>::Client<B>
    where
        B: IntoBody;

    async fn handshake<B>(self, se: Session<I>) -> Result<(Self::Client<B>, impl Future), Error>
    where
        B: IntoBody,
    {
        let Session { addr, io } = se;

        let name = as_server_name(&addr.host)?.to_owned();
        let tls = self.connector.connect(name, io).await?;

        let (_, conn) = tls.get_ref();
        let proto = conn.alpn_protocol().unwrap_or_default();

        // TODO: log
        // println!("! alpn proto: {}", String::from_utf8_lossy(proto));

        let handshake = self
            .inner
            .negotiate(proto)
            .ok_or_else(|| Error::UnsupportedProtocol(Box::from(proto)))?;

        let se = Session { addr, io: tls };
        let (client, conn) = handshake.handshake(se).await?;
        Ok((client, conn))
    }
}

fn as_server_name(host: &Host) -> Result<ServerName, Error> {
    match host {
        Host::Domain(domain) => {
            ServerName::try_from(domain.as_str()).map_err(|_| Error::InvalidHost)
        }
        Host::Ipv4(ip) => Ok(ServerName::from(*ip)),
        Host::Ipv6(ip) => Ok(ServerName::from(*ip)),
    }
}

fn read_tls_config<P>(mut cert: &[u8], protos: P) -> Result<ClientConfig, io::Error>
where
    P: IntoIterator,
    Vec<u8>: From<P::Item>,
{
    let mut root = RootCertStore::empty();
    for cert in rustls_pemfile::certs(&mut cert) {
        root.add(cert?).map_err(io::Error::other)?;
    }

    let mut conf = ClientConfig::builder()
        .with_root_certificates(root)
        .with_no_client_auth();

    conf.alpn_protocols
        .extend(protos.into_iter().map(Vec::from));

    Ok(conf)
}
