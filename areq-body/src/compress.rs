use {
    crate::body::{Body, BodyExt, Hint},
    async_compression::futures::bufread,
    bytes::{Bytes, BytesMut},
    futures_lite::{io::BufReader, prelude::*},
    std::{io::Error, pin::Pin},
};

type BoxedBufRead<'body> = Pin<Box<dyn AsyncBufRead + 'body>>;

pub struct GzipDecode<'body> {
    dec: bufread::GzipDecoder<BoxedBufRead<'body>>,
    buf: BytesMut,
}

impl<'body> GzipDecode<'body> {
    pub fn new<B>(body: B) -> Self
    where
        B: Body + 'body,
    {
        Self::from_buf_read(BufReader::new(body.read()))
    }

    pub fn from_buf_read<R>(read: R) -> Self
    where
        R: AsyncBufRead + 'body,
    {
        let read: BoxedBufRead<'body> = Box::pin(read);
        let dec = bufread::GzipDecoder::new(read);
        let buf = BytesMut::new();
        Self { dec, buf }
    }
}

impl Body for GzipDecode<'_> {
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
    use {super::*, futures_lite::future};

    #[test]
    fn gzip_decode() {
        let s = include_bytes!("../test/hello.gzip");
        let dec = GzipDecode::new(s.as_slice());
        let out = future::block_on(dec.text()).expect("decode body");
        assert_eq!(out, "hello world!");
    }
}
