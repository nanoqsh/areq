use {
    crate::proto::{Error, Handshake, Session},
    areq_body::IntoBody,
    futures_lite::{AsyncRead, AsyncWrite},
    futures_rustls::{client::TlsStream, pki_types::ServerName, TlsConnector},
    std::future::Future,
    url::Host,
};

pub struct Tls<H> {
    connector: TlsConnector,
    inner: H,
}

impl<I, H> Handshake<I> for Tls<H>
where
    I: AsyncRead + AsyncWrite + Unpin,
    H: Handshake<TlsStream<I>>,
{
    type Client<B>
        = H::Client<B>
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
        let alpn = conn.alpn_protocol().unwrap_or_default();
        println!("alpn: {}", String::from_utf8_lossy(alpn));

        let se = Session { addr, io: tls };
        let (client, conn) = self.inner.handshake(se).await?;
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
