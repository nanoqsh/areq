#![doc = include_str!("../README.md")]
#![allow(async_fn_in_trait)]

mod connect;
mod io;

/// The crate's prelude.
pub mod prelude {
    pub use {crate::Connect as _, areq::prelude::*};
}

/// Base `areq` crate.   
pub mod areq {
    pub use areq::*;
}

pub use crate::{connect::Connect, io::Io};
