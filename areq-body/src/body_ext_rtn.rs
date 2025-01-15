pub trait BodyExtRtn: Body {
    #[inline]
    fn boxed<'body>(self) -> BoxedBodySend<'body, Self::Chunk>
    where
        Self: Body<chunk(..): Send> + Send + Sized + 'body,
    {
        Box::pin(self.into_poll_body())
    }
}

impl<B> BodyExtRtn for B where B: Body {}
