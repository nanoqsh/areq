use {
    crate::proto::{Error, Request, Response},
    areq_body::Body,
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
    async fn get(&mut self, uri: Uri, body: B) -> Result<Response<Self::Body>, Error>;
    async fn head(&mut self, uri: Uri, body: B) -> Result<Response<Self::Body>, Error>;
    async fn post(&mut self, uri: Uri, body: B) -> Result<Response<Self::Body>, Error>;
    async fn put(&mut self, uri: Uri, body: B) -> Result<Response<Self::Body>, Error>;
    async fn delete(&mut self, uri: Uri, body: B) -> Result<Response<Self::Body>, Error>;
    async fn options(&mut self, uri: Uri, body: B) -> Result<Response<Self::Body>, Error>;
    async fn patch(&mut self, uri: Uri, body: B) -> Result<Response<Self::Body>, Error>;
}

impl<C, B> ClientExt<B> for C
where
    C: Client<B>,
{
    async fn get(&mut self, uri: Uri, body: B) -> Result<Response<Self::Body>, Error> {
        self.send(Request::get(uri, body)).await
    }

    async fn head(&mut self, uri: Uri, body: B) -> Result<Response<Self::Body>, Error> {
        self.send(Request::head(uri, body)).await
    }

    async fn post(&mut self, uri: Uri, body: B) -> Result<Response<Self::Body>, Error> {
        self.send(Request::post(uri, body)).await
    }

    async fn put(&mut self, uri: Uri, body: B) -> Result<Response<Self::Body>, Error> {
        self.send(Request::put(uri, body)).await
    }

    async fn delete(&mut self, uri: Uri, body: B) -> Result<Response<Self::Body>, Error> {
        self.send(Request::delete(uri, body)).await
    }

    async fn options(&mut self, uri: Uri, body: B) -> Result<Response<Self::Body>, Error> {
        self.send(Request::options(uri, body)).await
    }

    async fn patch(&mut self, uri: Uri, body: B) -> Result<Response<Self::Body>, Error> {
        self.send(Request::patch(uri, body)).await
    }
}
