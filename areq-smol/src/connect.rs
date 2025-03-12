use {
    areq::{Address, Error, HandshakeWith, Session},
    async_net::TcpStream,
    url::Host,
};

/// Extension trait to connect a [TCP stream](TcpStream).
///
/// If a connection is successful, it returns an HTTP client and a future
/// that needs to be polled in background while the client sends requests
/// and receives responses. To do this, you can spawn the task in an
/// [executor] or poll it concurrently with the client, for example,
/// using [`try_zip`].
///
/// [executor]: https://docs.rs/async-executor/latest/async_executor/struct.Executor.html
/// [`try_zip`]: https://docs.rs/futures-lite/latest/futures_lite/future/fn.try_zip.html
///
/// # Example
///
/// ```
/// use {
///     areq_smol::{areq::{http::Uri, http1::Http1}, prelude::*},
///     async_executor::Executor,
///     std::io::Error,
/// };
///
/// async fn get(ex: &Executor<'_>) -> Result<String, Error> {
///     let addr = Uri::from_static("http://127.0.0.1:3001");
///     let path = Uri::from_static("/hello");
///
///     // Establish a connection to the address
///     let (mut client, conn) = Http1::default().connect(addr).await?;
///
///     // Spawn the task in background
///     ex.spawn(conn).detach();
///
///     // Now you can work with the client
///     // The background task will complete once the client is dropped
///     client.get(path).await?.text().await
/// }
/// ```
///
/// You can also use an extension method [`handle`](crate::Handle::handle),
/// which takes an async closure, calls it on the client, and polls the task
/// in background for the entire duration of the closure execution.
///
/// ```
/// use {
///     areq_smol::{areq::{http::Uri, http1::Http1}, prelude::*},
///     std::io::Error,
/// };
///
/// async fn get() -> Result<String, Error> {
///     let addr = Uri::from_static("http://127.0.0.1:3001");
///     let path = Uri::from_static("/hello");
///
///     Http1::default()
///         .connect(addr)
///         .await?
///         .handle(async |mut client| client.get(path).await?.text().await)
///         .await
/// }
/// ```
pub trait Connect<A, B>: HandshakeWith<TcpStream, B> {
    /// Connects to the given address.
    async fn connect(self, addr: A) -> Result<(Self::Client, Self::Task), Error>;
}

impl<H, A, B> Connect<A, B> for H
where
    A: TryInto<Address, Error: Into<Error>>,
    H: HandshakeWith<TcpStream, B>,
{
    #[inline]
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
