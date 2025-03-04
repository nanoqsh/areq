//! Extension for the address type.

use {
    areq::{Address, Error, Session},
    async_net::TcpStream,
};

/// Extension trait for [address](Address).
pub trait AddressExt {
    /// Creates a new [session](Session)
    /// using smol's [TCP stream](TcpStream).
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn connect() -> Result<(), std::io::Error> {
    /// use areq_smol::{
    ///     areq::{Address, Session},
    ///     prelude::*, // imports `AddressExt`
    /// };
    ///
    /// let Session { io, .. } = Address::http("127.0.0.1").connect().await?;
    /// let remote_addr = io.peer_addr()?;
    /// println!("connected to {remote_addr}");
    /// # Ok(())
    /// # }
    /// ```
    #[allow(async_fn_in_trait)]
    async fn connect(self) -> Result<Session<TcpStream>, Error>;
}

impl AddressExt for Address {
    async fn connect(self) -> Result<Session<TcpStream>, Error> {
        let io = TcpStream::connect(self.repr().as_ref()).await?;
        Ok(Session { addr: self, io })
    }
}
