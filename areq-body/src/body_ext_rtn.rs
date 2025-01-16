pub trait BodyExtRtn: IntoBody + Sized {
    #[inline]
    fn boxed<'body>(self) -> Boxed<'body, Self::Chunk>
    where
        Self::Body: Body<chunk(..): Send> + Send,
        Self: 'body,
    {
        Box::pin(self.into_poll_body())
    }
}

impl<B> BodyExtRtn for B where B: IntoBody {}
