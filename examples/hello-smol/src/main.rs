fn main() {
    use {
        areq_smol::{
            areq::{http::Uri, http1::Http1},
            prelude::*,
        },
        async_executor::Executor,
        std::io::Error,
    };

    async fn get() -> Result<String, Error> {
        let addr = Uri::from_static("http://127.0.0.1:3001");
        let path = Uri::from_static("/hello");

        Http1::default()
            .connect(addr)
            .await?
            .handle(async |mut client| client.get(path).await?.body().text().await)
            .await
    }

    async fn get_in_executor(ex: &Executor<'_>) -> Result<String, Error> {
        let addr = Uri::from_static("http://127.0.0.1:3001");
        let path = Uri::from_static("/hello");

        let (mut client, conn) = Http1::default().connect(addr).await?;
        ex.spawn(conn).detach();

        client.get(path).await?.body().text().await
    }

    async fn run(mode: &str) -> Result<String, Error> {
        match mode {
            "handle" => get().await,
            "executor" => {
                let ex = Executor::new();
                ex.run(get_in_executor(&ex)).await
            }
            unknown => Err(Error::other(format!("unknown mode {unknown}"))),
        }
    }

    let mode = std::env::args().nth(1);
    let mode = mode.as_deref().unwrap_or("handle");
    match futures_lite::future::block_on(run(mode)) {
        Ok(text) => println!("{text}"),
        Err(e) => eprintln!("io error: {e}"),
    }
}
