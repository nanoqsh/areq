#[cfg(feature = "http1")]
pub mod http1;

#[cfg(feature = "http2")]
pub mod http2;

#[cfg(feature = "http2")]
mod io;

pub mod or;
mod proto;
pub mod tls;

pub mod body {
    pub use areq_body::*;
}

pub use {
    crate::proto::{Address, Client, Error, Handshake, InvalidUri, Request, Response, Session},
    http,
};
