fn main() {
    use {
        areq_smol::{
            areq::{http::Uri, http1::Http1},
            prelude::*,
        },
        async_executor::Executor,
        std::{env, io::Error},
    };

    async fn request() -> Result<String, Error> {
        let uri = Uri::from_static("http://127.0.0.1:3001");

        Http1::default()
            .connect(uri)
            .await?
            .handle(async |mut client| {
                let path = Uri::from_static("/hello");
                client.get(path).await?.body().text().await
            })
            .await
    }

    async fn request_in_executor(ex: &Executor<'_>) -> Result<String, Error> {
        let uri = Uri::from_static("http://127.0.0.1:3001");

        let (mut client, conn) = Http1::default().connect(uri).await?;
        ex.spawn(conn).detach();

        let path = Uri::from_static("/hello");
        client.get(path).await?.body().text().await
    }

    async fn run(mode: &str) -> Result<String, Error> {
        match mode {
            "handle" => request().await,
            "executor" => {
                let ex = Executor::new();
                ex.run(request_in_executor(&ex)).await
            }
            unknown => Err(Error::other(format!("unknown mode {unknown}"))),
        }
    }

    let mode = env::args().nth(1);
    let mode = mode.as_deref().unwrap_or("handle");
    match futures_lite::future::block_on(run(mode)) {
        Ok(text) => println!("{text}"),
        Err(e) => eprintln!("io error: {e}"),
    }
}
