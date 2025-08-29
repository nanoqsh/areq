<div align="center">
    <h1>areq-tokio</h1>
    <p>
        Async HTTP requests for the
        <a href="https://crates.io/crates/tokio">tokio</a>
        runtime
    </p>
    <p>
        This crate is a part of
        <a href="https://crates.io/crates/areq">areq</a>
        library, the runtime-agnostic HTTP requests
    </p>
    <p>
        <a href="https://crates.io/crates/areq-tokio"><img src="https://img.shields.io/crates/v/areq-tokio.svg"></img></a>
        <a href="https://docs.rs/areq-tokio"><img src="https://docs.rs/areq-tokio/badge.svg"></img></a>
    </p>
</div>

# Getting Started

To create an HTTP client, you need to choose a protocol version it'll support. For example, let's use HTTP/1.1. Add the dependency and the required features to a project:

```sh
cargo add areq-tokio -F http1
```

Now you can connect to a remote server, establish an HTTP connection and perform a request:

```rust
use {
    areq_tokio::{http::Uri, http1::Http1, prelude::*},
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
```
