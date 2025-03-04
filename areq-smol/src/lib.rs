pub mod addr;
#[cfg(all(feature = "executor", feature = "rtn"))]
mod ex;
#[cfg(all(feature = "http1", feature = "rtn"))]
pub mod once;

pub mod prelude {
    pub use {crate::addr::AddressExt as _, areq::prelude::*};
}

pub mod areq {
    pub use areq::*;
}

#[cfg(all(feature = "executor", feature = "rtn"))]
pub use crate::ex::{Spawned, spawn, spawn_local};
