
Async HTTP requests for [tokio] runtime.

This crate is a part of [areq] library, the runtime-agnostic HTTP requests. If you want your code to run in [tokio] runtime, this crate is the right choice.

[tokio]: https://docs.rs/tokio
[areq]: https://docs.rs/areq

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
