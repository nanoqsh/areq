use {
    futures_lite::prelude::*,
    std::{
        io::Error,
        pin::Pin,
        task::{Context, Poll},
    },
    tokio::io,
};

pin_project_lite::pin_project! {
    /// Async IO adapter.
    pub struct Io<I> {
        #[pin]
        io: I,
    }
}

impl<I> Io<I> {
    pub fn new(io: I) -> Self {
        Self { io }
    }
}

impl<I> AsyncRead for Io<I>
where
    I: io::AsyncRead,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<usize, Error>> {
        let mut buf = io::ReadBuf::new(buf);
        match self.project().io.poll_read(cx, &mut buf)? {
            Poll::Ready(()) => Poll::Ready(Ok(buf.filled().len())),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<I> AsyncWrite for Io<I>
where
    I: io::AsyncWrite,
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, Error>> {
        self.project().io.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        self.project().io.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        self.project().io.poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use {super::*, futures_lite::future};

    #[test]
    fn read() -> Result<(), Error> {
        let mut buf = [0; 5];
        let mut io = Io::new("hello".as_bytes());
        let n = future::block_on(io.read(&mut buf))?;
        assert_eq!(n, 5);
        assert_eq!(&buf, b"hello");
        Ok(())
    }

    #[test]
    fn write() -> Result<(), Error> {
        let mut buf = vec![];
        let mut io = Io::new(&mut buf);
        let n = future::block_on(io.write("hello".as_bytes()))?;
        assert_eq!(n, 5);
        assert_eq!(buf, b"hello");
        Ok(())
    }
}
