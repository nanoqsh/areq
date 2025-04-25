#![cfg_attr(doc, doc = include_str!("../README.md"))]
#![allow(async_fn_in_trait)]

mod connect;
mod handle;

/// Smol related traits.
pub mod smol {
    pub use crate::{connect::Connect, handle::Handle};
}

/// The crate's prelude.
pub mod prelude {
    pub use {
        crate::smol::{Connect as _, Handle as _},
        areq::prelude::*,
    };
}

pub use areq::*;
