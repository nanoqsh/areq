use {
    hyper::body::{Body, Frame, SizeHint},
    std::{
        convert::Infallible,
        pin::Pin,
        task::{Context, Poll},
    },
};

#[derive(Debug)]
pub(crate) struct Empty;

impl Body for Empty {
    type Data = &'static [u8];
    type Error = Infallible;

    #[inline]
    fn poll_frame(
        self: Pin<&mut Self>,
        _: &mut Context,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        Poll::Ready(None)
    }

    #[inline]
    fn is_end_stream(&self) -> bool {
        true
    }

    #[inline]
    fn size_hint(&self) -> SizeHint {
        SizeHint::with_exact(0)
    }
}
