fn main() {
    use {
        areq_smol::{http::Uri, http1::Http1, prelude::*, tls::Tls},
        async_executor::Executor,
        std::io::Error,
    };

    async fn get() -> Result<String, Error> {
        let uri = Uri::from_static("http://127.0.0.1:3001/hello");

        let mut s = String::new();
        Http1::default()
            .connect(&uri)
            .await?
            .handle(async |mut client| client.get(uri, ()).await?.read_to_string(&mut s).await)
            .await?;

        Ok(s)
    }

    async fn get_tls() -> Result<String, Error> {
        let uri = Uri::from_static(
            // fetch this code from github
            "https://raw.githubusercontent.com/nanoqsh/areq/refs/heads/main/examples/hello-smol/src/main.rs",
        );

        let mut s = String::new();
        Tls::with_webpki_roots(Http1::default())
            .connect(&uri)
            .await?
            .handle(async |mut client| client.get(uri, ()).await?.read_to_string(&mut s).await)
            .await?;

        Ok(s)
    }

    async fn get_executor(ex: &Executor<'_>) -> Result<String, Error> {
        let uri = Uri::from_static("http://127.0.0.1:3001/hello");

        let (mut client, conn) = Http1::default().connect(&uri).await?;

        // handle the connection in the smol's executor
        // it will automatically terminate when client is dropped
        ex.spawn(conn).detach();

        let mut s = String::new();
        client.get(uri, ()).await?.read_to_string(&mut s).await?;
        Ok(s)
    }

    async fn run(mode: &str) -> Result<String, Error> {
        match mode {
            "get" => get().await,
            "tls" => get_tls().await,
            "executor" => {
                let ex = Executor::new();
                ex.run(get_executor(&ex)).await
            }
            unknown => Err(Error::other(format!("unknown mode {unknown}"))),
        }
    }

    let mode = std::env::args().nth(1);
    let mode = mode.as_deref().unwrap_or("get");
    match smol::future::block_on(run(mode)) {
        Ok(text) => println!("{text}"),
        Err(e) => eprintln!("error: {e}"),
    }
}
