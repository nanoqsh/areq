fn main() {
    use {
        areq_smol::{http::Uri, http1::Http1, prelude::*},
        async_executor::Executor,
        std::io::Error,
    };

    async fn get() -> Result<String, Error> {
        let uri = Uri::from_static("http://127.0.0.1:3001/hello");

        Http1::default()
            .connect(uri.clone())
            .await?
            .handle(async |mut client| client.get(uri).await?.text().await)
            .await
    }

    async fn get_in_executor(ex: &Executor<'_>) -> Result<String, Error> {
        let uri = Uri::from_static("http://127.0.0.1:3001/hello");

        let (mut client, conn) = Http1::default().connect(uri.clone()).await?;
        ex.spawn(conn).detach();

        client.get(uri).await?.text().await
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
