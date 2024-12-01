use {
    async_executor::Executor,
    async_net::{TcpListener, TcpStream},
    axum::{
        body::Body,
        http::{Request, Response},
        routing, Router,
    },
    futures_lite::future,
    hyper::{
        body::Incoming,
        server::conn::{http1, http2},
        service::Service,
    },
    std::{
        convert::Infallible, future::Future, io::Error, net::Ipv4Addr, pin::Pin, sync::Arc, thread,
    },
    tower::{util::Oneshot, ServiceExt},
};

fn main() {
    use async_net::TcpListener;

    async fn handler() -> &'static str {
        "Hello, World!"
    }

    async fn run(ex: Arc<Executor<'_>>) -> Result<(), Error> {
        use futures_lite::future;

        let router = Router::new().route("/", routing::get(handler));

        let http1 = {
            let router = router.clone();
            async {
                let (ip, port) = (Ipv4Addr::LOCALHOST, 3000);
                let tcp = TcpListener::bind((ip, port)).await?;
                println!("serve http1 on {ip}:{port}");

                let h1 = H1 { router };
                serve(&ex, tcp, h1).await
            }
        };

        let http2 = async {
            let (ip, port) = (Ipv4Addr::LOCALHOST, 3001);
            let tcp = TcpListener::bind((ip, port)).await?;
            println!("serve http2 on {ip}:{port}");

            let h2 = H2 {
                router,
                ex: Arc::clone(&ex),
            };

            serve(&ex, tcp, h2).await
        };

        future::or(http1, http2).await
    }

    let n_threads = 2;
    if let Err(e) = block_on_thread_pool(n_threads, |ex| Box::pin(run(ex))) {
        eprintln!("runtime error: {e}");
    }
}

trait Serve {
    fn serve(self, stream: TcpStream) -> impl Future<Output = Result<(), Error>> + Send;
}

#[derive(Clone)]
struct H1 {
    router: Router,
}

impl Serve for H1 {
    async fn serve(self, stream: TcpStream) -> Result<(), Error> {
        use smol_hyper::rt::{FuturesIo, SmolTimer};

        let io = FuturesIo::new(stream);
        http1::Builder::new()
            .timer(SmolTimer::new())
            .serve_connection(io, App(self.router))
            .await
            .map_err(Error::other)
    }
}

#[derive(Clone)]
struct H2<'ex> {
    router: Router,
    ex: Arc<Executor<'ex>>,
}

impl Serve for H2<'_> {
    async fn serve(self, stream: TcpStream) -> Result<(), Error> {
        use smol_hyper::rt::{FuturesIo, SmolExecutor, SmolTimer};

        let io = FuturesIo::new(stream);
        http2::Builder::new(SmolExecutor::new(self.ex))
            .timer(SmolTimer::new())
            .serve_connection(io, App(self.router))
            .await
            .map_err(Error::other)
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

async fn serve<'ex, S>(ex: &Executor<'ex>, tcp: TcpListener, serve: S) -> Result<(), Error>
where
    S: Serve + Clone + Send + 'ex,
{
    loop {
        let serve = serve.clone();
        let (stream, _) = tcp.accept().await?;
        let task = async {
            if let Err(e) = serve.serve(stream).await {
                eprintln!("hyper error: {e}");
            }
        };

        ex.spawn(task).detach();
    }
}

fn block_on_thread_pool<F, U>(n_threads: usize, f: F) -> U
where
    F: FnOnce(Arc<Executor>) -> Pin<Box<dyn Future<Output = U> + '_>>,
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
