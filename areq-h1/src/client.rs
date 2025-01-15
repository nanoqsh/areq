use {
    crate::{
        body::{self, Body, IntoBody, Kind},
        error::Error,
        handler::{Handler, Parser, ReadStrategy},
        headers::{self, ContentLen},
    },
    async_channel::{Receiver, Sender},
    bytes::{Buf, Bytes},
    futures_lite::prelude::*,
    http::{header, HeaderValue, Request, Response},
    std::{
        fmt, io,
        pin::{self, Pin},
    },
};

#[derive(Clone)]
pub struct Config {
    parser: Parser,
    read_strategy: ReadStrategy,
}

impl Config {
    #[inline]
    pub fn read_strategy(mut self, read_strategy: ReadStrategy) -> Self {
        self.read_strategy = read_strategy;
        self
    }

    #[inline]
    pub fn max_headers(mut self, n: usize) -> Self {
        self.parser.set_max_headers(n);
        self
    }

    #[inline]
    pub fn handshake<I, B>(self, io: I) -> (Requester<B>, impl Future<Output = ()>)
    where
        I: AsyncRead + AsyncWrite,
        B: IntoBody,
    {
        let (send_req, recv_req) = async_channel::bounded(1);
        let (send_res, recv_res) = async_channel::bounded(1);
        let reqs = Requester { send_req, recv_res };
        let conn = async move {
            let io = pin::pin!(io);
            let conn = Connection {
                recv_req,
                send_res,
                io: Handler::new(io, self.read_strategy),
                parser: self.parser,
            };

            connect(conn).await;
        };

        (reqs, conn)
    }
}

impl Default for Config {
    #[inline]
    fn default() -> Self {
        Self {
            parser: Parser::new(),
            read_strategy: ReadStrategy::default(),
        }
    }
}

struct Connection<'pin, I, B> {
    recv_req: Receiver<Request<B>>,
    send_res: Sender<Result<Response<FetchBody>, Error>>,
    io: Handler<Pin<&'pin mut I>>,
    parser: Parser,
}

async fn connect<I, B>(mut conn: Connection<'_, I, B>)
where
    I: AsyncRead + AsyncWrite,
    B: IntoBody,
{
    while let Ok(req) = conn.recv_req.recv().await {
        let process = async {
            let (parts, body) = req.into_parts();

            let mut head = Request::from_parts(parts, ());
            let mut body = body.into_body();

            match body.kind() {
                Kind::Full => {
                    let full = body::take_full(body).await?;

                    let chunk = full.chunk();
                    let chunk_len = HeaderValue::from(chunk.len());

                    head.headers_mut().insert(header::CONTENT_LENGTH, chunk_len);
                    headers::remove_chunked_encoding(head.headers_mut());

                    conn.io.write_header(&head).await?;
                    conn.io.write_body(chunk).await?;
                }
                Kind::Chunked => {
                    head.headers_mut().remove(header::CONTENT_LENGTH);
                    headers::insert_chunked_encoding(head.headers_mut());

                    conn.io.write_header(&head).await?;
                    while let Some(chunk) = body.chunk().await {
                        conn.io.write_chunk(chunk?.chunk()).await?;
                        conn.io.flush().await?;
                    }

                    conn.io.write_chunk(&[]).await?;
                }
            }

            conn.io.flush().await?;

            let head = conn.io.read_header().await?;
            let res = conn.parser.parse_header(head)?;

            let headers = res.headers();
            let state = match headers::parse_content_len(headers) {
                ContentLen::Num(n) => ReadBodyState::Remaining(n),
                ContentLen::None if headers::has_chunked_encoding(headers) => {
                    ReadBodyState::Chunked
                }
                _ => return Err(Error::invalid_input()),
            };

            Ok((res, state))
        };

        let (give, fetch) = async_channel::bounded(16);
        let mut state = match process.await {
            Ok((res, state)) => {
                let res = res.map(|_| FetchBody { fetch, end: false });
                _ = conn.send_res.send(Ok(res)).await;
                state
            }
            Err(e) => {
                _ = conn.send_res.send(Err(e)).await;
                continue;
            }
        };

        loop {
            let (frame, end) = match &mut state {
                ReadBodyState::Remaining(0) => (Ok(Bytes::new()), true),
                ReadBodyState::Remaining(n) => (conn.io.read_body(n).await, false),
                ReadBodyState::Chunked => {
                    let chunk = conn.io.read_chunk().await;
                    let end = chunk.as_ref().is_ok_and(Bytes::is_empty);
                    (chunk, end)
                }
            };

            let error = frame.is_err();
            let next = Next { frame, end };
            if give.send(next).await.is_err() || error || end {
                break;
            }
        }
    }
}

#[derive(Clone, Copy)]
enum ReadBodyState {
    Remaining(usize),
    Chunked,
}

pub struct Requester<B> {
    send_req: Sender<Request<B>>,
    recv_res: Receiver<Result<Response<FetchBody>, Error>>,
}

impl<B> Requester<B> {
    #[inline]
    pub async fn send(&self, req: Request<B>) -> Result<Response<FetchBody>, Error>
    where
        B: IntoBody,
    {
        self.send_req.send(req).await.map_err(|_| Error::Closed)?;
        self.recv_res.recv().await.map_err(|_| Error::Closed)?
    }
}

struct Next {
    frame: Result<Bytes, Error>,
    end: bool,
}

pub struct FetchBody {
    fetch: Receiver<Next>,
    end: bool,
}

impl FetchBody {
    #[inline]
    pub async fn frame(&mut self) -> Result<Bytes, Error> {
        let Next { frame, end } = self.fetch.recv().await.map_err(|_| Error::Closed)?;
        self.end = end;
        frame
    }
}

impl fmt::Debug for FetchBody {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FetchBody").finish()
    }
}

