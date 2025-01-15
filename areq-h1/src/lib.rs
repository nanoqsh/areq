mod bytes;
mod client;
mod error;
mod handler;
mod headers;
#[cfg(test)]
mod test;

pub mod body {
    pub use areq_body::*;
}

pub use crate::{
    client::{Config, FetchBody, Requester},
    error::Error,
    handler::ReadStrategy,
};
