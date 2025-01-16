use {
    bytes::Buf,
    futures_lite::prelude::*,
    std::{
        future::{self, IntoFuture},
        io::Error,
        mem,
        ops::DerefMut,
        pin::Pin,
        task::{Context, Poll},
    },
};

pub trait Body {
    type Chunk: Buf;

    #[expect(async_fn_in_trait)]
    async fn chunk(&mut self) -> Option<Result<Self::Chunk, Error>>;
    fn size_hint(&self) -> Hint;
}

#[derive(Clone, Copy)]
pub enum Hint {
    Full { len: Option<u64> },
    Chunked { end: bool },
}

impl Hint {
    /// Returns `true` if the hint is [`Full`]
    #[inline]
    pub fn is_full(&self) -> bool {
        matches!(self, Self::Full { .. })
    }

    /// Returns `true` if the hint is [`Chunked`]
    #[inline]
    pub fn is_chunked(&self) -> bool {
        matches!(self, Self::Chunked { .. })
    }

    #[inline]
    pub fn end(self) -> bool {
        match self {
            Self::Full { len } => len == Some(0),
            Self::Chunked { end } => end,
        }
    }
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

    fn size_hint(&self) -> Hint {
        (**self).size_hint()
    }
}

impl Body for () {
    type Chunk = &'static [u8];

    #[inline]
    async fn chunk(&mut self) -> Option<Result<Self::Chunk, Error>> {
        None
    }

    #[inline]
    fn size_hint(&self) -> Hint {
        Hint::Full { len: Some(0) }
    }
}

impl<'slice> Body for &'slice [u8] {
    type Chunk = &'slice [u8];

    #[inline]
    async fn chunk(&mut self) -> Option<Result<Self::Chunk, Error>> {
        if self.is_empty() {
            None
        } else {
            Some(Ok(mem::take(self)))
        }
    }

    #[inline]
    fn size_hint(&self) -> Hint {
        Hint::Full {
            len: Some(self.len() as u64),
        }
    }
}

impl<'str> Body for &'str str {
    type Chunk = &'str [u8];

    #[inline]
    async fn chunk(&mut self) -> Option<Result<Self::Chunk, Error>> {
        if self.is_empty() {
            None
        } else {
            let s = mem::take(self);
            Some(Ok(s.as_bytes()))
        }
    }

