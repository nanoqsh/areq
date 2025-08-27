use {
    crate::proto::{Error, Request, Response},
    areq_body::{Body, Boxed},
    bytes::Bytes,
    http::{Method, Uri},
};

pub trait Client<B> {
    type Body: Body<Chunk = Bytes>;

    async fn send(&mut self, req: Request<B>) -> Result<Response<Self::Body>, Error>;
}

pub trait ClientExt<'body>: Client<Boxed<'body>> {
    async fn get<U>(&mut self, uri: U) -> Result<Response<Self::Body>, Error>
    where
        U: Into<Uri>;

    async fn head<U>(&mut self, uri: U) -> Result<Response<Self::Body>, Error>
    where
        U: Into<Uri>;

    async fn post<U>(&mut self, uri: U, body: Boxed<'body>) -> Result<Response<Self::Body>, Error>
    where
        U: Into<Uri>;

    async fn put<U>(&mut self, uri: U, body: Boxed<'body>) -> Result<Response<Self::Body>, Error>
    where
        U: Into<Uri>;

    async fn delete<U>(
        &mut self,
        uri: U,
        body: Boxed<'body>,
    ) -> Result<Response<Self::Body>, Error>
    where
        U: Into<Uri>;

    async fn options<U>(
        &mut self,
        uri: U,
        body: Boxed<'body>,
    ) -> Result<Response<Self::Body>, Error>
    where
        U: Into<Uri>;

    async fn patch<U>(&mut self, uri: U, body: Boxed<'body>) -> Result<Response<Self::Body>, Error>
    where
        U: Into<Uri>;
}

impl<'body, C> ClientExt<'body> for C
where
    C: Client<Boxed<'body>>,
{
    fn get<U>(&mut self, uri: U) -> impl Future<Output = Result<Response<Self::Body>, Error>>
    where
        U: Into<Uri>,
    {
        let req = Request::new(Method::GET, uri, Boxed::default());
        self.send(req)
    }

    fn head<U>(&mut self, uri: U) -> impl Future<Output = Result<Response<Self::Body>, Error>>
    where
        U: Into<Uri>,
    {
        let req = Request::new(Method::HEAD, uri, Boxed::default());
        self.send(req)
    }

    fn post<U>(
        &mut self,
        uri: U,
        body: Boxed<'body>,
    ) -> impl Future<Output = Result<Response<Self::Body>, Error>>
    where
        U: Into<Uri>,
    {
        let req = Request::new(Method::POST, uri, body);
        self.send(req)
    }

    fn put<U>(
        &mut self,
        uri: U,
        body: Boxed<'body>,
    ) -> impl Future<Output = Result<Response<Self::Body>, Error>>
    where
        U: Into<Uri>,
    {
        let req = Request::new(Method::PUT, uri, body);
        self.send(req)
    }

    fn delete<U>(
        &mut self,
        uri: U,
        body: Boxed<'body>,
    ) -> impl Future<Output = Result<Response<Self::Body>, Error>>
    where
        U: Into<Uri>,
    {
        let req = Request::new(Method::DELETE, uri, body);
        self.send(req)
    }

    fn options<U>(
        &mut self,
        uri: U,
        body: Boxed<'body>,
    ) -> impl Future<Output = Result<Response<Self::Body>, Error>>
    where
        U: Into<Uri>,
    {
        let req = Request::new(Method::OPTIONS, uri, body);
        self.send(req)
    }

    fn patch<U>(
        &mut self,
        uri: U,
        body: Boxed<'body>,
    ) -> impl Future<Output = Result<Response<Self::Body>, Error>>
    where
        U: Into<Uri>,
    {
        let req = Request::new(Method::PATCH, uri, body);
        self.send(req)
    }
}

/// Asserts the client extension futures are `Send` despite a uri type is'nt `Send`.
#[cfg(feature = "rtn")]
fn _client_ext_futures_send<U>(uri: U)
where
    U: Into<Uri>,
{
    fn assert_send<S>(s: S) -> S
    where
        S: Send,
    {
        s
    }

    struct Mock;

    impl<B> Client<B> for Mock {
        type Body = Bytes;

        async fn send(&mut self, _: Request<B>) -> Result<Response<Self::Body>, Error> {
            unreachable!()
        }
    }

    // The client must be `Send`
    let mut client = assert_send(Mock);
    _ = assert_send(client.post(uri, Boxed::default()));
}
