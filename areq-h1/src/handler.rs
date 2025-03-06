use {
    crate::{bytes::InitBytesMut, error::Error},
    bytes::Bytes,
    futures_lite::prelude::*,
    http::{Request, Response, Uri, Version},
    httparse::{Header, ParserConfig},
    std::{io::Write, str},
};

const KB: usize = 1 << 10;
const INIT_BUFFER_LEN: usize = 8 * KB;
const MAX_BUFFER_LEN: usize = 128 * KB;

pub(crate) struct Handler<I> {
    io: I,
    read_buf: InitBytesMut,
    read_strategy: Strategy,
    write_buf: Vec<u8>,
}

impl<I> Handler<I> {
    pub fn new(io: I, read_strategy: ReadStrategy) -> Self {
        Self {
            io,
            read_buf: InitBytesMut::new(),
            read_strategy: read_strategy.state(),
            write_buf: Vec::with_capacity(INIT_BUFFER_LEN),
        }
    }

    async fn read_to_buf(&mut self) -> Result<(), Error>
    where
        I: AsyncRead + Unpin,
    {
        let next = self.read_strategy.next();
        if self.read_buf.spare_capacity_len() < next {
            self.read_buf.reserve(next);
        }

        let buf = self.read_buf.spare_capacity_mut();
        if buf.is_empty() {
            return Err(Error::TooLargeInput);
        }

        let n = self.io.read(buf).await?;
        self.read_buf.advance(n);
        self.read_strategy.record(n);

        if n == 0 {
            Err(Error::unexpected_eof())
        } else {
            Ok(())
        }
    }

    async fn read_until(&mut self, sep: &[u8]) -> Result<Bytes, Error>
    where
        I: AsyncRead + Unpin,
    {
        debug_assert!(!sep.is_empty(), "sep must not be empty");

        let mut cursor = 0;
        loop {
            let start = usize::saturating_sub(cursor, sep.len());
            let buf = &self.read_buf.as_mut()[start..];
            if let Some(n) = buf.windows(sep.len()).position(|sub| sub == sep) {
                let at = start + n + sep.len();
                break Ok(self.read_buf.split_to(at).freeze());
            }

            cursor += self.read_buf.len() - cursor;
            self.read_to_buf().await?;
        }
    }

    pub async fn read_header(&mut self) -> Result<Bytes, Error>
    where
        I: AsyncRead + Unpin,
    {
        let bytes = self.read_until(b"\r\n\r\n").await?;
        Ok(bytes)
    }

    pub async fn read_body(&mut self, remaining: &mut usize) -> Result<Bytes, Error>
    where
        I: AsyncRead + Unpin,
    {
        debug_assert_ne!(*remaining, 0, "do not call this when remaining is zero");

        if self.read_buf.is_empty() {
            self.read_to_buf().await?;
        }

        let chunk_len = usize::min(*remaining, self.read_buf.len());
        let chunk = self.read_buf.split_to(chunk_len).freeze();
        *remaining -= chunk_len;
        Ok(chunk)
    }

    pub async fn read_chunk(&mut self) -> Result<Bytes, Error>
    where
        I: AsyncRead + Unpin,
    {
        const SEP: &[u8; 2] = b"\r\n";

        let len = {
            let len_bytes = self.read_until(SEP).await?;
            let len_bytes = len_bytes
                .strip_suffix(SEP)
                .expect("bytes read include suffix");

            let len_str = str::from_utf8(len_bytes).map_err(|_| Error::invalid_input())?;
            let len = usize::from_str_radix(len_str, 16).map_err(|_| Error::invalid_input())?;
            len + SEP.len()
        };

        while self.read_buf.len() < len {
            self.read_to_buf().await?;
        }

        let mut chunk = self.read_buf.split_to(len);
        if chunk.ends_with(SEP) {
            chunk.truncate(chunk.len() - SEP.len());
            Ok(chunk.freeze())
        } else {
            Err(Error::invalid_input())
        }
    }

