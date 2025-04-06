use {
    axum::{
        Router,
        extract::Path,
        response::{IntoResponse, Response},
        routing,
    },
    bytes::Bytes,
    futures_concurrency::prelude::*,
    futures_lite::{future, prelude::*, stream},
    futures_rustls::{
        TlsAcceptor,
        rustls::{ServerConfig, lock::Mutex},
    },
    http_body_util::StreamBody,
    hyper::{
        body::Frame,
        header::{self, HeaderValue},
        http,
        server::conn::{http1, http2},
        service,
    },
    smol::{
        Executor, Timer, channel,
        net::{TcpListener, TcpStream},
    },
    std::{
        collections::HashMap, convert::Infallible, io::Error, net::Ipv4Addr, pin, sync::Arc,
        thread, time::Duration,
    },
    tower::ServiceExt,
};

fn main() {
    async fn handler() -> &'static str {
        "Hello, World!"
    }

    async fn events() -> impl IntoResponse {
        let stream = stream::repeat(())
            .then(|()| async {
                Timer::after(Duration::from_secs(1)).await;
                const { Bytes::from_static(b"data: {}\n\n") }
            })
            .map(Frame::data)
            .map(Ok::<_, Infallible>)
            .boxed();

        let mut res = Response::new(StreamBody::new(stream));

        res.headers_mut().insert(
            header::CONTENT_TYPE,
            const { HeaderValue::from_static("text/event-stream") },
        );

        res.headers_mut().insert(
            header::CACHE_CONTROL,
            const { HeaderValue::from_static("no-cache") },
        );

        res
    }

    #[derive(Clone)]
    struct State(Arc<Mutex<HashMap<String, u32>>>);

    impl State {
        fn set(&self, key: String, val: u32) -> Option<u32> {
            self.0.lock().expect("lock mutex").insert(key, val)
        }

        fn get(&self, key: &str) -> Option<u32> {
            self.0.lock().expect("lock mutex").get(key).copied()
        }

        fn set_kv(self, key: String, val: String) -> String {
            let Ok(n) = val.parse() else {
                return format!("failed to parse {val}");
            };

            self.set(key, n)
                .map(|n| n.to_string())
                .unwrap_or_else(|| String::from("none"))
        }

        fn get_kv(self, key: String) -> String {
            self.get(&key)
                .map(|n| n.to_string())
                .unwrap_or_else(|| String::from("none"))
        }
    }

    let state = State(Arc::new(Mutex::new(HashMap::new())));

    let router = Router::new()
        .route("/hello", routing::get(handler))
        .route("/events", routing::get(events))
        .route("/kv/{key}", {
            let state = state.clone();
            routing::post(async |Path(key), val| state.set_kv(key, val))
        })
        .route("/kv/{key}", {
            let state = state.clone();
            routing::get(async |Path(key)| state.get_kv(key))
        });

    let h1 = H1(router.clone());
    let h2 = H2(router);

    let acceptor = match load_tls_config() {
        Ok(conf) => TlsAcceptor::from(Arc::new(conf)),
        Err(e) => {
            eprintln!("failed to load tls config: {e}");
            return;
        }
    };

    let tls = Tls {
        acceptor,
        h1: &h1,
        h2: &h2,
    };

    async fn listen(port: u16, scheme: &str, proto: &str) -> Result<TcpListener, Error> {
        let ip = Ipv4Addr::LOCALHOST;
        let tcp = TcpListener::bind((ip, port)).await?;
        println!("serve {proto} on {scheme}://{ip}:{port}");
        Ok(tcp)
    }

    block_on_thread_pool(2, async |ex| {
        let tasks = (
            {
                let ex = ex.clone();
                async {
                    let tcp = listen(3001, "http", "http1").await?;
                    serve(ex, &h1, tcp).await
                }
            },
            {
                let ex = ex.clone();
                async {
                    let tcp = listen(3002, "http", "http2").await?;
                    serve(ex, &h2, tcp).await
                }
            },
            {
                let ex = ex.clone();
                async {
                    let tcp = listen(3003, "https", "http1 and http2").await?;
                    serve(ex, &tls, tcp).await
                }
            },
        );

        if let Err(e) = tasks.try_join().await {
            eprintln!("runtime error: {e}");
        }
    });
}

