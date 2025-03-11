fn main() {
    use {
        areq_tokio::{
            areq::{http::Uri, http1::Http1},
            prelude::*,
        },
        std::io::Error,
    };

    async fn request() -> Result<String, Error> {
        let uri = Uri::from_static("http://127.0.0.1:3001");
        let path = Uri::from_static("/hello");

        Http1::default()
            .connect_spawned(uri)
            .await?
            .get(path)
            .await?
            .body()
            .text()
            .await
    }

    let rt = tokio::runtime::Runtime::new();
    match rt.and_then(|rt| rt.block_on(request())) {
        Ok(text) => println!("{text}"),
        Err(e) => eprintln!("io error: {e}"),
    }
}
