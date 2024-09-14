use {
    futures_io::{AsyncRead, AsyncWrite},
    hyper::rt,
    std::{
        io::{Error, IoSlice},
        mem::MaybeUninit,
        pin::Pin,
        slice,
        task::{Context, Poll},
    },
};

/// Async IO trait alias.
pub trait AsyncIo: AsyncRead + AsyncWrite + Unpin {}
impl<I> AsyncIo for I where I: AsyncRead + AsyncWrite + Unpin {}

pub(crate) struct Io<I>(pub(crate) I);

impl<I> rt::Read for Io<I>
where
    I: AsyncRead + Unpin,
{
    #[inline]
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
        mut buf: rt::ReadBufCursor,
    ) -> Poll<Result<(), Error>> {
        const MAX_BUF_SIZE: usize = 1 << 11;

        fn fill(uninit: &mut [MaybeUninit<u8>], byte: u8) -> &mut [u8] {
            let ptr = uninit.as_mut_ptr();
            let len = uninit.len();

            unsafe { ptr.write_bytes(byte, len) };

            unsafe { slice::from_raw_parts_mut(ptr.cast(), len) }
        }

        let bytes = {
            // SAFETY:
            // get an unfilled part of the buffer to advance it later
            let uninit = unsafe { buf.as_mut() };

            let len = usize::min(uninit.len(), MAX_BUF_SIZE);
            fill(&mut uninit[..len], 0)
        };

        let io = Pin::new(&mut self.0);
        let n = match io.poll_read(cx, bytes)? {
            Poll::Ready(n) => n,
            Poll::Pending => return Poll::Pending,
        };

        // SAFETY: n bytes were initialized
        unsafe { buf.advance(n) };

        Poll::Ready(Ok(()))
    }
}

impl<I> rt::Write for Io<I>
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

    #[inline]
    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
        bufs: &[IoSlice],
    ) -> Poll<Result<usize, Error>> {
        Pin::new(&mut self.0).poll_write_vectored(cx, bufs)
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
            hyper::rt::{Read, ReadBuf},
            std::pin,
        };

        let mut raw = [const { MaybeUninit::uninit() }; 5];
        let mut buf = ReadBuf::uninit(&mut raw);

        let io = pin::pin!(Io(&b"hello"[..]));
        let Poll::Ready(Ok(())) = poll(|cx| io.poll_read(cx, buf.unfilled())) else {
            unreachable!()
        };

        assert_eq!(buf.filled(), b"hello");
    }

    #[test]
    fn write() {
        use {hyper::rt::Write, std::pin};

        let mut buf = vec![];
        let io = pin::pin!(Io(&mut buf));
        let Poll::Ready(Ok(n)) = poll(|cx| io.poll_write(cx, b"hello")) else {
            unreachable!()
        };

        assert_eq!(n, 5);
        assert_eq!(buf, b"hello");
    }
}
