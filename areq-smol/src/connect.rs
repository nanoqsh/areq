use {
    areq::{Address, Error, HandshakeWith, Session},
    async_net::TcpStream,
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
        let io = TcpStream::connect(addr.repr().as_ref()).await?;
        let se = Session { addr, io };
        self.handshake(se).await
    }
}
