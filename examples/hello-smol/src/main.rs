fn main() {
    use {futures_lite::future, std::io::Error};

    async fn make_request() -> Result<String, Error> {
        areq_smol::once::get("http://127.0.0.1:3001/hello")?
            .text()
            .await
    }

    match future::block_on(make_request()) {
        Ok(text) => println!("{text}"),
        Err(e) => eprintln!("io error: {e}"),
    }
}
