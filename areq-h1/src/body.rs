use {
    bytes::Buf,
    futures_lite::Stream,
    std::{
        marker::PhantomData,
        pin::Pin,
        task::{Context, Poll},
    },
};

pub trait Body {
    type Buf: Buf;
    type Stream: Stream<Item = Self::Buf>;
    fn chunk(self) -> Chunk<Self::Buf, Self::Stream>;
}

pub enum Chunk<B, S> {
    Full(B),
    Stream(S),
}

impl Body for () {
    type Buf = &'static [u8];
    type Stream = Empty<Self::Buf>;

    fn chunk(self) -> Chunk<Self::Buf, Self::Stream> {
        Chunk::Full(&[])
    }
}

impl<'slice> Body for &'slice [u8] {
    type Buf = &'slice [u8];
    type Stream = Empty<Self::Buf>;

    fn chunk(self) -> Chunk<Self::Buf, Self::Stream> {
        Chunk::Full(self)
    }
}

pub struct Chunked<S>(pub S);

impl<S> Body for Chunked<S>
where
    S: Stream<Item: Buf>,
{
    type Buf = S::Item;
    type Stream = S;

    fn chunk(self) -> Chunk<Self::Buf, Self::Stream> {
        Chunk::Stream(self.0)
    }
}

pub struct Full<B>(pub B);

impl<B> Body for Full<B>
where
    B: Buf,
{
    type Buf = B;
    type Stream = Empty<Self::Buf>;

    fn chunk(self) -> Chunk<Self::Buf, Self::Stream> {
        Chunk::Full(self.0)
    }
}

pub struct Empty<T>(PhantomData<T>);

impl<T> Stream for Empty<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, _: &mut Context) -> Poll<Option<Self::Item>> {
        Poll::Ready(None)
    }
}
