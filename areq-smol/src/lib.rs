pub mod addr;
#[cfg(all(feature = "executor", feature = "rtn"))]
mod ex;
mod proto;

/// The crate's prelude.
pub mod prelude {
    pub use {
        crate::{Handle as _, addr::AddressExt as _},
        areq::prelude::*,
    };
}

/// Base `areq` crate.  
pub mod areq {
    pub use areq::*;
}

pub use crate::proto::Handle;

#[cfg(all(feature = "executor", feature = "rtn"))]
pub use crate::ex::{Spawned, spawn, spawn_local};
