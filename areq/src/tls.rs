//! The http client over TLS.

use {
    crate::proto::{Error, Handshake, Session, Task},
    futures_lite::prelude::*,
    futures_rustls::{
        TlsConnector,
        client::TlsStream,
        pki_types::ServerName,
        rustls::{ClientConfig, RootCertStore},
    },
    std::{io, sync::Arc},
    url::Host,
};

pub use crate::negotiate::{Negotiate, Select};

pub struct Tls<N> {
    inner: N,
    connector: TlsConnector,
}

impl<N> Tls<N> {
    pub fn with_cert(inner: N, cert: &[u8]) -> Result<Self, Error>
    where
        N: Negotiate,
    {
        let conf = read_tls_config(cert, inner.support())?;
        let connector = TlsConnector::from(Arc::new(conf));
        Ok(Self::with_connector(inner, connector))
    }

    pub fn with_connector(inner: N, connector: TlsConnector) -> Self {
        Self { inner, connector }
    }
}

impl<I, B, N> Handshake<I, B> for Tls<N>
where
    I: AsyncRead + AsyncWrite + Unpin,
    N: Negotiate<Handshake: Handshake<TlsStream<I>, B>>,
{
    type Client = <N::Handshake as Handshake<TlsStream<I>, B>>::Client;

    async fn handshake(self, se: Session<I>) -> Result<(Self::Client, impl Task), Error> {
        let Session { addr, io } = se;

        let name = as_server_name(&addr.host)?.to_owned();
        let tls = self.connector.connect(name, io).await?;

        let (_, conn) = tls.get_ref();
        let proto = conn
            .alpn_protocol()
            // if the remote server doesn't specify a protocol,
            // fall back to the first supported one by default
            .unwrap_or_else(|| self.inner.support().next().unwrap_or_default());

        let handshake = self
            .inner
            .negotiate(proto)
            .ok_or_else(|| Error::UnsupportedProtocol(Box::from(proto)))?;

        let se = Session { addr, io: tls };
        let (client, conn) = handshake.handshake(se).await?;
        Ok((client, conn))
    }
}

fn as_server_name(host: &Host) -> Result<ServerName<'_>, Error> {
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
    P: Iterator<Item = &'static [u8]>,
{
    let mut root = RootCertStore::empty();
    for cert in rustls_pemfile::certs(&mut cert) {
        root.add(cert?).map_err(io::Error::other)?;
    }

    let mut conf = ClientConfig::builder()
        .with_root_certificates(root)
        .with_no_client_auth();

    conf.alpn_protocols.extend(protos.map(Vec::from));

    Ok(conf)
}
