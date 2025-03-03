pub mod addr;
#[cfg(feature = "rtn")]
mod ex;

pub mod prelude {
    pub use {crate::addr::AddressExt as _, areq::prelude::*};
}

#[cfg(feature = "rtn")]
pub use crate::ex::{Spawned, spawn_on, spawn_on_local};

pub use areq;
