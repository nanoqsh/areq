use {
    futures_io::{AsyncRead, AsyncWrite},
    std::{
        io::Error,
        pin::Pin,
        task::{Context, Poll},
    },
    tokio::io,
};

/// Async IO adapter.
pub(crate) struct Io<I>(pub I);

impl<I> io::AsyncRead for Io<I>
where
    I: AsyncRead + Unpin,
{
    #[inline]
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut io::ReadBuf,
    ) -> Poll<Result<(), Error>> {
        let bytes = buf.initialize_unfilled();
        match Pin::new(&mut self.0).poll_read(cx, bytes)? {
            Poll::Ready(n) => {
                buf.advance(n);
                Poll::Ready(Ok(()))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<I> io::AsyncWrite for Io<I>
where
    I: AsyncWrite + Unpin,
{
    #[inline]
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &[u8],
    ) -> Poll<Result<usize, Error>> {
        Pin::new(&mut self.0).poll_write(cx, buf)
    }

    #[inline]
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Error>> {
        Pin::new(&mut self.0).poll_flush(cx)
    }

    #[inline]
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Error>> {
        Pin::new(&mut self.0).poll_close(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn poll<F, R>(f: F) -> R
    where
        F: FnOnce(&mut Context) -> R,
    {
        use std::{
            sync::Arc,
            task::{Wake, Waker},
        };

        struct TestWaker;

        impl Wake for TestWaker {
            fn wake(self: Arc<Self>) {}
        }

        let waker = Waker::from(Arc::new(TestWaker));
        f(&mut Context::from_waker(&waker))
    }

    #[test]
    fn read() {
        use {
            std::{mem::MaybeUninit, pin},
            tokio::io::{AsyncRead, ReadBuf},
        };

        let mut raw = [const { MaybeUninit::uninit() }; 5];
        let mut buf = ReadBuf::uninit(&mut raw);

        let io = pin::pin!(Io(&b"hello"[..]));
        let Poll::Ready(Ok(())) = poll(|cx| io.poll_read(cx, &mut buf)) else {
            unreachable!()
        };

        assert_eq!(buf.filled(), b"hello");
    }

    #[test]
    fn write() {
        use {std::pin, tokio::io::AsyncWrite};

        let mut buf = vec![];
        let io = pin::pin!(Io(&mut buf));
        let Poll::Ready(Ok(n)) = poll(|cx| io.poll_write(cx, b"hello")) else {
            unreachable!()
        };

        assert_eq!(n, 5);
        assert_eq!(buf, b"hello");
    }
}
