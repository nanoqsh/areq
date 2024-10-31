use {
    crate::{
        body::{Body, Chunk},
        error::Error,
        fu::StreamExt as _,
        handler::{Handler, Parser, ReadStrategy},
        headers::{self, ContentLen},
    },
    async_channel::{Receiver, Sender},
    bytes::{Buf, Bytes},
    futures_core::Stream,
    futures_io::{AsyncRead, AsyncWrite},
    http::{header, HeaderValue, Request, Response},
    std::{
        fmt,
        future::Future,
        pin::{self, Pin},
        task::{Context, Poll},
    },
};

#[derive(Clone)]
pub struct Config {
    parser: Parser,
    read_strategy: ReadStrategy,
}

impl Config {
    pub fn read_strategy(mut self, read_strategy: ReadStrategy) -> Self {
        self.read_strategy = read_strategy;
        self
    }

    pub fn max_headers(mut self, n: usize) -> Self {
        self.parser.set_max_headers(n);
        self
    }

    pub fn handshake<I, B>(self, io: I) -> (Requester<B>, impl Future<Output = ()>)
    where
        I: AsyncRead + AsyncWrite,
        B: Body,
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
    B: Body,
{
    while let Ok(req) = conn.recv_req.recv().await {
        let process = async {
            let (parts, body) = req.into_parts();
            let mut head = Request::from_parts(parts, ());

            match body.chunk() {
                Chunk::Full(full) => {
                    let body = full.chunk();
                    let body_len = HeaderValue::from(body.len());

                    head.headers_mut().insert(header::CONTENT_LENGTH, body_len);
                    headers::remove_chunked_encoding(head.headers_mut());

                    conn.io.write_header(&head).await?;
                    conn.io.write_body(body).await?;
                    conn.io.flush().await?;
                }
                Chunk::Stream(stream) => {
                    head.headers_mut().remove(header::CONTENT_LENGTH);
                    headers::insert_chunked_encoding(head.headers_mut());

                    conn.io.write_header(&head).await?;
                    let mut stream = pin::pin!(stream);
                    while let Some(c) = stream.next().await {
                        conn.io.write_chunk(c.chunk()).await?;
                        conn.io.flush().await?;
                    }

                    conn.io.write_chunk(&[]).await?;
                    conn.io.flush().await?;
                }
            }

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
                let res = res.map(|_| FetchBody { fetch });
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
            if give.send(frame).await.is_err() || error || end {
                break;
            }
        }
    }
}

enum ReadBodyState {
    Remaining(usize),
    Chunked,
}

pub struct Requester<B> {
    send_req: Sender<Request<B>>,
    recv_res: Receiver<Result<Response<FetchBody>, Error>>,
}

impl<B> Requester<B> {
    pub async fn send(&self, req: Request<B>) -> Result<Response<FetchBody>, Error>
    where
        B: Body,
    {
        self.send_req.send(req).await.map_err(|_| Error::Closed)?;
        self.recv_res.recv().await.map_err(|_| Error::Closed)?
    }
}

pub struct FetchBody {
    fetch: Receiver<Result<Bytes, Error>>,
}

impl FetchBody {
    pub async fn frame(&self) -> Result<Bytes, Error> {
        self.fetch.recv().await.map_err(|_| Error::Closed)?
    }

    pub fn into_stream(self) -> BodyStream {
        BodyStream(Box::pin(self.fetch))
    }
}

impl fmt::Debug for FetchBody {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("FetchBody").finish()
    }
}

pub struct BodyStream(Pin<Box<Receiver<Result<Bytes, Error>>>>);

impl Stream for BodyStream {
    type Item = Result<Bytes, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.0).poll_next(cx) {
            Poll::Ready(Some(Ok(bytes))) if bytes.is_empty() => Poll::Ready(None),
            Poll::Ready(o) => Poll::Ready(o),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::fu::{io, parts},
    };

    #[test]
    fn roundtrip_full() -> Result<(), Error> {
        use futures_lite::future;

        const REQUEST_BODY: &str = "Hello, request!";
        const REQUEST: [&str; 4] = [
            "GET / HTTP/1.1\r\n",
            "content-length: 15\r\n",
            "\r\n",
            REQUEST_BODY,
        ];

        const RESPONCE_BODY: &str = "Hello, responce!";
        const RESPONCE: [&str; 4] = [
            "HTTP/1.1 200 OK\r\n",
            "content-length: 16\r\n",
            "\r\n",
            RESPONCE_BODY,
        ];

        let read = parts::make(RESPONCE.map(str::as_bytes));
        let mut write = vec![];
        let io = io::make(read, &mut write);

        let (reqs, conn) = Config::default().handshake(io);
        async_io::block_on(future::or(
            async {
                conn.await;
                Ok::<_, Error>(())
            },
            async {
                let body = REQUEST_BODY.as_bytes();
                let req = Request::new(body);
                let mut res = reqs.send(req).await?;

                let body = res.body_mut().frame().await?;
                assert_eq!(body, RESPONCE_BODY);

                let empty = res.body_mut().frame().await?;
                assert!(empty.is_empty());
                Ok(())
            },
        ))?;

        assert_eq!(String::from_utf8(write), Ok(REQUEST.concat()));
        Ok(())
    }

    #[test]
    fn roundtrip_chunked() -> Result<(), Error> {
        use {
            crate::body::Chunked,
            futures_lite::{future, stream, StreamExt},
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

        const RESPONCE: [&str; 14] = [
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

        let read = parts::make(RESPONCE.map(str::as_bytes));
        let mut write = vec![];
        let io = io::make(read, &mut write);

        let (reqs, conn) = Config::default().handshake(io);
        async_io::block_on(future::or(
            async {
                conn.await;
                Ok::<_, Error>(())
            },
            async {
                let body = stream::iter(CHUNKS).map(str::as_bytes);
                let req = Request::new(Chunked(body));
                let mut res = reqs.send(req).await?;
                for expected in CHUNKS {
                    let chunk = res.body_mut().frame().await?;
                    assert_eq!(chunk, expected);
                }

                let empty = res.body_mut().frame().await?;
                assert!(empty.is_empty());
                Ok(())
            },
        ))?;

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
        let io = io::make(read, write);
        let (reqs, conn): (Requester<&[u8]>, _) = Config::default().handshake(io);
        assert_send(reqs);
        assert_send(conn);
    }
}
