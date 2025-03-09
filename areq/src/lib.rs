#![cfg_attr(feature = "rtn", feature(return_type_notation))]
#![cfg_attr(not(feature = "rtn"), allow(async_fn_in_trait))]

mod addr;
mod alt;
mod client;
#[cfg(feature = "http1")]
pub mod http1;
#[cfg(feature = "http2")]
pub mod http2;
#[cfg(feature = "http2")]
mod io;
mod negotiate;
mod proto;
#[cfg(feature = "rtn")]
mod proto_rtn;
#[cfg(feature = "tls")]
pub mod tls;

/// The crate's prelude.
pub mod prelude {
    pub use {
        crate::{Client, ClientExt as _, ClientWithoutBodyExt as _, Handshake},
        areq_body::prelude::*,
    };
}

pub mod body {
    pub use areq_body::*;
}

pub use {
    crate::{
        addr::{Address, IntoHost, InvalidUri},
        alt::Alt,
        client::{Client, ClientExt, ClientWithoutBodyExt},
        proto::{Error, Handshake, Request, Response, Session},
    },
    bytes, http,
};

#[cfg(feature = "rtn")]
pub use crate::proto::HandshakeWith;
