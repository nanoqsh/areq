mod client;
pub mod http1;
pub mod http2;
mod io;
mod proto;

pub mod body {
    pub use areq_body::*;
}

pub use {
    crate::{
        client::Client,
        proto::{
            Address, BodyStream, BoxedBody, Error, Protocol, Request, Response, Serve, Session,
        },
    },
    url,
};
