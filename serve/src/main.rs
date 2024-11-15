use {
    async_executor::Executor,
    std::{
        convert::Infallible, future::Future, io::Error, net::Ipv4Addr, num::NonZero, pin::Pin,
        sync::Arc, thread,
    },
};

fn main() {
    use {
        async_net::TcpListener,
        axum::{routing, Router},
    };

    async fn handler() -> &'static str {
        "Hello, World!"
    }

    async fn run(ex: Arc<Executor<'_>>) -> Result<(), Error> {
        let (ip, port) = (Ipv4Addr::LOCALHOST, 3000);
        let tcp = TcpListener::bind((ip, port)).await?;
        println!("listening on http://{ip}:{port}");

        let app = Router::new().route("/", routing::get(handler));
        smol_axum::serve(ex, tcp, app).await
    }

    let n_threads = thread::available_parallelism().map_or(0, NonZero::get);
    if let Err(e) = block_on_thread_pool(n_threads, |ex| Box::pin(run(ex))) {
        eprintln!("runtime error: {e}");
    }
}

fn block_on_thread_pool<F, U>(n_threads: usize, f: F) -> U
where
    F: for<'ex> FnOnce(Arc<Executor<'ex>>) -> Pin<Box<dyn Future<Output = U> + 'ex>>,
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
