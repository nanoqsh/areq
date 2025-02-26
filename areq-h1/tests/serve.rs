use {
    async_executor::Executor,
    async_net::{TcpListener, TcpStream},
    bytes::{Buf, BufMut, Bytes, BytesMut},
    futures_lite::future,
    http_body_util::Full,
    hyper::{Request, Response, body::Incoming, server::conn::http1::Builder, service},
    std::{io::Error, net::Ipv4Addr},
};

#[test]
fn serve() -> Result<(), Error> {
    async fn inc(mut req: Request<Incoming>) -> Result<Response<Full<Bytes>>, Error> {
        use http_body_util::BodyExt;

        let body = req
            .body_mut()
            .collect()
            .await
            .map_err(Error::other)?
            .aggregate();

        let mut payload: u32 = serde_json::from_reader(body.reader())?;
        payload += 1;

        let mut json = BytesMut::new();
        serde_json::to_writer((&mut json).writer(), &payload).expect("write json");

        Ok(Response::new(Full::new(json.freeze())))
    }

    async fn serve(tcp: TcpStream) {
        use smol_hyper::rt::FuturesIo;

        let io = FuturesIo::new(tcp);
        let serv = service::service_fn(inc);
        if let Err(e) = Builder::new().serve_connection(io, serv).await {
            eprintln!("serving client error: {e}");
        }
    }

    async fn fetch(
        ex: &Executor<'_>,
        tcp: TcpStream,
        payload: Vec<u32>,
    ) -> Result<Vec<u32>, Error> {
        use areq_h1::{Config, body::Full};

        let (reqs, conn) = Config::default().handshake(tcp);
        ex.spawn(conn).detach();

        let mut out = vec![];
        for n in payload {
            let mut json = BytesMut::new();
            serde_json::to_writer((&mut json).writer(), &n).expect("write json");

            let req = Request::new(Full::new(json));
            let mut res = reqs.send(req).await?;

            let mut buf = vec![];
            loop {
                let frame = res.body_mut().frame().await?;
                if frame.is_empty() {
                    break;
                }

                buf.extend_from_slice(&frame);
            }

            let payload = serde_json::from_reader(&buf[..])?;
            out.push(payload);
        }

        Ok(out)
    }

    let ex = Executor::new();
    future::block_on(ex.run(async {
        let (send_addr, recv_addr) = async_channel::bounded(1);
        let server = async {
            let addr = (Ipv4Addr::LOCALHOST, 0);
            let listener = TcpListener::bind(addr).await?;

            _ = send_addr.send(listener.local_addr()?).await;

            let (tcp, _) = listener.accept().await?;
            serve(tcp).await;
            Ok(())
        };

        let client = async {
            let addr = recv_addr
                .recv()
                .await
                .map_err(|_| Error::other("failed to receive socket address"))?;

            let tcp = TcpStream::connect(addr).await?;
            let payload = vec![1, 2, 3];
            fetch(&ex, tcp, payload).await
        };

        let (_, payload) = future::try_zip(server, client).await?;
        assert_eq!(payload, [2, 3, 4]);
        Ok(())
    }))
}
