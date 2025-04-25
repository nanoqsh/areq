#![cfg_attr(doc, doc = include_str!("../README.md"))]
#![allow(async_fn_in_trait)]

mod connect;
mod io;

/// Tokio related types and traits.
pub mod tokio {
    pub use crate::{connect::Connect, io::Io};
}

/// The crate's prelude.
pub mod prelude {
    pub use {crate::tokio::Connect as _, areq::prelude::*};
}

pub use areq::*;