async fn serve<'ex, S>(ex: Arc<Executor<'ex>>, serve: &'ex S, tcp: TcpListener) -> Result<(), Error>
where
    S: Serve<TcpStream> + Sync,
{
    loop {
        let (stream, _) = tcp.accept().await?;
        let task = {
            let ex = ex.clone();
            async {
                if let Err(e) = serve.serve(ex, stream).await {
                    eprintln!("serve error: {e}");
                }
            }
        };

        ex.spawn(task).detach();
    }
}

fn block_on_thread_pool<'ex, F, U>(n_threads: usize, f: F) -> U
where
    F: AsyncFnOnce(Arc<Executor<'ex>>) -> U,
{
    let ex = Arc::new(Executor::new());
    let (stop, wait) = channel::unbounded::<Infallible>();
    thread::scope(|scope| {
        for _ in 0..n_threads {
            scope.spawn(|| future::block_on(ex.run(wait.recv())));
        }

        let main = f(Arc::clone(&ex));
        let result = future::block_on(ex.run(main));
        drop(stop);
        result
    })
}

trait Serve<I> {
    fn serve(&self, ex: Arc<Executor<'_>>, io: I)
    -> impl Future<Output = Result<(), Error>> + Send;
}

struct H1(Router);

impl<I> Serve<I> for H1
where
    I: AsyncRead + AsyncWrite + Send,
{
    async fn serve(&self, _: Arc<Executor<'_>>, io: I) -> Result<(), Error> {
        use smol_hyper::rt::{FuturesIo, SmolTimer};

        let io = pin::pin!(FuturesIo::new(io));
        http1::Builder::new()
            .timer(SmolTimer::new())
            .serve_connection(
                io,
                service::service_fn(|req| {
                    print_request(&req);
                    self.0.clone().oneshot(req)
                }),
            )
            .await
            .map_err(Error::other)
    }
}

struct H2(Router);

impl<I> Serve<I> for H2
where
    I: AsyncRead + AsyncWrite + Send,
{
    async fn serve(&self, ex: Arc<Executor<'_>>, io: I) -> Result<(), Error> {
        use smol_hyper::rt::{FuturesIo, SmolExecutor, SmolTimer};

        let io = pin::pin!(FuturesIo::new(io));
        http2::Builder::new(SmolExecutor::new(ex))
            .timer(SmolTimer::new())
            .serve_connection(
                io,
                service::service_fn(|req| {
                    print_request(&req);
                    self.0.clone().oneshot(req)
                }),
            )
            .await
            .map_err(Error::other)
    }
}

struct Tls<'ex> {
    acceptor: TlsAcceptor,
    h1: &'ex H1,
    h2: &'ex H2,
}

impl Serve<TcpStream> for Tls<'_> {
    async fn serve(&self, ex: Arc<Executor<'_>>, io: TcpStream) -> Result<(), Error> {
        let io = self.acceptor.accept(io).await?;
        let (_, conn) = io.get_ref();
        match conn.alpn_protocol() {
            Some(b"http/1.1") => self.h1.serve(ex, io).await,
            Some(b"h2") => self.h2.serve(ex, io).await,
            _ => Err(Error::other("undefined alpn protocol")),
        }
    }
}

fn load_tls_config() -> Result<ServerConfig, Error> {
    let cert = include_bytes!("../../certs/cert.pem");
    let pkey = include_bytes!("../../certs/pkey.pem");

    let certs: Result<Vec<_>, _> = rustls_pemfile::certs(&mut &cert[..]).collect();
    let pkey = rustls_pemfile::private_key(&mut &pkey[..])?
        .ok_or_else(|| Error::other("no private key found"))?;

    let mut conf = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs?, pkey)
        .map_err(Error::other)?;

    conf.alpn_protocols
        .extend(["h2", "http/1.1"].map(Vec::from));

    Ok(conf)
}

fn print_request<B>(req: &http::Request<B>) {
    use std::io::{self, Write};

    let mut stdout = io::stdout();

    let method = req.method();
    let uri = req.uri();
    let version = req.version();
    _ = writeln!(&mut stdout, "{method} {uri} {version:?}");

    for (name, val) in req.headers() {
        _ = stdout.write_all(name.as_ref());
        _ = stdout.write_all(b": ");
        _ = stdout.write_all(val.as_bytes());
        _ = stdout.write_all(b"\n");
    }

    _ = stdout.flush();
}
