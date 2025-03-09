use crate::body::{Body, BodyExt, Boxed, IntoBody};

/// Trait alias for a thread-safe [body](Body).
pub trait SendBody: Body<Chunk: Send, chunk(..): Send> + Send {}
impl<B> SendBody for B where B: Body<Chunk: Send, chunk(..): Send> + Send {}

/// Extension methods for a thread-safe [body](Body).
pub trait SendBodyExt: IntoBody {
    #[inline]
    fn boxed<'body>(self) -> Boxed<'body, Self::Chunk>
    where
        Self::Body: SendBody,
        Self: 'body,
    {
        Box::pin(self.into_poll_body())
    }
}

impl<B> SendBodyExt for B where B: IntoBody {}
