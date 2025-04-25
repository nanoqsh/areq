fn main() {
    use {
        areq_tokio::{http::Uri, http1::Http1, prelude::*},
        std::io::Error,
    };

    async fn get() -> Result<String, Error> {
        let addr = Uri::from_static("http://127.0.0.1:3001");
        let (mut client, conn) = Http1::default().connect(addr).await?;
        tokio::spawn(conn);

        let path = Uri::from_static("/hello");
        client.get(path).await?.text().await
    }

    let rt = tokio::runtime::Runtime::new();
    match rt.and_then(|rt| rt.block_on(get())) {
        Ok(text) => println!("{text}"),
        Err(e) => eprintln!("io error: {e}"),
    }
}