    #[inline]
    fn size_hint(&self) -> Hint {
        Hint::Full {
            len: Some(self.len() as u64),
        }
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
    fn size_hint(&self) -> Hint {
        Hint::Full {
            len: Some(
                self.0
                    .as_ref()
                    .map(|b| b.remaining() as u64)
                    .unwrap_or_default(),
            ),
        }
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
    fn size_hint(&self) -> Hint {
        Hint::Full {
            len: if self.0.is_none() { Some(0) } else { None },
        }
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
    fn size_hint(&self) -> Hint {
        let (_, upper_bound) = self.0.size_hint();
        Hint::Chunked {
            end: upper_bound == Some(0),
        }
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

pub trait PollBody {
    type Chunk: Buf;

    fn poll_chunk(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Self::Chunk, Error>>>;

    fn size_hint(&self) -> Hint;
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
    fn size_hint(&self) -> Hint {
        self.as_ref().size_hint()
    }
}

pub type Boxed<'body, C> = Pin<Box<dyn PollBody<Chunk = C> + Send + 'body>>;
pub type BoxedLocal<'body, C> = Pin<Box<dyn PollBody<Chunk = C> + 'body>>;

pub trait BodyExt: IntoBody + Sized {
    #[expect(async_fn_in_trait)]
    #[inline]
    async fn take_full(self) -> Result<Option<Self::Chunk>, Error> {
        let mut body = self.into_body();
        let size = body.size_hint();

        assert!(
            matches!(size, Hint::Full { .. }),
            "the body size must be full",
        );

        let chunk = body.chunk().await;

        match &chunk {
            Some(Ok(chunk)) => {
                debug_assert!(
                    !size.end() || !chunk.has_remaining(),
                    "an empty body shouldn't have remaining chunks",
                );
            }
            Some(Err(_)) => {}
            None => debug_assert!(size.end(), "the body must be empty"),
        }

        debug_assert!(body.size_hint().end(), "the body must ends after the chunk",);

        chunk.transpose()
    }

    #[inline]
    fn reader(self) -> impl AsyncRead {
        Reader {
            body: self.into_poll_body(),
            state: State::Start,
        }
    }

    #[inline]
    fn into_poll_body(self) -> impl PollBody<Chunk = Self::Chunk> {
        unfold(self.into_body(), |mut body| async {
            match body.chunk().await {
                Some(res) => Step::Next { body, res },
                None => Step::End(body),
            }
        })
    }

    #[inline]
    fn boxed_local<'body>(self) -> BoxedLocal<'body, Self::Chunk>
    where
        Self: 'body,
    {
        Box::pin(self.into_poll_body())
    }
}

impl<B> BodyExt for B where B: IntoBody {}

#[cfg(feature = "rtn")]
include!("body_ext_rtn.rs");

enum Step<B, C> {
    Next { body: B, res: Result<C, Error> },
    End(B),
}

#[inline]
fn unfold<B, F, U>(body: B, f: F) -> Unfold<B, F, U>
where
    B: Body,
    F: FnMut(B) -> U,
    U: Future<Output = Step<B, B::Chunk>>,
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
    U: Future<Output = Step<B, B::Chunk>>,
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

        let fu = me.fu.as_pin_mut().expect("future should always be here");

        match fu.poll(cx) {
            Poll::Ready(Step::Next { body, res }) => {
                *me.body = Some(body);
                Poll::Ready(Some(res))
            }
            Poll::Ready(Step::End(body)) => {
                *me.body = Some(body);
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }

    #[inline]
    fn size_hint(&self) -> Hint {
        self.body
            .as_ref()
            .expect("called before the chunk retrieval was completed")
            .size_hint()
    }
}

pin_project_lite::pin_project! {
     struct Reader<B, C> {
        #[pin]
        body: B,
        state: State<C>,
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
        let me = self.project();

        if let State::End = me.state {
            return Poll::Ready(Ok(0));
        }

        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }

        if !me.state.has_remaining() {
            match me.body.poll_chunk(cx) {
                Poll::Ready(Some(Ok(c))) => {
                    if c.has_remaining() {
                        *me.state = State::Next(c);
                    } else {
                        // if next bytes is empty skip this iteration and reschedule
                        cx.waker().wake_by_ref();
                        return Poll::Pending;
                    }
                }
                Poll::Ready(Some(Err(e))) => return Poll::Ready(Err(e)),
                Poll::Ready(None) => {
                    *me.state = State::End;
                    return Poll::Ready(Ok(0));
                }
                Poll::Pending => return Poll::Pending,
            }
        }

        let n = usize::min(me.state.remaining(), buf.len());
        debug_assert_ne!(n, 0, "at least one byte must be read");

        me.state.copy_to_slice(&mut buf[..n]);
        Poll::Ready(Ok(n))
    }
}

enum State<B> {
    Start,
    Next(B),
    End,
}

impl<B> Buf for State<B>
where
    B: Buf,
{
    #[inline]
    fn remaining(&self) -> usize {
        match self {
            Self::Next(b) => b.remaining(),
            Self::Start | Self::End => 0,
        }
    }

    #[inline]
    fn chunk(&self) -> &[u8] {
        match self {
            Self::Next(b) => b.chunk(),
            Self::Start | Self::End => &[],
        }
    }

    #[inline]
    fn advance(&mut self, cnt: usize) {
        match self {
            Self::Next(b) => b.advance(cnt),
            Self::Start | Self::End => debug_assert_eq!(cnt, 0, "can't advance further"),
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
        let actual = future::block_on(src.as_bytes().take_full()).expect("take full body");

        assert_eq!(
            actual.as_ref().map(Buf::chunk).unwrap_or_default(),
            src.as_bytes(),
        );
    }

    #[test]
    fn str() {
        let src = "hi";
        let actual = future::block_on(src.take_full()).expect("take full body");

        assert_eq!(
            actual.as_ref().map(Buf::chunk).unwrap_or_default(),
            src.as_bytes(),
        );
    }

    #[test]
    fn full() {
        let src = "hi";
        let full = Full::new(src.as_bytes());
        let actual = future::block_on(full.take_full()).expect("take full body");

        assert_eq!(
            actual.as_ref().map(Buf::chunk).unwrap_or_default(),
            src.as_bytes(),
        );
    }

    #[test]
    fn deferred() {
        let src = "hi";
        let deferred = Deferred::new(future::ready(Ok(src.as_bytes())));
        let actual = future::block_on(deferred.take_full()).expect("take full body");

        assert_eq!(
            actual.as_ref().map(Buf::chunk).unwrap_or_default(),
            src.as_bytes(),
        );
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

        for (size, part) in [
            (1, b"h\0"),
            (1, b"e\0"),
            (2, b"ll"),
            (1, b"o\0"),
            (0, b"\0\0"),
        ] {
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
        let actual = future::block_on(poll_body.take_full()).expect("take full body");

        assert_eq!(
            actual.as_ref().map(Buf::chunk).unwrap_or_default(),
            src.as_bytes(),
        );
    }

    #[test]
    fn boxed_local() {
        let src = "hi";
        let body = Full::new(src.as_bytes());
        let boxed_body = body.boxed_local();
        let actual = future::block_on(boxed_body.take_full()).expect("take full body");

        assert_eq!(
            actual.as_ref().map(Buf::chunk).unwrap_or_default(),
            src.as_bytes(),
        );
    }

    #[cfg(feature = "rtn")]
    #[test]
    fn boxed() {
        let src = "hi";
        let body = Full::new(src.as_bytes());
        let boxed_body = body.boxed();
        let actual = future::block_on(boxed_body.take_full()).expect("take full body");

        assert_eq!(
            actual.as_ref().map(Buf::chunk).unwrap_or_default(),
            src.as_bytes(),
        );
    }
}
