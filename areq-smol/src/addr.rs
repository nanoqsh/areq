use {
    areq::{Address, Error, Session},
    async_net::TcpStream,
};

pub trait AddressExt {
    #[allow(async_fn_in_trait)]
    async fn connect(self) -> Result<Session<TcpStream>, Error>;
}

impl AddressExt for Address {
    async fn connect(self) -> Result<Session<TcpStream>, Error> {
        let io = TcpStream::connect(self.repr().as_ref()).await?;
        Ok(Session { addr: self, io })
    }
}
