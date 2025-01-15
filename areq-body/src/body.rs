use {
    bytes::Buf,
    futures_lite::prelude::*,
    std::{
        future::{self, IntoFuture},
        io::Error,
        ops::DerefMut,
        pin::Pin,
        task::{Context, Poll},
    },
};

pub trait Body {
    type Chunk: Buf;

    #[expect(async_fn_in_trait)]
    async fn chunk(&mut self) -> Option<Result<Self::Chunk, Error>>;
    fn kind(&self) -> Kind;
    fn is_end(&self) -> bool;
}

pub enum Kind {
    Empty,
    Full,
    Chunked,
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

impl<B> Body for &mut B
where
    B: Body,
{
    type Chunk = B::Chunk;

    #[inline]
    async fn chunk(&mut self) -> Option<Result<Self::Chunk, Error>> {
        (**self).chunk().await
    }

    #[inline]
    fn kind(&self) -> Kind {
        (**self).kind()
    }

    #[inline]
    fn is_end(&self) -> bool {
        (**self).is_end()
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

pub trait IntoBody {
    type Chunk: Buf;
    type Body: Body<Chunk = Self::Chunk>;

    fn into_body(self) -> Self::Body;
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

pub trait PollBody {
    type Chunk: Buf;

    fn poll_chunk(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Self::Chunk, Error>>>;

    fn kind(&self) -> Kind;
    fn is_end(&self) -> bool;
}

impl<P> Body for Pin<P>
where
    P: DerefMut<Target: PollBody>,
{
    type Chunk = <P::Target as PollBody>::Chunk;

    #[inline]
    async fn chunk(&mut self) -> Option<Result<Self::Chunk, Error>> {
        future::poll_fn(|cx| self.as_mut().poll_chunk(cx)).await
    }

    #[inline]
    fn kind(&self) -> Kind {
        self.as_ref().kind()
    }

    #[inline]
    fn is_end(&self) -> bool {
        self.as_ref().is_end()
    }
}

pub type BoxedBodySend<'body, C> = Pin<Box<dyn PollBody<Chunk = C> + Send + 'body>>;
pub type BoxedBody<'body, C> = Pin<Box<dyn PollBody<Chunk = C> + 'body>>;

pub trait BodyExt: Body {
    #[inline]
    fn reader(self) -> impl AsyncRead
    where
        Self: Sized,
    {
        Reader {
            body: self.into_poll_body(),
            chunk: Next::Empty,
        }
    }

    #[inline]
    fn into_poll_body(self) -> impl PollBody<Chunk = Self::Chunk>
    where
        Self: Sized,
    {
        unfold(self, |mut body| async {
            let res = body.chunk().await?;
            Some((body, res))
        })
    }

    #[inline]
    fn boxed<'body>(self) -> BoxedBody<'body, Self::Chunk>
    where
        Self: Sized + 'body,
    {
        Box::pin(self.into_poll_body())
    }
}

impl<B> BodyExt for B where B: Body {}

#[cfg(feature = "rtn")]
include!("body_ext_rtn.rs");

#[inline]
fn unfold<B, F, U>(body: B, f: F) -> Unfold<B, F, U>
where
    B: Body,
    F: FnMut(B) -> U,
    U: Future<Output = Option<(B, Result<B::Chunk, Error>)>>,
{
    Unfold {
        body: Some(body),
        f,
        fu: None,
    }
}

pin_project_lite::pin_project! {
    struct Unfold<B, F, U> {
        body: Option<B>,
        f: F,
        #[pin]
        fu: Option<U>,
    }
}

impl<B, F, U> PollBody for Unfold<B, F, U>
where
    B: Body,
    F: FnMut(B) -> U,
    U: Future<Output = Option<(B, Result<B::Chunk, Error>)>>,
{
    type Chunk = B::Chunk;

    #[inline]
    fn poll_chunk(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Self::Chunk, Error>>> {
        let mut me = self.project();
        if let Some(body) = me.body.take() {
            me.fu.set(Some((me.f)(body)));
        }

        let fu = me.fu.as_pin_mut().expect("poll after `None` was returned");
        match fu.poll(cx) {
            Poll::Ready(Some((body, res))) => {
                *me.body = Some(body);
                Poll::Ready(Some(res))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }

    #[inline]
    fn kind(&self) -> Kind {
        self.body
            .as_ref()
            .expect("called before the chunk retrieval was completed")
            .kind()
    }

    #[inline]
    fn is_end(&self) -> bool {
        self.body
            .as_ref()
            .expect("called before the chunk retrieval was completed")
            .is_end()
    }
}

pin_project_lite::pin_project! {
     struct Reader<B, C> {
        #[pin]
        body: B,
        chunk: Next<C>,
    }
}

impl<B> AsyncRead for Reader<B, B::Chunk>
where
    B: PollBody,
{
    #[inline]
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<usize, Error>> {
        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }

        let me = self.project();

        if !me.chunk.has_remaining() {
            match me.body.poll_chunk(cx) {
                Poll::Ready(Some(Ok(c))) => {
                    if c.has_remaining() {
                        *me.chunk = Next::Buf(c);
                    } else {
                        // if next bytes is empty skip this iteration and reschedule
                        cx.waker().wake_by_ref();
                        return Poll::Pending;
                    }
                }
                Poll::Ready(Some(Err(e))) => return Poll::Ready(Err(e)),
                Poll::Ready(None) => return Poll::Ready(Ok(0)),
                Poll::Pending => return Poll::Pending,
            }
        }

        let n = usize::min(me.chunk.remaining(), buf.len());
        me.chunk.copy_to_slice(&mut buf[..n]);
        Poll::Ready(Ok(n))
    }
}

enum Next<B> {
    Buf(B),
    Empty,
}

impl<B> Buf for Next<B>
where
    B: Buf,
{
    #[inline]
    fn remaining(&self) -> usize {
        match self {
            Self::Buf(b) => b.remaining(),
            Self::Empty => 0,
        }
    }

    #[inline]
    fn chunk(&self) -> &[u8] {
        match self {
            Self::Buf(b) => b.chunk(),
            Self::Empty => &[],
        }
    }

    #[inline]
    fn advance(&mut self, cnt: usize) {
        match self {
            Self::Buf(b) => b.advance(cnt),
            Self::Empty => debug_assert_eq!(cnt, 0, "can't advance further"),
        }
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        futures_lite::{future, stream},
        std::{io::ErrorKind, pin},
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

    #[test]
    fn reader() {
        let src = ["h", "e", "ll", "o"].map(str::as_bytes).map(Ok);
        let body = Chunked(stream::iter(src));
        let mut reader = pin::pin!(body.reader());

        let mut out = String::new();
        let res = future::block_on(reader.read_to_string(&mut out));
        assert_eq!(res.ok(), Some(5));
        assert_eq!(out, "hello");
    }

    #[test]
    fn reader_partial() {
        let src = ["h", "e", "ll", "o"].map(str::as_bytes).map(Ok);
        let body = Chunked(stream::iter(src));
        let mut reader = pin::pin!(body.reader());

        for (size, part) in [(1, b"h\0"), (1, b"e\0"), (2, b"ll"), (1, b"o\0")] {
            let mut buf = [0; 2];
            let n = future::block_on(reader.read(&mut buf)).expect("read body part to the buffer");
            assert_eq!(n, size, "failed to read {part:?}");
            assert_eq!(&buf, part);
        }
    }

    #[test]
    fn into_poll_body() {
        let src = "hi";
        let body = Full::new(src.as_bytes());
        let poll_body = pin::pin!(body.into_poll_body());
        let actual = future::block_on(take_full(poll_body));
        assert_eq!(actual.ok(), Some(src.as_bytes()));
    }

    #[test]
    fn boxed() {
        let src = "hi";
        let body = Full::new(src.as_bytes());
        let boxed_body = body.boxed();
        let actual = future::block_on(take_full(boxed_body));
        assert_eq!(actual.ok(), Some(src.as_bytes()));
    }

    #[cfg(feature = "rtn")]
    #[test]
    fn boxed_send() {
        let src = "hi";
        let body = Full::new(src.as_bytes());
        let boxed_body = body.boxed_send();
        let actual = future::block_on(take_full(boxed_body));
        assert_eq!(actual.ok(), Some(src.as_bytes()));
    }
}
