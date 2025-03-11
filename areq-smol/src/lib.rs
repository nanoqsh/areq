#[cfg(feature = "rtn")]
mod connect;
mod handle;

/// The crate's prelude.
pub mod prelude {
    pub use {crate::Handle as _, areq::prelude::*};

    #[cfg(feature = "rtn")]
    pub use crate::Connect as _;
}

/// Base `areq` crate.   
pub mod areq {
    pub use areq::*;
}

pub use crate::handle::Handle;

#[cfg(feature = "rtn")]
pub use crate::connect::Connect;
