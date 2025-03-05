use {
    crate::proto::{Error, Request, Response},
    areq_body::Body,
    bytes::Bytes,
    http::{Method, Uri},
};

pub trait Client<B> {
    type Body: Body<Chunk = Bytes>;

    #[expect(async_fn_in_trait)]
    async fn send(&mut self, req: Request<B>) -> Result<Response<Self::Body>, Error>;
}

pub trait ClientWithoutBodyExt<U>: Client<()> {
    async fn get(&mut self, uri: U) -> Result<Response<Self::Body>, Error>;
    async fn head(&mut self, uri: U) -> Result<Response<Self::Body>, Error>;
}

impl<C, U> ClientWithoutBodyExt<U> for C
where
    C: Client<()>,
    U: Into<Uri>,
{
    async fn get(&mut self, uri: U) -> Result<Response<Self::Body>, Error> {
        self.send(Request::new(Method::GET, uri, ())).await
    }

    async fn head(&mut self, uri: U) -> Result<Response<Self::Body>, Error> {
        self.send(Request::new(Method::HEAD, uri, ())).await
    }
}

pub trait ClientExt<B, U>: Client<B> {
    async fn post(&mut self, uri: U, body: B) -> Result<Response<Self::Body>, Error>;
    async fn put(&mut self, uri: U, body: B) -> Result<Response<Self::Body>, Error>;
    async fn delete(&mut self, uri: U, body: B) -> Result<Response<Self::Body>, Error>;
    async fn options(&mut self, uri: U, body: B) -> Result<Response<Self::Body>, Error>;
    async fn patch(&mut self, uri: U, body: B) -> Result<Response<Self::Body>, Error>;
}

impl<C, B, U> ClientExt<B, U> for C
where
    C: Client<B>,
    U: Into<Uri>,
{
    async fn post(&mut self, uri: U, body: B) -> Result<Response<Self::Body>, Error> {
        self.send(Request::new(Method::POST, uri, body)).await
    }

    async fn put(&mut self, uri: U, body: B) -> Result<Response<Self::Body>, Error> {
        self.send(Request::new(Method::PUT, uri, body)).await
    }

    async fn delete(&mut self, uri: U, body: B) -> Result<Response<Self::Body>, Error> {
        self.send(Request::new(Method::DELETE, uri, body)).await
    }

    async fn options(&mut self, uri: U, body: B) -> Result<Response<Self::Body>, Error> {
        self.send(Request::new(Method::OPTIONS, uri, body)).await
    }

    async fn patch(&mut self, uri: U, body: B) -> Result<Response<Self::Body>, Error> {
        self.send(Request::new(Method::PATCH, uri, body)).await
    }
}
