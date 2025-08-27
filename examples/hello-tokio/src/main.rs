fn main() {
    use {
        areq_tokio::{http::Uri, http1::Http1, prelude::*, tls::Tls},
        std::io::Error,
    };

    async fn get() -> Result<String, Error> {
        // uri contains the server address and path to the http resource
        let uri = Uri::from_static("http://127.0.0.1:3001/hello");

        // establish connection to address "127.0.0.1:3001"
        let (mut client, conn) = Http1::default().connect(&uri).await?;

        // tokio will handle the connection in the background
        // it will automatically terminate when client is dropped
        tokio::spawn(conn);

        // perform GET request to the specified path "/hello"
        client.get(uri, ()).await?.text().await
    }

    async fn get_tls() -> Result<String, Error> {
        let uri = Uri::from_static(
            // fetch this code from github
            "https://raw.githubusercontent.com/nanoqsh/areq/refs/heads/main/examples/hello-tokio/src/main.rs",
        );

        let (mut client, conn) = Tls::with_webpki_roots(Http1::default())
            .connect(&uri)
            .await?;

        tokio::spawn(conn);

        client.get(uri, ()).await?.text().await
    }

    async fn run(mode: &str) -> Result<String, Error> {
        match mode {
            "get" => get().await,
            "tls" => get_tls().await,
            unknown => Err(Error::other(format!("unknown mode {unknown}"))),
        }
    }

    let rt = tokio::runtime::Runtime::new();
    let mode = std::env::args().nth(1);
    let mode = mode.as_deref().unwrap_or("get");
    match rt.and_then(|rt| rt.block_on(run(mode))) {
        Ok(text) => println!("{text}"),
        Err(e) => eprintln!("error: {e}"),
    }
}
