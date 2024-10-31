#[cfg(test)]
pub(crate) mod parts {
    use {
        futures_lite::{AsyncRead, AsyncReadExt},
        std::{
            io::Error,
            pin::Pin,
            task::{Context, Poll},
        },
    };

    pub fn make<I>(reads: I) -> impl AsyncRead
    where
        I: IntoIterator<IntoIter: Iterator<Item: AsyncRead + Unpin> + Unpin>,
    {
        let mut reads = reads.into_iter();
        let next_read = reads.next();
        Parts { reads, next_read }
    }

    struct Parts<R>
    where
        R: Iterator,
    {
        reads: R,
        next_read: Option<R::Item>,
    }

    impl<R> AsyncRead for Parts<R>
    where
        R: Iterator<Item: AsyncRead + Unpin> + Unpin,
    {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context,
            buf: &mut [u8],
        ) -> Poll<Result<usize, Error>> {
            let me = &mut *self;
            loop {
                match &mut me.next_read {
                    Some(read) => match Pin::new(read).poll_read(cx, buf)? {
                        Poll::Ready(0) => {
                            me.next_read = me.reads.next();
                            continue;
                        }
                        Poll::Ready(n) => break Poll::Ready(Ok(n)),
                        Poll::Pending => break Poll::Pending,
                    },
                    None => break Poll::Ready(Ok(0)),
                }
            }
        }
    }

    #[test]
    fn parts() -> Result<(), Error> {
        let mut parts = {
            let reads = ["foo", "", "bar"].map(str::as_bytes);
            make(reads)
        };

        let mut buf = [0; 6];
        let (foo, bar) = buf.split_at_mut(3);
        for buf in [foo, bar] {
            let n = async_io::block_on(parts.read(buf))?;
            assert_eq!(n, buf.len());
        }

        assert_eq!(&buf, b"foobar");
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod io {
    use {
        futures_lite::{AsyncRead, AsyncWrite},
        std::{
            io::Error,
            pin::Pin,
            task::{Context, Poll},
        },
    };

    pub fn make<R, W>(read: R, write: W) -> impl AsyncRead + AsyncWrite
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        Io { read, write }
    }

    struct Io<R, W> {
        read: R,
        write: W,
    }

    impl<R, W> AsyncRead for Io<R, W>
    where
        R: AsyncRead + Unpin,
        W: Unpin,
    {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context,
            buf: &mut [u8],
        ) -> Poll<Result<usize, Error>> {
            Pin::new(&mut self.read).poll_read(cx, buf)
        }
    }

    impl<R, W> AsyncWrite for Io<R, W>
    where
        R: Unpin,
        W: AsyncWrite + Unpin,
    {
        fn poll_write(
            mut self: Pin<&mut Self>,
            cx: &mut Context,
            buf: &[u8],
        ) -> Poll<Result<usize, Error>> {
            Pin::new(&mut self.write).poll_write(cx, buf)
        }

        fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Error>> {
            Pin::new(&mut self.write).poll_flush(cx)
        }

        fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Error>> {
            Pin::new(&mut self.write).poll_close(cx)
        }
    }
}
