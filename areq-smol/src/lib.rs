pub mod addr;
#[cfg(all(feature = "executor", feature = "rtn"))]
mod ex;
#[cfg(all(feature = "http1", feature = "rtn"))]
pub mod once;

/// The crate's prelude.
pub mod prelude {
    pub use {crate::addr::AddressExt as _, areq::prelude::*};
}

/// Base `areq` crate.  
pub mod areq {
    pub use areq::*;
}

#[cfg(all(feature = "executor", feature = "rtn"))]
pub use crate::ex::{Spawned, spawn, spawn_local};
