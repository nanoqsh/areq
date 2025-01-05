use {
    futures_lite::{AsyncRead, AsyncWrite},
    std::{
        io::Error,
        pin::Pin,
        task::{Context, Poll},
    },
    tokio::io,
};

pin_project_lite::pin_project! {
    /// Async IO adapter.
    pub(crate) struct Io<I> {
        #[pin]
        io: I,
    }
}

impl<I> Io<I> {
    pub fn new(io: I) -> Self {
        Self { io }
    }
}

impl<I> io::AsyncRead for Io<I>
where
    I: AsyncRead,
{
    #[inline]
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut io::ReadBuf<'_>,
    ) -> Poll<Result<(), Error>> {
        let bytes = buf.initialize_unfilled();
        match self.project().io.poll_read(cx, bytes)? {
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
    I: AsyncWrite,
{
    #[inline]
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, Error>> {
        self.project().io.poll_write(cx, buf)
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        self.project().io.poll_flush(cx)
    }

    #[inline]
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        self.project().io.poll_close(cx)
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
        let mut io = pin::pin!(Io::new(&b"hello"[..]));
        let n = future::block_on(io.read(&mut buf))?;
        assert_eq!(n, 5);
        assert_eq!(&buf, b"hello");
        Ok(())
    }

    #[test]
    fn write() -> Result<(), Error> {
        let mut buf = vec![];
        let mut io = pin::pin!(Io::new(&mut buf));
        let n = future::block_on(io.write(b"hello"))?;
        assert_eq!(n, 5);
        assert_eq!(buf, b"hello");
        Ok(())
    }
}
