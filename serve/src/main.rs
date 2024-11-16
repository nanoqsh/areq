use {
    async_executor::Executor,
    async_net::TcpListener,
    axum::{
        body::Body,
        http::{Request, Response},
        routing, Router,
    },
    std::{
        convert::Infallible, future::Future, io::Error, net::Ipv4Addr, pin::Pin, sync::Arc, thread,
    },
};

fn main() {
    use async_net::TcpListener;

    async fn handler() -> &'static str {
        "Hello, World!"
    }

    async fn run(ex: Arc<Executor<'_>>) -> Result<(), Error> {
        let (ip, port) = (Ipv4Addr::LOCALHOST, 3000);
        let tcp = TcpListener::bind((ip, port)).await?;
        println!("listening on http://{ip}:{port}");

        let app = Router::new().route("/", routing::get(handler));
        serve(ex, tcp, app).await
    }

    let n_threads = 2;
    if let Err(e) = block_on_thread_pool(n_threads, |ex| Box::pin(run(ex))) {
        eprintln!("runtime error: {e}");
    }
}

async fn serve(ex: Arc<Executor<'_>>, tcp: TcpListener, app: Router) -> Result<(), Error> {
    use {
        hyper::{body::Incoming, server::conn::http1::Builder, service::Service},
        smol_hyper::rt::{FuturesIo, SmolTimer},
        tower::{util::Oneshot, ServiceExt},
    };

    struct App(Router);

    impl Service<Request<Incoming>> for App {
        type Response = Response<Body>;
        type Error = Infallible;
        type Future = Oneshot<Router, Request<Incoming>>;

        fn call(&self, req: Request<Incoming>) -> Self::Future {
            self.0.clone().oneshot(req)
        }
    }

    loop {
        let (stream, _) = tcp.accept().await?;
        let io = FuturesIo::new(stream);
        let app = app.clone();
        let task = ex.spawn(async {
            let serve = Builder::new()
                .timer(SmolTimer::new())
                .serve_connection(io, App(app));

            if let Err(e) = serve.await {
                eprintln!("hyper error: {e}");
            }
        });

        task.detach();
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
            scope.spawn(|| async_io::block_on(ex.run(wait.recv())));
        }

        let main = f(Arc::clone(&ex));
        let result = async_io::block_on(ex.run(main));
        drop(stop);
        result
    })
}
