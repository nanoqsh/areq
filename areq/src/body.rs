use {
    futures_io::AsyncBufRead,
    hyper::body::{Body, Frame, SizeHint},
    std::{
        convert::Infallible,
        io::{Cursor, Error},
        pin::Pin,
        task::{Context, Poll},
    },
};

pub(crate) struct Reader<R>
where
    R: ?Sized,
{
    end: bool,
    used: usize,
    read: R,
}

impl<R> Reader<R>
where
    R: AsyncBufRead + Unpin + ?Sized,
{
    fn poll_read_buf<'buf>(
        self: Pin<&'buf mut Self>,
        cx: &mut Context,
    ) -> Poll<Option<Result<&'buf [u8], Error>>> {
        let me = self.get_mut();

        let mut read = Pin::new(&mut me.read);
        read.as_mut().consume(me.used);
        me.used = 0;

        match read.poll_fill_buf(cx)? {
            Poll::Ready([]) => {
                me.end = true;
                Poll::Ready(None)
            }
            Poll::Ready(buf) => {
                me.used = buf.len();
                Poll::Ready(Some(Ok(buf)))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<R> Body for Reader<R>
where
    R: AsyncBufRead + Unpin + ?Sized,
{
    type Data = Cursor<Box<[u8]>>; // TODO: remove too many allocations
    type Error = Error;

    #[inline]
    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        match self.poll_read_buf(cx)? {
            Poll::Ready(Some(buf)) => {
                let cur = Cursor::new(Box::from(buf));
                Poll::Ready(Some(Ok(Frame::data(cur))))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }

    #[inline]
    fn is_end_stream(&self) -> bool {
        self.end
    }
}

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
