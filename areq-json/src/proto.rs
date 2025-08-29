use {
    areq::{
        Request,
        body::{Body, IntoRequestBody},
        http::{HeaderValue, header},
    },
    serde::Serialize,
    std::{io::Error, marker::PhantomData},
};

pub struct Json<T>
where
    T: ?Sized,
{
    buffer: String,
    ty: PhantomData<fn() -> T>,
}

impl<T> Json<T>
where
    T: ?Sized,
{
    pub fn new(t: &T) -> Result<Self, Error>
    where
        T: Serialize,
    {
        let buffer = serde_json::to_string(t)?;
        Ok(Self {
            buffer,
            ty: PhantomData,
        })
    }
}

impl<T> IntoRequestBody for Json<T>
where
    T: ?Sized,
{
    type Chunk = <String as Body>::Chunk;
    type Body = String;

    fn into_req_body(self) -> Self::Body {
        self.buffer
    }

    fn upd_req(req: &mut Request<Self::Body>) {
        req.headers_mut()
            .entry(header::CONTENT_TYPE)
            .or_insert(HeaderValue::from_static("application/json"));
    }
}

impl<'str, T> IntoRequestBody for &'str Json<T>
where
    T: ?Sized,
{
    type Chunk = <&'str str as Body>::Chunk;
    type Body = &'str str;

    fn into_req_body(self) -> Self::Body {
        &self.buffer
    }

    fn upd_req(req: &mut Request<Self::Body>) {
        req.headers_mut()
            .entry(header::CONTENT_TYPE)
            .or_insert(HeaderValue::from_static("application/json"));
    }
}
