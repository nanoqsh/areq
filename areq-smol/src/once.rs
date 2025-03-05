//! Module for one-time requests.
//!
//! *Warning: Currently under development*
//!
//! A simple API for quickly creating a request without the need to manage
//! a client or maintain a connection. Whenever possible, use the full API, as
//! it is more optimized and flexible.
//!
//! # Examples
//!
//! ```
//! # async fn request() -> Result<(), std::io::Error> {
//! let s = areq_smol::once::get("http://127.0.0.1:3001/hello")?
//!     .text()
//!     .await?;
//!
//! assert_eq!(s, "Hello, World!");
//! # Ok(())
//! # }
//! ```

use {
    areq::{
        Address, Handshake, Request, Session,
        body::{Boxed, SendBody},
        bytes::{Bytes, BytesMut},
        http::{Method, Uri},
        http1::Http1,
        prelude::*,
    },
    async_net::TcpStream,
    futures_lite::future,
    std::{error, io::Error},
};

pub fn get<U>(uri: U) -> Result<MakeRequest, Error>
where
    U: IntoUri,
{
    Ok(MakeRequest {
        method: Method::GET,
        uri: uri.into_uri()?,
        body: (),
    })
}

pub fn post<U, B>(uri: U, body: B) -> Result<MakeRequest<B>, Error>
where
    U: IntoUri,
    B: IntoBody,
{
    Ok(MakeRequest {
        method: Method::GET,
        uri: uri.into_uri()?,
        body,
    })
}

pub struct MakeRequest<B = ()> {
    method: Method,
    uri: Uri,
    body: B,
}

impl<B> MakeRequest<B>
where
    B: IntoBody<Body: SendBody> + Send,
{
    pub async fn text(self) -> Result<String, Error> {
        self.send(async |body| body.text().await).await
    }

    pub async fn bytes(self) -> Result<Bytes, Error> {
        self.send(async |body| body.bytes().await).await
    }

    pub async fn bytes_mut(self) -> Result<BytesMut, Error> {
        self.send(async |body| body.bytes_mut().await).await
    }

    async fn send<H, F, U>(self, h: H) -> Result<U, Error>
    where
        H: Fn(Boxed<'static, Bytes>) -> F,
        F: Future<Output = Result<U, Error>>,
    {
        let Self { method, uri, body } = self;

        let addr = Address::from_uri(&uri)?;
        let io = TcpStream::connect(addr.repr().as_ref()).await?;
        let se = Session { addr, io };

        let (mut client, conn) = Http1::default().handshake(se).await?;

        let handle_io = async {
            conn.await;
            Ok(())
        };

        let send_request = async move {
            let req = Request::new(method, uri, body);
            let res = client.send(req).await?;
            let body = res.body().boxed();
            h(body).await
        };

        Box::pin(future::try_zip(handle_io, send_request)) // box large futures
            .await
            .map(|(_, res)| res)
    }
}

pub trait IntoUri {
    fn into_uri(self) -> Result<Uri, Error>;
}

impl<U> IntoUri for U
where
    U: TryInto<Uri, Error: error::Error + Send + Sync + 'static>,
{
    fn into_uri(self) -> Result<Uri, Error> {
        self.try_into().map_err(Error::other)
    }
}
