fn main() {
    use {
        areq_tokio::{http::Uri, http1::Http1, prelude::*},
        std::io::Error,
    };

    async fn get() -> Result<String, Error> {
        // uri contains the server address and path to the http resource
        let uri = Uri::from_static("http://127.0.0.1:3001/hello");

        // establish connection to address "127.0.0.1:3001"
        let (mut client, conn) = Http1::default().connect(uri.clone()).await?;

        // tokio will handle the connection in the background
        // it will automatically terminate when client is dropped
        tokio::spawn(conn);

        // perform GET request to the specified path "/hello"
        client.get(uri).await?.text().await
    }

    let rt = tokio::runtime::Runtime::new();
    match rt.and_then(|rt| rt.block_on(get())) {
        Ok(text) => println!("{text}"),
        Err(e) => eprintln!("io error: {e}"),
    }
}
