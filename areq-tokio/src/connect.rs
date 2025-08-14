use {
    crate::io::Io,
    areq::{Address, Error, HandshakeWith, Session},
    tokio::net::TcpStream,
    url::Host,
};

/// Extension trait to connect tokio [TCP stream](TcpStream).
///
/// If a connection is successful, the [`connect`](Connect::connect) method
/// returns an HTTP client and a future that needs to be polled in background
/// while the client sends requests and receives responses.
/// The simplest way to do this is to call
/// [`tokio::spawn`](https://docs.rs/tokio/latest/tokio/task/fn.spawn.html).
///
/// # Example
///
/// ```
/// use {
///     areq_tokio::{http::Uri, http1::Http1, prelude::*},
///     std::io::Error,
/// };
///
/// async fn get() -> Result<String, Error> {
///     let uri = Uri::from_static("http://127.0.0.1:3001/hello");
///
///     // Establish a connection to the address
///     let (mut client, conn) = Http1::default().connect(uri.clone()).await?;
///
///     // Spawn the task in background
///     tokio::spawn(conn);
///
///     // Now you can work with the client
///     // The background task will complete once the client is dropped
///     client.get(uri).await?.text().await
/// }
/// ```
pub trait Connect<A, B>: HandshakeWith<Io<TcpStream>, B> {
    /// Connects to the given address.
    async fn connect(self, addr: A) -> Result<(Self::Client, Self::Task), Error>;
}

impl<H, A, B> Connect<A, B> for H
where
    A: TryInto<Address, Error: Into<Error>>,
    H: HandshakeWith<Io<TcpStream>, B, Task: Send + 'static>,
{
    #[inline]
    async fn connect(self, addr: A) -> Result<(Self::Client, Self::Task), Error> {
        let addr = addr.try_into().map_err(A::Error::into)?;
        let io = match &addr.host {
            Host::Domain(d) => TcpStream::connect((d.as_str(), addr.port)).await?,
            Host::Ipv4(ip4) => TcpStream::connect((*ip4, addr.port)).await?,
            Host::Ipv6(ip6) => TcpStream::connect((*ip6, addr.port)).await?,
        };

        let se = Session {
            addr,
            io: Io::new(io),
        };

        self.handshake(se).await
    }
}
