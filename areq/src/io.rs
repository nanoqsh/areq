use {
    futures_lite::{AsyncRead, AsyncWrite},
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
    use {
        super::*,
        futures_lite::future,
        std::pin,
        tokio::io::{AsyncReadExt, AsyncWriteExt},
    };

    #[test]
    fn read() -> Result<(), Error> {
        let mut buf = [0; 5];
        let mut io = pin::pin!(Io(&b"hello"[..]));
        let n = future::block_on(io.read(&mut buf))?;
        assert_eq!(n, 5);
        assert_eq!(&buf, b"hello");
        Ok(())
    }

    #[test]
    fn write() -> Result<(), Error> {
        let mut buf = vec![];
        let mut io = pin::pin!(Io(&mut buf));
        let n = future::block_on(io.write(b"hello"))?;
        assert_eq!(n, 5);
        assert_eq!(buf, b"hello");
        Ok(())
    }
}
