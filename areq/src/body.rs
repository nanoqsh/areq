//! Body types and traits.

pub use areq_body::*;

use {crate::Request, bytes::Buf};

pub trait IntoRequestBody {
    type Chunk: Buf;
    type Body: Body<Chunk = Self::Chunk>;

    fn into_req_body(self) -> Self::Body;

    fn upd_req(_: &mut Request<Self::Body>) {}
}

impl<I> IntoRequestBody for I
where
    I: IntoBody,
{
    type Chunk = I::Chunk;
    type Body = I::Body;

    fn into_req_body(self) -> Self::Body {
        self.into_body()
    }
}
