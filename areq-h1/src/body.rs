use {
    bytes::Buf,
    futures_lite::{Stream, StreamExt},
    std::pin::Pin,
};

pub trait IntoBody {
    type Body: Body;

    fn into_body(self) -> Self::Body;
}

pub trait Body: Sized {
    const KIND: Kind;

    type Chunk: Buf;

    #[expect(async_fn_in_trait)]
    async fn chunk(&mut self) -> Option<Self::Chunk>;

    // http2 extension
    fn is_end(&self) -> bool {
        matches!(Self::KIND, Kind::Empty)
    }
}

impl<C> IntoBody for C
where
    C: Body,
{
    type Body = Self;

    fn into_body(self) -> Self::Body {
        self
    }
}

pub enum Kind {
    Empty,
    Full,
    Chunked,
}

pub async fn take_full<B>(body: B) -> <B::Body as Body>::Chunk
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
    fn remaining(&self) -> usize {
        unreachable!()
    }

    fn chunk(&self) -> &[u8] {
        unreachable!()
    }

    fn advance(&mut self, _: usize) {
        unreachable!()
    }
}

impl Body for () {
    const KIND: Kind = Kind::Empty;

    type Chunk = Void;

    async fn chunk(&mut self) -> Option<Self::Chunk> {
        None
    }
}

pub struct Full<B>(Option<B>);

impl<B> Full<B> {
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

    async fn chunk(&mut self) -> Option<Self::Chunk> {
        self.0.take()
    }

    fn is_end(&self) -> bool {
        self.0.is_none()
    }
}

impl<'slice> IntoBody for &'slice [u8] {
    type Body = Full<&'slice [u8]>;

    fn into_body(self) -> Self::Body {
        Full::new(self)
    }
}

// TODO: remove boxing
pub struct Chunked<S>(Pin<Box<S>>);

impl<S> Chunked<S> {
    pub fn new(stream: S) -> Self {
        Self(Box::pin(stream))
    }
}

impl<S> Body for Chunked<S>
where
    S: Stream<Item: Buf>,
{
    const KIND: Kind = Kind::Chunked;

    type Chunk = S::Item;

    async fn chunk(&mut self) -> Option<Self::Chunk> {
        self.0.next().await
    }

    fn is_end(&self) -> bool {
        let (_, upper_bound) = self.0.size_hint();
        upper_bound == Some(0)
    }
}
