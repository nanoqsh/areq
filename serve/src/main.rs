use {
    async_executor::Executor,
    async_net::{TcpListener, TcpStream},
    axum::{
        body::Body,
        http::{Request, Response},
        routing, Router,
    },
    futures_concurrency::prelude::*,
    futures_lite::{future, AsyncRead, AsyncWrite},
    futures_rustls::{rustls::ServerConfig, TlsAcceptor},
    hyper::{
        body::Incoming,
        server::conn::{http1, http2},
        service::Service,
    },
    std::{convert::Infallible, future::Future, io::Error, net::Ipv4Addr, pin, sync::Arc, thread},
    tower::{util::Oneshot, ServiceExt},
};

fn main() {
    async fn handler() -> &'static str {
        "Hello, World!"
    }

    let router = Router::new().route("/hello", routing::get(handler));

    let h1 = H1 {
        router: router.clone(),
    };

    let h2 = H2 { router };

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

    if let Err(e) = block_on_thread_pool(2, |ex| {
        (
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
        )
            .race_ok()
    }) {
        eprintln!("runtime error: {e}");
    }
}

async fn serve<'ex, S>(ex: Arc<Executor<'ex>>, serve: &'ex S, tcp: TcpListener) -> Result<(), Error>
where
    S: Serve<TcpStream> + Sync,
{
    loop {
        let (stream, _) = tcp.accept().await?;
        let task = async {
            if let Err(e) = serve.serve(stream).await {
                eprintln!("serve error: {e}");
            }
        };

        ex.spawn(task).detach();
    }
}

fn block_on_thread_pool<'ex, F, U>(n_threads: usize, f: F) -> U::Output
where
    F: FnOnce(Arc<Executor<'ex>>) -> U,
    U: Future + 'ex,
{
    let ex = Arc::new(Executor::new());
    let (stop, wait) = async_channel::unbounded::<Infallible>();
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
    fn serve(&self, io: I) -> impl Future<Output = Result<(), Error>> + Send;
}

struct H1 {
    router: Router,
}

impl<I> Serve<I> for H1
where
    I: AsyncRead + AsyncWrite + Send,
{
    async fn serve(&self, io: I) -> Result<(), Error> {
        use smol_hyper::rt::{FuturesIo, SmolTimer};

        let io = pin::pin!(FuturesIo::new(io));
        http1::Builder::new()
            .timer(SmolTimer::new())
            .serve_connection(io, App(self.router.clone()))
            .await
            .map_err(Error::other)
    }
}

struct H2 {
    router: Router,
}

impl<I> Serve<I> for H2
where
    I: AsyncRead + AsyncWrite + Send,
{
    async fn serve(&self, io: I) -> Result<(), Error> {
        use smol_hyper::rt::{FuturesIo, SmolExecutor, SmolTimer};

        struct Ex(Executor<'static>);

        impl AsRef<Executor<'static>> for Ex {
            fn as_ref(&self) -> &Executor<'static> {
                &self.0
            }
        }

        let ex = Ex(Executor::new());

        let io = pin::pin!(FuturesIo::new(io));
        let serve = http2::Builder::new(SmolExecutor::new(&ex))
            .timer(SmolTimer::new())
            .serve_connection(io, App(self.router.clone()));

        ex.0.run(serve).await.map_err(Error::other)
    }
}

struct App(Router);

impl Service<Request<Incoming>> for App {
    type Response = Response<Body>;
    type Error = Infallible;
    type Future = Oneshot<Router, Request<Incoming>>;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        self.0.clone().oneshot(req)
    }
}

struct Tls<'ex> {
    acceptor: TlsAcceptor,
    h1: &'ex H1,
    h2: &'ex H2,
}

impl Serve<TcpStream> for Tls<'_> {
    async fn serve(&self, io: TcpStream) -> Result<(), Error> {
        let io = self.acceptor.accept(io).await?;
        let (_, conn) = io.get_ref();
        match conn.alpn_protocol() {
            Some(b"http/1.1") => self.h1.serve(io).await,
            Some(b"h2") => self.h2.serve(io).await,
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

    conf.alpn_protocols.push(Vec::from("h2"));
    Ok(conf)
}
