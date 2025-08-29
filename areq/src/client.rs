use {
    crate::{
        body::{Body, IntoRequestBody},
        proto::{Error, Request, Response},
    },
    bytes::Bytes,
    http::Uri,
};

pub trait Client<B> {
    type Body: Body<Chunk = Bytes>;

    async fn send(&mut self, req: Request<B>) -> Result<Response<Self::Body>, Error>;

    fn try_clone(&self) -> Option<Self>
    where
        Self: Sized,
    {
        None
    }
}

pub trait ClientExt<B>: Client<B> {
    async fn get<I>(&mut self, uri: Uri, body: I) -> Result<Response<Self::Body>, Error>
    where
        I: IntoRequestBody<Body = B>,
    {
        self.send(Request::get(uri, body)).await
    }

    async fn head<I>(&mut self, uri: Uri, body: I) -> Result<Response<Self::Body>, Error>
    where
        I: IntoRequestBody<Body = B>,
    {
        self.send(Request::head(uri, body)).await
    }

    async fn post<I>(&mut self, uri: Uri, body: I) -> Result<Response<Self::Body>, Error>
    where
        I: IntoRequestBody<Body = B>,
    {
        self.send(Request::post(uri, body)).await
    }

    async fn put<I>(&mut self, uri: Uri, body: I) -> Result<Response<Self::Body>, Error>
    where
        I: IntoRequestBody<Body = B>,
    {
        self.send(Request::put(uri, body)).await
    }

    async fn delete<I>(&mut self, uri: Uri, body: I) -> Result<Response<Self::Body>, Error>
    where
        I: IntoRequestBody<Body = B>,
    {
        self.send(Request::delete(uri, body)).await
    }

    async fn options<I>(&mut self, uri: Uri, body: I) -> Result<Response<Self::Body>, Error>
    where
        I: IntoRequestBody<Body = B>,
    {
        self.send(Request::options(uri, body)).await
    }

    async fn patch<I>(&mut self, uri: Uri, body: I) -> Result<Response<Self::Body>, Error>
    where
        I: IntoRequestBody<Body = B>,
    {
        self.send(Request::patch(uri, body)).await
    }
}

impl<C, B> ClientExt<B> for C where C: Client<B> {}
