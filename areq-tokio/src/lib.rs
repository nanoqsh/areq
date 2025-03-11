#[cfg(feature = "rtn")]
mod connect;
pub mod io;

/// The crate's prelude.
pub mod prelude {
    pub use areq::prelude::*;

    #[cfg(feature = "rtn")]
    pub use crate::Connect as _;
}

/// Base `areq` crate.   
pub mod areq {
    pub use areq::*;
}

#[cfg(feature = "rtn")]
pub use crate::connect::Connect;
