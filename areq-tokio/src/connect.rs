use {
    areq::{Address, Error, HandshakeWith, Session},
    tokio::net::TcpStream,
    url::Host,
};

pub trait Connect<A, B>: HandshakeWith<TcpStream, B> {
    #[expect(async_fn_in_trait)]
    async fn connect(self, addr: A) -> Result<(Self::Client, Self::Task), Error>;
}

impl<H, A, B> Connect<A, B> for H
where
    A: TryInto<Address, Error: Into<Error>>,
    H: HandshakeWith<TcpStream, B>,
{
    async fn connect(self, addr: A) -> Result<(Self::Client, Self::Task), Error> {
        let addr = addr.try_into().map_err(A::Error::into)?;
        let io = match &addr.host {
            Host::Domain(d) => TcpStream::connect((d.as_str(), addr.port)).await?,
            Host::Ipv4(ip4) => TcpStream::connect((*ip4, addr.port)).await?,
            Host::Ipv6(ip6) => TcpStream::connect((*ip6, addr.port)).await?,
        };

        let se = Session { addr, io };
        self.handshake(se).await
    }
}
