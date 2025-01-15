#![cfg_attr(feature = "rtn", feature(return_type_notation))]

mod body;

pub mod prelude {
    pub use crate::{Body, BodyExt as _, IntoBody};

    #[cfg(feature = "rtn")]
    pub use crate::BodyExtRtn as _;
}

pub use crate::body::{
    take_full, Body, BodyExt, BoxedBody, BoxedBodySend, Chunked, Deferred, Full, IntoBody, Kind,
    PollBody, Void,
};

#[cfg(feature = "rtn")]
pub use crate::body::BodyExtRtn;
