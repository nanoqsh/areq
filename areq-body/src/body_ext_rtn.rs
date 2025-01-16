pub trait BodyExtRtn: Body + Sized {
    #[inline]
    fn boxed<'body>(self) -> Boxed<'body, Self::Chunk>
    where
        Self: Body<chunk(..): Send> + Send + 'body,
    {
        Box::pin(self.into_poll_body())
    }
}

impl<B> BodyExtRtn for B where B: Body {}
