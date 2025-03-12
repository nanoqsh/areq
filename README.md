<div align="center">
    <h1>areq</h1>
    <p>
        Async runtime independent HTTP requests
    </p>
    <p>
        <a href="https://crates.io/crates/areq"><img src="https://img.shields.io/crates/v/areq.svg"></img></a>
        <a href="https://docs.rs/areq"><img src="https://docs.rs/areq/badge.svg"></img></a>
    </p>
</div>

# Development

<div align="center">
    <h4>🚧 The library is currently under development 🚧</h4>
</div>

Many features require an unstable Rust feature – [return type notation](https://blog.rust-lang.org/inside-rust/2024/09/26/rtn-call-for-testing.html). The main crate `areq` has the `rtn` feature, which enables the missing functionality. High-level crates `areq-smol` and `areq-tokio` require this feature for compilation, so to use the library you need to install nightly compiler. Once the RTN feature is stabilized, the library will be usable on stable.

# Features

* Async **only**, no hidden overhead from *blocking* API
* Independent of any async runtime, including [features](https://doc.rust-lang.org/cargo/reference/features.html). Instead, the runtime *extends* the functionality of the base crate
* Zero-cost abstractions for a flexible solution and an ability to choose a simple API when needed
* Modular and configurable – build exactly the HTTP client you need

# Getting Started

Although the library is runtime-independent, it also provides high-level crates for choosing a specific runtime. Currently, two crates are available:

* [`areq-smol`](https://docs.rs/areq-smol) provides support for the [smol](https://docs.rs/smol) runtime
* [`areq-tokio`](https://docs.rs/areq-tokio) provides support for the [tokio](https://docs.rs/tokio) runtime

As an example, let's use tokio and add the crate to a project. You also need to select an HTTP protocol version that the client will support. For this example, let's use HTTP/1.1 which can be enabled with the `http1` cargo feature:

```sh
cargo add areq-tokio -F http1
```

Now you can make a GET request and read a response body:

```rust
fn main() {
    use {
        areq_tokio::{areq::{http::Uri, http1::Http1}, prelude::*},
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
```
