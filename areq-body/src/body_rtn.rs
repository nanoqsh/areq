use crate::body::{Body, BodyExt, Boxed, IntoBody};

pub trait SendBody: Body<Chunk: Send, chunk(..): Send> + Send {}
impl<B> SendBody for B where B: Body<Chunk: Send, chunk(..): Send> + Send {}

pub trait BodyExtRtn: IntoBody {
    #[inline]
    fn boxed<'body>(self) -> Boxed<'body, Self::Chunk>
    where
        Self::Body: SendBody,
        Self: 'body,
    {
        Box::pin(self.into_poll_body())
    }
}

impl<B> BodyExtRtn for B where B: IntoBody {}
