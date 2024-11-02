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
    type Data: Buf;
    type Stream: Stream<Item = Self::Data>;
    fn chunk(self) -> Chunk<Self::Data, Self::Stream>;
}

pub enum Chunk<B, S> {
    Full(B),
    Stream(S),
}

impl Body for () {
    type Data = &'static [u8];
    type Stream = Empty<Self::Data>;

    fn chunk(self) -> Chunk<Self::Data, Self::Stream> {
        Chunk::Full(&[])
    }
}

impl<'slice> Body for &'slice [u8] {
    type Data = &'slice [u8];
    type Stream = Empty<Self::Data>;

    fn chunk(self) -> Chunk<Self::Data, Self::Stream> {
        Chunk::Full(self)
    }
}

pub struct Chunked<S>(pub S);

impl<S> Body for Chunked<S>
where
    S: Stream<Item: Buf>,
{
    type Data = S::Item;
    type Stream = S;

    fn chunk(self) -> Chunk<Self::Data, Self::Stream> {
        Chunk::Stream(self.0)
    }
}

pub struct Full<B>(pub B);

impl<B> Body for Full<B>
where
    B: Buf,
{
    type Data = B;
    type Stream = Empty<Self::Data>;

    fn chunk(self) -> Chunk<Self::Data, Self::Stream> {
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
