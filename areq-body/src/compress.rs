use {
    crate::body::{Body, Hint},
    async_compression::futures::{bufread, write},
    bytes::{Buf, Bytes, BytesMut},
    futures_lite::{io::BufReader, prelude::*, AsyncWriteExt},
    std::{
        io::Error,
        pin::Pin,
        task::{Context, Poll},
    },
};

pub struct GzipEncode<B> {
    end: bool,
    enc: write::GzipEncoder<Writer>,
    body: B,
}

impl<B> GzipEncode<B> {
    pub fn new(body: B) -> Self {
        let write = Writer(BytesMut::new());
        let enc = write::GzipEncoder::new(write);
        Self {
            end: false,
            enc,
            body,
        }
    }
}

impl<B> Body for GzipEncode<B>
where
    B: Body,
{
    type Chunk = Bytes;

    async fn chunk(&mut self) -> Option<Result<Self::Chunk, Error>> {
        if self.end {
            return None;
        }

        let chunk = match self.body.chunk().await {
            Some(Ok(chunk)) => chunk,
            Some(Err(e)) => return Some(Err(e)),
            None => {
                self.end = true;

                self.enc.flush().await.expect("infallible flush");
                let bytes = self.enc.get_mut().0.split().freeze();
                return Some(Ok(bytes));
            }
        };

        let buf = chunk.chunk();
        if buf.is_empty() {
            return Some(Ok(Bytes::new()));
        }

        self.enc.write_all(buf).await.expect("infallible write");
        let bytes = self.enc.get_mut().0.split().freeze();
        Some(Ok(bytes))
    }

    fn size_hint(&self) -> Hint {
        Hint::Chunked { end: false }
    }
}

struct Writer(BytesMut);

impl AsyncWrite for Writer {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, Error>> {
        self.0.extend_from_slice(buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Error>> {
        Poll::Ready(Ok(()))
    }
}

pub struct GzipDecode<R> {
    dec: bufread::GzipDecoder<R>,
    buf: BytesMut,
}

impl<R> GzipDecode<R> {
    pub fn from_buf_read(read: R) -> Self
    where
        R: AsyncBufRead + Unpin,
    {
        let dec = bufread::GzipDecoder::new(read);
        let buf = BytesMut::new();
        Self { dec, buf }
    }
}

impl<R> GzipDecode<BufReader<R>> {
    pub fn from_read(read: R) -> Self
    where
        R: AsyncRead + Unpin,
    {
        Self::from_buf_read(BufReader::new(read))
    }
}

impl<R> Body for GzipDecode<R>
where
    R: AsyncBufRead + Unpin,
{
    type Chunk = Bytes;

    async fn chunk(&mut self) -> Option<Result<Self::Chunk, Error>> {
        const READ_LEN: usize = 128;

        if self.buf.len() < READ_LEN {
            self.buf.resize(READ_LEN, 0);
        }

        match self.dec.read(&mut self.buf).await {
            Ok(0) => None,
            Ok(n) => Some(Ok(self.buf.split_to(n).freeze())),
            Err(e) => Some(Err(e)),
        }
    }

    fn size_hint(&self) -> Hint {
        Hint::Chunked { end: false }
    }
}

#[cfg(test)]
mod tests {
    use {super::*, crate::body::BodyExt, futures_lite::future, std::pin};

    #[test]
    fn gzip_encode() {
        let s = "hello world!";
        let enc = GzipEncode::new(s);
        let out = future::block_on(enc.bytes()).expect("encode body");

        assert_eq!(
            &*out,
            [
                31, 139, 8, 0, 0, 0, 0, 0, 0, 255, 202, 72, 205, 201, 201, 87, 40, 207, 47, 202,
                73, 81, 4, 0, 0, 0, 255, 255,
            ]
        );
    }

    #[test]
    fn gzip_decode() {
        let s = include_bytes!("../test/hello.gzip");
        let body = pin::pin!(s.as_slice().read());
        let dec = GzipDecode::from_read(body);
        let out = future::block_on(dec.text()).expect("decode body");
        assert_eq!(out, "hello world!");
    }
}