    pub async fn write_header(&mut self, req: &Request<()>) -> Result<(), Error>
    where
        I: AsyncWrite + Unpin,
    {
        fn write_uri_to_buf(uri: &Uri, buf: &mut Vec<u8>) {
            let n = buf.len();
            _ = write!(buf, "{uri}");

            // uri was not written
            // may happen because of https://github.com/hyperium/http/issues/507
            if n == buf.len() {
                buf.push(b'/');
            }
        }

        fn write_to_buf(req: &Request<()>, buf: &mut Vec<u8>) {
            let method = req.method();
            let uri = req.uri();

            assert_eq!(
                req.version(),
                Version::HTTP_11,
                "only HTTP/1.1 version is supported",
            );

            _ = write!(buf, "{method} ");
            write_uri_to_buf(uri, buf);
            buf.extend_from_slice(b" HTTP/1.1\r\n");
            for (name, value) in req.headers() {
                _ = write!(buf, "{name}: ");
                buf.extend_from_slice(value.as_bytes());
                buf.extend_from_slice(b"\r\n");
            }

            buf.extend_from_slice(b"\r\n");
        }

        self.write_buf.clear();
        write_to_buf(req, &mut self.write_buf);
        self.io.write(&self.write_buf).await?;
        Ok(())
    }

    pub async fn write_body(&mut self, body: &[u8]) -> Result<(), Error>
    where
        I: AsyncWrite + Unpin,
    {
        self.io.write(body).await?;
        Ok(())
    }

    pub async fn write_chunk(&mut self, chunk: &[u8]) -> Result<(), Error>
    where
        I: AsyncWrite + Unpin,
    {
        self.write_buf.clear();
        let chunk_len = chunk.len();
        _ = write!(&mut self.write_buf, "{chunk_len:X}\r\n");

        self.io.write(&self.write_buf).await?;
        self.io.write(chunk).await?;
        self.io.write(b"\r\n").await?;
        Ok(())
    }

