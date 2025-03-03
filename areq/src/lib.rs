#![cfg_attr(feature = "rtn", feature(return_type_notation))]

mod addr;
mod alt;
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

pub mod prelude {
    pub use {
        crate::{Client, Handshake},
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
        proto::{Client, Error, Handshake, Request, Response, Session},
    },
    http,
};

#[cfg(feature = "rtn")]
pub use crate::proto::HandshakeWith;
