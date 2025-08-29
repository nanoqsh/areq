#![cfg_attr(doc, doc = include_str!("../README.md"))]
#![allow(async_fn_in_trait)]

mod body;
mod proto;

/// The crate's prelude.
pub mod prelude {
    pub use crate::{Json, JsonBodyExt as _};
}

pub use crate::{body::JsonBodyExt, proto::Json};
