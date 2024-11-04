use {
    bytes::Buf,
    futures_lite::{Stream, StreamExt},
};

pub trait IntoBody {
    type Chunk: Buf;
    type Body: Body<Chunk = Self::Chunk>;

    fn into_body(self) -> Self::Body;
}

pub trait Body: Sized {
    const KIND: Kind;

    type Chunk: Buf;

    #[expect(async_fn_in_trait)]
    async fn chunk(&mut self) -> Option<Self::Chunk>;

    #[inline]
    fn is_end(&self) -> bool {
        matches!(Self::KIND, Kind::Empty)
    }
}

impl<B> IntoBody for B
where
    B: Body,
{
    type Chunk = B::Chunk;
    type Body = Self;

    #[inline]
    fn into_body(self) -> Self::Body {
        self
    }
}

pub enum Kind {
    Empty,
    Full,
    Chunked,
}

#[inline]
pub async fn take_full<B>(body: B) -> B::Chunk
where
    B: IntoBody,
{
    assert!(
        matches!(B::Body::KIND, Kind::Full),
        "body type must be full",
    );

    let mut body = body.into_body();
    debug_assert!(!body.is_end(), "the body must have only one chunk");

    let chunk = body.chunk().await.expect("full body should have content");
    debug_assert!(body.is_end(), "the body must have only one chunk");

    chunk
}

pub enum Void {}

impl Buf for Void {
    #[inline]
    fn remaining(&self) -> usize {
        match *self {}
    }

    #[inline]
    fn chunk(&self) -> &[u8] {
        match *self {}
    }

    #[inline]
    fn advance(&mut self, _: usize) {
        match *self {}
    }
}

impl Body for () {
    const KIND: Kind = Kind::Empty;

    type Chunk = Void;

    #[inline]
    async fn chunk(&mut self) -> Option<Self::Chunk> {
        None
    }
}

pub struct Full<B>(Option<B>);

impl<B> Full<B> {
    #[inline]
    pub fn new(body: B) -> Self {
        Self(Some(body))
    }
}

impl<B> Body for Full<B>
where
    B: Buf,
{
    const KIND: Kind = Kind::Full;

    type Chunk = B;

    #[inline]
    async fn chunk(&mut self) -> Option<Self::Chunk> {
        self.0.take()
    }

    #[inline]
    fn is_end(&self) -> bool {
        self.0.is_none()
    }
}

impl<'slice> IntoBody for &'slice [u8] {
    type Chunk = <Self::Body as Body>::Chunk;
    type Body = Full<&'slice [u8]>;

    #[inline]
    fn into_body(self) -> Self::Body {
        Full::new(self)
    }
}

pub struct Chunked<S>(pub S);

impl<S> Body for Chunked<S>
where
    S: Stream<Item: Buf> + Unpin,
{
    const KIND: Kind = Kind::Chunked;

    type Chunk = S::Item;

    #[inline]
    async fn chunk(&mut self) -> Option<Self::Chunk> {
        self.0.next().await
    }

    #[inline]
    fn is_end(&self) -> bool {
        let (_, upper_bound) = self.0.size_hint();
        upper_bound == Some(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slice() {
        let src = "hi";
        let actual = async_io::block_on(take_full(src.as_bytes()));
        assert_eq!(actual, src.as_bytes());
    }

    #[test]
    fn full() {
        let src = "hi";
        let full = Full::new(src.as_bytes());
        let actual = async_io::block_on(take_full(full));
        assert_eq!(actual, src.as_bytes());
    }
}
