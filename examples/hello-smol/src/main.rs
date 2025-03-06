fn main() {
    use {
        areq_smol::{
            areq::{http::Uri, http1::Http1},
            prelude::*,
        },
        std::io::Error,
    };

    async fn make_request() -> Result<String, Error> {
        let uri = Uri::from_static("http://127.0.0.1:3001");

        uri.connect()
            .await?
            .handshake(Http1::default())
            .await?
            .handle(async |mut client| {
                let path = Uri::from_static("/hello");
                client.get(path).await?.body().text().await
            })
            .await
    }

    match futures_lite::future::block_on(make_request()) {
        Ok(text) => println!("{text}"),
        Err(e) => eprintln!("io error: {e}"),
    }
}