    pub async fn flush(&mut self) -> Result<(), Error>
    where
        I: AsyncWrite + Unpin,
    {
        self.io.flush().await?;
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub enum ReadStrategy {
    Exact(usize),
    Adaptive { max: usize },
}

impl ReadStrategy {
    fn state(self) -> Strategy {
        match self {
            Self::Exact(n) => Strategy::Exact(n),
            Self::Adaptive { max } => Strategy::Adaptive {
                next: INIT_BUFFER_LEN,
                max,
            },
        }
    }
}

impl Default for ReadStrategy {
    fn default() -> Self {
        Self::Adaptive {
            max: MAX_BUFFER_LEN,
        }
    }
}

#[derive(Clone, Copy)]
enum Strategy {
    Exact(usize),
    Adaptive { next: usize, max: usize },
}

impl Strategy {
    fn next(self) -> usize {
        match self {
            Self::Exact(n) => n,
            Self::Adaptive { next, .. } => next,
        }
    }

    fn record(&mut self, n: usize) {
        match self {
            Self::Exact(_) => {}
            Self::Adaptive { next, max } => {
                if n >= *next {
                    let incpow = usize::saturating_mul(*next, 2);
                    *next = usize::min(incpow, *max);
                }
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct Parser {
    conf: ParserConfig,
    max_headers: usize,
}

impl Parser {
    const HEADERS_STACK_BUFFER_LEN: usize = 150;

    pub fn new() -> Self {
        Self {
            conf: ParserConfig::default(),
            max_headers: Self::HEADERS_STACK_BUFFER_LEN,
        }
    }

    pub fn set_max_headers(&mut self, n: usize) {
        self.max_headers = n;
    }

    pub fn parse_header(&self, buf: Bytes) -> Result<Response<()>, Error> {
        use {
            http::{HeaderName, HeaderValue, StatusCode},
            httparse::Status,
            std::mem::MaybeUninit,
        };

        let mut out = httparse::Response::new(&mut []);
        let uninit_headers = if self.max_headers <= Self::HEADERS_STACK_BUFFER_LEN {
            &mut [MaybeUninit::uninit(); Self::HEADERS_STACK_BUFFER_LEN][..self.max_headers]
        } else {
            &mut vec![MaybeUninit::uninit(); self.max_headers][..]
        };

        match self
            .conf
            .parse_response_with_uninit_headers(&mut out, &buf, uninit_headers)?
        {
            Status::Complete(n) if n == buf.len() => {}
            _ => panic!("failed to complete parsing"),
        }

        let mut res = Response::new(());
        *res.version_mut() = match out.version {
            Some(9) => return Err(Error::UnsupportedVersion(Version::HTTP_09)),
            Some(0) => return Err(Error::UnsupportedVersion(Version::HTTP_10)),
            Some(1) => Version::HTTP_11,
            _ => return Err(Error::Parse(httparse::Error::Version)),
        };

        *res.status_mut() =
            StatusCode::from_u16(out.code.unwrap_or_default()).expect("valid status code");

        *res.headers_mut() = {
            let entry = |header: Header<'_>| {
                let name =
                    HeaderName::from_bytes(header.name.as_bytes()).expect("valid header name");
                let value = HeaderValue::from_maybe_shared(buf.slice_ref(header.value))
                    .expect("valid header value");

                (name, value)
            };

            out.headers.iter().copied().map(entry).collect()
        };

        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use {super::*, futures_lite::future};

    impl<I> Handler<I> {
        fn test(io: I) -> Self {
            Self::new(io, ReadStrategy::default())
        }
    }

    const RESPONSE: &[u8] = b"\
        HTTP/1.1 200 OK\r\n\
        date: mon, 27 jul 2009 12:28:53 gmt\r\n\
        last-modified: wed, 22 jul 2009 19:15:56 gmt\r\n\
        accept-ranges: bytes\r\n\
        content-length: 4\r\n\
        vary: accept-encoding\r\n\
        content-type: text/plain\r\n\
        \r\n\
        body\
    ";

    fn header() -> &'static [u8] {
        RESPONSE.strip_suffix(b"body").expect("strip body")
    }

    #[test]
    fn read_head() -> Result<(), Error> {
        let mut h = Handler::test(RESPONSE);
        let head = future::block_on(h.read_header())?;
        assert_eq!(head, header());
        Ok(())
    }

    #[test]
    fn parse_head() -> Result<(), Error> {
        use http::{HeaderMap, HeaderName, HeaderValue, StatusCode, Version};

        let res = header();
        let head = Parser::new().parse_header(Bytes::copy_from_slice(res))?;
        assert_eq!(head.status(), StatusCode::OK);
        assert_eq!(head.version(), Version::HTTP_11);

        let headers = [
            ("date", "mon, 27 jul 2009 12:28:53 gmt"),
            ("last-modified", "wed, 22 jul 2009 19:15:56 gmt"),
            ("accept-ranges", "bytes"),
            ("content-length", "4"),
            ("vary", "accept-encoding"),
            ("content-type", "text/plain"),
        ];

        let headers: HeaderMap = headers
            .into_iter()
            .map(|(name, value)| {
                (
                    HeaderName::from_bytes(name.as_bytes()).expect("lowercased header name"),
                    HeaderValue::from_static(value),
                )
            })
            .collect();

        assert_eq!(head.headers(), &headers);
        Ok(())
    }

    #[test]
    fn parse_head_max_headers() -> Result<(), Error> {
        use http::{StatusCode, Version};

        let parser = Parser {
            max_headers: 5,
            ..Parser::new()
        };

        let res = header();
        let e = parser
            .parse_header(Bytes::copy_from_slice(res))
            .expect_err("too many headers");

        assert!(matches!(e, Error::Parse(httparse::Error::TooManyHeaders)));

        let parser = Parser {
            max_headers: 6,
            ..Parser::new()
        };

        let head = parser.parse_header(Bytes::copy_from_slice(res))?;
        assert_eq!(head.status(), StatusCode::OK);
        assert_eq!(head.version(), Version::HTTP_11);
        Ok(())
    }

    #[test]
    fn read_body() -> Result<(), Error> {
        const BODY: &[u8] = b"Hello, World!";

        let mut h = Handler::test(BODY);
        let mut remaining = BODY.len();
        let body = future::block_on(h.read_body(&mut remaining))?;
        assert_eq!(body, BODY);
        assert_eq!(remaining, 0);
        Ok(())
    }

    #[test]
    fn read_response() -> Result<(), Error> {
        let mut h = Handler::test(RESPONSE);
        let head = future::block_on(h.read_header())?;
        assert_eq!(head, header());

        let mut remaining = 4;
        let body = future::block_on(h.read_body(&mut remaining))?;
        assert_eq!(body, b"body"[..]);
        assert_eq!(remaining, 0);
        assert!(h.read_buf.is_empty());
        Ok(())
    }

    #[test]
    fn read_partial() -> Result<(), Error> {
        use crate::test;

        let cases = [
            (&["_", "_", "A"][..], "A", "__A"),
            (&["_", "_", "A", "_"][..], "A", "__A"),
            (&["A", "B"][..], "AB", "AB"),
            (&["A", "B", "C"][..], "ABC", "ABC"),
            (&["___A", "B", "___"][..], "AB", "___AB"),
            (&["___A", "B", "C___"][..], "ABC", "___ABC"),
            (&["_", "__", "_A", "B", "C___"][..], "ABC", "____ABC"),
            (&["_", "__", "_A", "B", "C___"][..], "A", "____A"),
            (&["AA", "_BA_", "_A", "B", "C___"][..], "AB", "AA_BA__AB"),
        ];

        for (reads, until, actual) in cases {
            let parts = test::parts(reads.iter().copied().map(str::as_bytes));
            let mut h = Handler::test(parts);
            let bytes = future::block_on(h.read_until(until.as_bytes()))?;
            assert_eq!(bytes, actual);
        }

        Ok(())
    }

    #[test]
    fn write_head() -> Result<(), Error> {
        use http::{HeaderValue, Method, Uri, Version};

        const REQUEST: &[u8] = b"\
            GET /get HTTP/1.1\r\n\
            name: value\r\n\
            \r\n\
        ";

        let mut req = Request::new(());
        *req.method_mut() = Method::GET;
        *req.uri_mut() = Uri::from_static("/get");
        *req.version_mut() = Version::HTTP_11;
        req.headers_mut()
            .append("name", HeaderValue::from_static("value"));

        let mut write = vec![];
        let mut h = Handler::test(&mut write);
        future::block_on(h.write_header(&req))?;
        assert_eq!(write, REQUEST);
        Ok(())
    }

    #[test]
    fn write_head_empty_path() -> Result<(), Error> {
        use http::{Method, Uri, Version};

        const REQUEST: &[u8] = b"\
            GET / HTTP/1.1\r\n\
            \r\n\
        ";

        let mut req = Request::new(());
        *req.method_mut() = Method::GET;
        *req.uri_mut() = Uri::from_static("s://a")
            .into_parts()
            .path_and_query
            .expect("get empty path")
            .into();

        *req.version_mut() = Version::HTTP_11;

        let mut write = vec![];
        let mut h = Handler::test(&mut write);
        future::block_on(h.write_header(&req))?;
        assert_eq!(write, REQUEST);
        Ok(())
    }

    #[test]
    fn exact_read() -> Result<(), Error> {
        let mut h = Handler::test(RESPONSE);
        h.read_strategy = Strategy::Exact(2);

        future::block_on(h.read_to_buf())?;
        assert_eq!(h.read_strategy.next(), 2);
        Ok(())
    }

    #[test]
    fn adaptive_read() -> Result<(), Error> {
        let mut h = Handler::test(RESPONSE);
        h.read_strategy = Strategy::Adaptive { next: 1, max: 10 };

        for n in [2, 4, 8, 10] {
            future::block_on(h.read_to_buf())?;
            assert_eq!(h.read_strategy.next(), n);
        }

        Ok(())
    }
}
