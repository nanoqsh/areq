use {
    bytes::Buf,
    futures_lite::prelude::*,
    std::{
        future::{self, IntoFuture},
        pin::Pin,
        task::{Context, Poll},
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
    async fn chunk(&mut self) -> Option<Self::Chunk>;

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
pub async fn take_full<B>(body: B) -> B::Chunk
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
    async fn chunk(&mut self) -> Option<Self::Chunk> {
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
    async fn chunk(&mut self) -> Option<Self::Chunk> {
        self.0.take()
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

impl<F> Body for Deferred<F>
where
    F: Future<Output: Buf> + Unpin,
{
    type Chunk = F::Output;

    #[inline]
    async fn chunk(&mut self) -> Option<Self::Chunk> {
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

impl<S> Chunked<S> {
    #[inline]
    pub fn catch_error<T, E>(self) -> Chunked<CatchError<S, E>>
    where
        S: Stream<Item = Result<T, E>>,
    {
        Chunked(CatchError {
            stream: self.0,
            err: None,
        })
    }
}

impl<S, E> Chunked<CatchError<S, E>> {
    #[inline]
    pub fn take_error(&mut self) -> Result<(), E> {
        match self.0.err.take() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

impl<S> Body for Chunked<S>
where
    S: Stream<Item: Buf> + Unpin,
{
    type Chunk = S::Item;

    #[inline]
    async fn chunk(&mut self) -> Option<Self::Chunk> {
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

pin_project_lite::pin_project! {
    pub struct CatchError<S, E> {
        #[pin]
        stream: S,
        err: Option<E>,
    }
}

impl<S, E, T> Stream for CatchError<S, E>
where
    S: Stream<Item = Result<T, E>>,
{
    type Item = T;

    #[inline]
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let me = self.project();
        match me.stream.poll_next(cx) {
            Poll::Ready(Some(Ok(t))) => Poll::Ready(Some(t)),
            Poll::Ready(Some(Err(e))) => {
                *me.err = Some(e);
                Poll::Ready(None)
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        futures_lite::{future, stream},
    };

    #[test]
    fn slice() {
        let src = "hi";
        let actual = future::block_on(take_full(src.as_bytes()));
        assert_eq!(actual, src.as_bytes());
    }

    #[test]
    fn full() {
        let src = "hi";
        let full = Full::new(src.as_bytes());
        let actual = future::block_on(take_full(full));
        assert_eq!(actual, src.as_bytes());
    }

    #[test]
    fn deferred() {
        let src = "hi";
        let deferred = Deferred::new(future::ready(src.as_bytes()));
        let actual = future::block_on(take_full(deferred));
        assert_eq!(actual, src.as_bytes());
    }

    #[test]
    fn chunked() {
        let src = ["a", "b", "c"].map(str::as_bytes);

        let mut chunked = Chunked(stream::iter(src));
        for expected in src {
            let actual = future::block_on(chunked.chunk());
            assert_eq!(actual, Some(expected));
        }
    }

    #[test]
    fn chunked_catch_error() {
        let src = [Ok("a"), Ok("b"), Err(2)].map(|r| r.map(str::as_bytes));

        let mut chunked = Chunked(stream::iter(src)).catch_error();
        for expected in src {
            let actual = future::block_on(chunked.chunk());

            assert_eq!(
                actual.ok_or_else(|| chunked
                    .take_error()
                    .expect_err("on none, an error must be here")),
                expected,
            );
        }
    }
}
