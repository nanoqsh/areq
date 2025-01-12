use {
    bytes::Buf,
    futures_lite::prelude::*,
    std::{
        future::{self, IntoFuture},
        io::Error,
        pin::Pin,
    },
};

pub trait IntoBody {
    type Chunk: Buf;
    type Body: Body<Chunk = Self::Chunk>;

    fn into_body(self) -> Self::Body;
}

pub trait Body {
    type Chunk: Buf;

    #[expect(async_fn_in_trait)]
    async fn chunk(&mut self) -> Option<Result<Self::Chunk, Error>>;

    fn kind(&self) -> Kind;

    fn is_end(&self) -> bool;
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
pub async fn take_full<B>(body: B) -> Result<B::Chunk, Error>
where
    B: IntoBody,
{
    let mut body = body.into_body();
    debug_assert!(matches!(body.kind(), Kind::Full), "body type must be full");
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
    type Chunk = Void;

    #[inline]
    async fn chunk(&mut self) -> Option<Result<Self::Chunk, Error>> {
        None
    }

    #[inline]
    fn kind(&self) -> Kind {
        Kind::Empty
    }

    #[inline]
    fn is_end(&self) -> bool {
        true
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

impl<'str> IntoBody for &'str str {
    type Chunk = <Self::Body as Body>::Chunk;
    type Body = Full<&'str [u8]>;

    #[inline]
    fn into_body(self) -> Self::Body {
        Full::new(self.as_bytes())
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
    type Chunk = B;

    #[inline]
    async fn chunk(&mut self) -> Option<Result<Self::Chunk, Error>> {
        self.0.take().map(Ok)
    }

    #[inline]
    fn kind(&self) -> Kind {
        Kind::Full
    }

    #[inline]
    fn is_end(&self) -> bool {
        self.0.is_none()
    }
}

pub struct Deferred<F>(Option<F>);

impl<F> Deferred<F> {
    #[inline]
    pub fn new<U>(fu: U) -> Self
    where
        U: IntoFuture<IntoFuture = F>,
    {
        Self(Some(fu.into_future()))
    }
}

impl<F, I> Body for Deferred<F>
where
    F: Future<Output = Result<I, Error>> + Unpin,
    I: Buf,
{
    type Chunk = I;

    #[inline]
    async fn chunk(&mut self) -> Option<Result<Self::Chunk, Error>> {
        match &mut self.0 {
            Some(fu) => {
                let mut fu = Pin::new(fu);
                let res = future::poll_fn(|cx| fu.as_mut().poll(cx)).await;
                self.0 = None;
                Some(res)
            }
            None => None,
        }
    }

    #[inline]
    fn kind(&self) -> Kind {
        Kind::Full
    }

    #[inline]
    fn is_end(&self) -> bool {
        self.0.is_none()
    }
}

pub struct Chunked<S>(pub S);

impl<S, I> Body for Chunked<S>
where
    S: Stream<Item = Result<I, Error>> + Unpin,
    I: Buf,
{
    type Chunk = I;

    #[inline]
    async fn chunk(&mut self) -> Option<Result<Self::Chunk, Error>> {
        self.0.next().await
    }

    #[inline]
    fn kind(&self) -> Kind {
        Kind::Chunked
    }

    #[inline]
    fn is_end(&self) -> bool {
        let (_, upper_bound) = self.0.size_hint();
        upper_bound == Some(0)
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        futures_lite::{future, stream},
        std::io::ErrorKind,
    };

    #[test]
    fn slice() {
        let src = "hi";
        let actual = future::block_on(take_full(src.as_bytes()));
        assert_eq!(actual.ok(), Some(src.as_bytes()));
    }

    #[test]
    fn str() {
        let src = "hi";
        let actual = future::block_on(take_full(src));
        assert_eq!(actual.ok(), Some(src.as_bytes()));
    }

    #[test]
    fn full() {
        let src = "hi";
        let full = Full::new(src.as_bytes());
        let actual = future::block_on(take_full(full));
        assert_eq!(actual.ok(), Some(src.as_bytes()));
    }

    #[test]
    fn deferred() {
        let src = "hi";
        let deferred = Deferred::new(future::ready(Ok(src.as_bytes())));
        let actual = future::block_on(take_full(deferred));
        assert_eq!(actual.ok(), Some(src.as_bytes()));
    }

    #[test]
    fn chunked() {
        let src = [Ok("a"), Ok("b"), Err(Error::from(ErrorKind::UnexpectedEof))]
            .map(|r| r.map(str::as_bytes));

        let n = src.len();

        let mut chunked = Chunked(stream::iter(src));
        for _ in 0..n {
            let actual = future::block_on(chunked.chunk());
            match actual {
                Some(Ok(a)) => assert!(matches!(a, b"a" | b"b")),
                Some(Err(e)) => assert_eq!(e.kind(), ErrorKind::UnexpectedEof),
                None => unreachable!(),
            }
        }
    }
}