impl Body for FetchBody {
    type Chunk = Bytes;

    #[inline]
    async fn chunk(&mut self) -> Option<Result<Self::Chunk, io::Error>> {
        match self.frame().await {
            Ok(chunk) => {
                if chunk.is_empty() {
                    None
                } else {
                    Some(Ok(chunk))
                }
            }
            Err(e) => Some(Err(e.into())),
        }
    }

    #[inline]
    fn kind(&self) -> Kind {
        Kind::Chunked
    }

    #[inline]
    fn is_end(&self) -> bool {
        self.end
    }
}

#[cfg(test)]
mod tests {
    use {super::*, crate::test, futures_lite::future};

    fn run<C, R>(conn: C, reqs: R) -> Result<(), Error>
    where
        C: Future,
        R: Future<Output = Result<(), Error>>,
    {
        future::block_on(future::or(
            async {
                conn.await;
                Ok(())
            },
            reqs,
        ))
    }

    #[test]
    fn roundtrip_empty() -> Result<(), Error> {
        const REQUEST: [&str; 3] = ["GET / HTTP/1.1\r\n", "content-length: 0\r\n", "\r\n"];
        const RESPONSE: [&str; 3] = ["HTTP/1.1 200 OK\r\n", "content-length: 0\r\n", "\r\n"];

        let read = test::parts(RESPONSE.map(str::as_bytes));
        let mut write = vec![];
        let io = test::io(read, &mut write);

        let (reqs, conn) = Config::default().handshake(io);
        run(conn, async {
            let req = Request::new(());
            let mut res = reqs.send(req).await?;

            let empty = res.body_mut().frame().await?;
            assert!(empty.is_empty());
            Ok(())
        })?;

        assert_eq!(String::from_utf8(write), Ok(REQUEST.concat()));
        Ok(())
    }

    #[test]
    fn roundtrip_full() -> Result<(), Error> {
        const REQUEST_BODY: &str = "Hello, request!";
        const REQUEST: [&str; 4] = [
            "GET / HTTP/1.1\r\n",
            "content-length: 15\r\n",
            "\r\n",
            REQUEST_BODY,
        ];

        const RESPONSE_BODY: &str = "Hello, response!";
        const RESPONSE: [&str; 4] = [
            "HTTP/1.1 200 OK\r\n",
            "content-length: 16\r\n",
            "\r\n",
            RESPONSE_BODY,
        ];

        let read = test::parts(RESPONSE.map(str::as_bytes));
        let mut write = vec![];
        let io = test::io(read, &mut write);

        let (reqs, conn) = Config::default().handshake(io);
        run(conn, async {
            let req = Request::new(REQUEST_BODY);
            let mut res = reqs.send(req).await?;

            let body = res.body_mut().frame().await?;
            assert_eq!(body, RESPONSE_BODY);

            let empty = res.body_mut().frame().await?;
            assert!(empty.is_empty());
            Ok(())
        })?;

        assert_eq!(String::from_utf8(write), Ok(REQUEST.concat()));
        Ok(())
    }

    #[test]
    fn roundtrip_chunked() -> Result<(), Error> {
        use {
            crate::body::Chunked,
            futures_lite::{stream, StreamExt},
        };

        const CHUNKS: [&str; 5] = ["hello", "from", "the", "internet", ":3"];

        const REQUEST: [&str; 14] = [
            "GET / HTTP/1.1\r\n",
            "transfer-encoding: chunked\r\n",
            "\r\n5\r\n",
            CHUNKS[0],
            "\r\n4\r\n",
            CHUNKS[1],
            "\r\n3\r\n",
            CHUNKS[2],
            "\r\n8\r\n",
            CHUNKS[3],
            "\r\n2\r\n",
            CHUNKS[4],
            "\r\n0\r\n",
            "\r\n",
        ];

        const RESPONSE: [&str; 14] = [
            "HTTP/1.1 200 OK\r\n",
            "transfer-encoding: chunked\r\n",
            "\r\n5\r\n",
            CHUNKS[0],
            "\r\n4\r\n",
            CHUNKS[1],
            "\r\n3\r\n",
            CHUNKS[2],
            "\r\n8\r\n",
            CHUNKS[3],
            "\r\n2\r\n",
            CHUNKS[4],
            "\r\n0\r\n",
            "\r\n",
        ];

        let read = test::parts(RESPONSE.map(str::as_bytes));
        let mut write = vec![];
        let io = test::io(read, &mut write);

        let (reqs, conn) = Config::default().handshake(io);
        run(conn, async {
            let body = stream::iter(CHUNKS).map(str::as_bytes).map(Ok);
            let req = Request::new(Chunked(body));
            let mut res = reqs.send(req).await?;
            for expected in CHUNKS {
                let chunk = res.body_mut().frame().await?;
                assert_eq!(chunk, expected);
            }

            let empty = res.body_mut().frame().await?;
            assert!(empty.is_empty());
            Ok(())
        })?;

        assert_eq!(String::from_utf8(write), Ok(REQUEST.concat()));
        Ok(())
    }

    #[test]
    fn handshake_is_send() {
        fn assert_send<S>(_: S)
        where
            S: Send,
        {
        }

        let read: &[u8] = &[];
        let write = vec![];
        let io = test::io(read, write);
        let (reqs, conn): (Requester<&[u8]>, _) = Config::default().handshake(io);
        assert_send(reqs);
        assert_send(conn);
    }
}
