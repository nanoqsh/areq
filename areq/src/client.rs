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

pub trait ClientExt<'body, U>: Client<Boxed<'body>> {
    async fn get(&mut self, uri: U) -> Result<Response<Self::Body>, Error>;
    async fn head(&mut self, uri: U) -> Result<Response<Self::Body>, Error>;
    async fn post(&mut self, uri: U, body: Boxed<'body>) -> Result<Response<Self::Body>, Error>;
    async fn put(&mut self, uri: U, body: Boxed<'body>) -> Result<Response<Self::Body>, Error>;
    async fn delete(&mut self, uri: U, body: Boxed<'body>) -> Result<Response<Self::Body>, Error>;
    async fn options(&mut self, uri: U, body: Boxed<'body>) -> Result<Response<Self::Body>, Error>;
    async fn patch(&mut self, uri: U, body: Boxed<'body>) -> Result<Response<Self::Body>, Error>;
}

impl<'body, C, U> ClientExt<'body, U> for C
where
    C: Client<Boxed<'body>>,
    U: Into<Uri>,
{
    async fn get(&mut self, uri: U) -> Result<Response<Self::Body>, Error> {
        let req = Request::new(Method::GET, uri, Boxed::default());
        self.send(req).await
    }

    async fn head(&mut self, uri: U) -> Result<Response<Self::Body>, Error> {
        let req = Request::new(Method::HEAD, uri, Boxed::default());
        self.send(req).await
    }

    async fn post(&mut self, uri: U, body: Boxed<'body>) -> Result<Response<Self::Body>, Error> {
        let req = Request::new(Method::POST, uri, body);
        self.send(req).await
    }

    async fn put(&mut self, uri: U, body: Boxed<'body>) -> Result<Response<Self::Body>, Error> {
        let req = Request::new(Method::PUT, uri, body);
        self.send(req).await
    }

    async fn delete(&mut self, uri: U, body: Boxed<'body>) -> Result<Response<Self::Body>, Error> {
        let req = Request::new(Method::DELETE, uri, body);
        self.send(req).await
    }

    async fn options(&mut self, uri: U, body: Boxed<'body>) -> Result<Response<Self::Body>, Error> {
        let req = Request::new(Method::OPTIONS, uri, body);
        self.send(req).await
    }

    async fn patch(&mut self, uri: U, body: Boxed<'body>) -> Result<Response<Self::Body>, Error> {
        let req = Request::new(Method::PATCH, uri, body);
        self.send(req).await
    }
}
