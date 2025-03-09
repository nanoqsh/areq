use {
    bytes::{Buf, Bytes, BytesMut},
    futures_lite::prelude::*,
    std::{
        future,
        io::Error,
        mem,
        ops::DerefMut,
        pin::Pin,
        task::{Context, Poll},
    },
};

#[cfg(feature = "rtn")]
pub use crate::body_rtn::{BodyExtRtn, SendBody};

/// Type representing the body of an HTTP request/response.
pub trait Body {
    /// Type of the body data chunk.
    type Chunk: Buf;

    /// Asynchronously retrieves a body data chunk.
    ///
    /// Returns `Some(Ok(_))` when data is successfully received.
    /// If an I/O error occurs, it is returned as `Some(Err(_))`.
    /// When the entire body has been received, a returned `None`
    /// indicates the end of the data stream.
    async fn chunk(&mut self) -> Option<Result<Self::Chunk, Error>>;

    /// Returns a size [hint](Hint) for the body.
    fn size_hint(&self) -> Hint;
}

/// Body size hint.
///
/// Used to indicate a kind and size of the request/response body.
#[derive(Clone, Copy, Debug)]
pub enum Hint {
    /// The body is empty.
    Empty,

    /// The entire body is returned in a single
    /// [`chunk`](Body::chunk) call.
    ///
    /// For this variant, an HTTP/1.1 client sets the
    /// `Content-Length` header for the request.
    Full {
        /// Specifies the body size in bytes, if possible.
        /// If the size cannot be determined in advance, this
        /// field must be `None`.
        len: Option<u64>,
    },

    /// The body is chunked and received through sequential
    /// [`chunk`](Body::chunk) calls.
    ///
    /// For this variant, an HTTP/1.1 client sets the
    /// `Transfer-Encoding: chunked` header for the request.
    Chunked {
        /// Indicates the end of the body stream.
        end: bool,
    },
}

impl Hint {
    /// Returns `true` if the hint is [`Empty`](Hint::Empty).
    #[inline]
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    /// Returns `true` if the hint is [`Full`](Hint::Full).
    #[inline]
    pub fn is_full(&self) -> bool {
        matches!(self, Self::Full { .. })
    }

    /// Returns `true` if the hint is [`Chunked`](Hint::Chunked).
    #[inline]
    pub fn is_chunked(&self) -> bool {
        matches!(self, Self::Chunked { .. })
    }

    /// Checks if the body data stream has ended.
    #[inline]
    pub fn end(self) -> bool {
        match self {
            Self::Empty => true,
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

    #[inline]
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
        Hint::Empty
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

/// Returns the full body from a [buffer](Buf).
pub struct Full<B>(Option<B>);

impl<B> Full<B> {
    /// Creates a body from the given [buffer](Buf).
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

pub trait IntoBody: Sized {
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

pub type BoxedLocal<'body, C> = Pin<Box<dyn PollBody<Chunk = C> + 'body>>;
pub type Boxed<'body, C> = Pin<Box<dyn PollBody<Chunk = C> + Send + 'body>>;

pub trait BodyExt: IntoBody {
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
    async fn text(self) -> Result<String, Error>
    where
        Self::Chunk: Into<Bytes>,
    {
        let bytes = self.bytes().await?;
        let v = Vec::from(bytes);
        let s = match String::from_utf8(v) {
            Ok(s) => s,
            Err(e) => String::from_utf8_lossy(e.as_bytes()).into_owned(),
        };

        Ok(s)
    }

    #[inline]
    async fn bytes(self) -> Result<Bytes, Error>
    where
        Self::Chunk: Into<Bytes>,
    {
        let bytes_mut = self.bytes_mut().await?;
        Ok(bytes_mut.freeze())
    }

    #[inline]
    async fn bytes_mut(self) -> Result<BytesMut, Error>
    where
        Self::Chunk: Into<Bytes>,
    {
        let mut out = BytesMut::new();
        let mut body = self.into_body();
        while let Some(res) = body.chunk().await {
            match res?.into().try_into_mut() {
                Ok(bytes_mut) => out.unsplit(bytes_mut),
                Err(bytes) => out.extend_from_slice(&bytes),
            }
        }

        Ok(out)
    }

    #[inline]
    fn read(self) -> impl AsyncRead {
        Reader {
            body: self.into_poll_body(),
            state: State::Start,
        }
    }

    #[inline]
    fn stream(self) -> impl Stream<Item = Result<Self::Chunk, Error>> {
        Streamer {
            body: self.into_poll_body(),
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

pin_project_lite::pin_project! {
    struct Streamer<B> {
       #[pin]
       body: B,
   }
}

impl<B> Stream for Streamer<B>
where
    B: PollBody,
{
    type Item = Result<B::Chunk, Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.project().body.poll_chunk(cx)
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
    fn text() {
        let src = ["he", "ll", "o"].map(str::as_bytes).map(Ok);
        let body = Chunked(stream::iter(src));
        let text = future::block_on(body.text()).expect("read body text");
        assert_eq!(text, "hello");
    }

    #[test]
    fn bytes() {
        let src = ["he", "ll", "o"].map(str::as_bytes).map(Ok);
        let body = Chunked(stream::iter(src));
        let bytes = future::block_on(body.bytes()).expect("read body bytes");
        assert_eq!(bytes, "hello");
    }

    #[test]
    fn bytes_mut() {
        let src = ["he", "ll", "o"].map(str::as_bytes).map(Ok);
        let body = Chunked(stream::iter(src));
        let bytes_mut = future::block_on(body.bytes_mut()).expect("read body bytes");
        assert_eq!(bytes_mut, "hello");
    }

    #[test]
    fn read() {
        let src = ["h", "e", "ll", "o"].map(str::as_bytes).map(Ok);
        let body = Chunked(stream::iter(src));
        let mut reader = pin::pin!(body.read());

        let mut out = String::new();
        let res = future::block_on(reader.read_to_string(&mut out));
        assert_eq!(res.ok(), Some(5));
        assert_eq!(out, "hello");
    }

    #[test]
    fn reader_partial() {
        let src = ["h", "e", "ll", "o"].map(str::as_bytes).map(Ok);
        let body = Chunked(stream::iter(src));
        let mut reader = pin::pin!(body.read());

        for (size, part) in [
            (1, b"h\0"),
            (1, b"e\0"),
            (2, b"ll"),
            (1, b"o\0"),
            (0, b"\0\0"),
        ] {
            let mut buf = [0; 2];
            let n = future::block_on(reader.read(&mut buf)).expect("read body part to the buffer");
            assert_eq!(n, size);
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
