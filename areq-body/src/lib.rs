#![cfg_attr(feature = "rtn", feature(return_type_notation))]

mod body;

pub mod prelude {
    pub use crate::{Body, BodyExt as _, Hint, IntoBody};

    #[cfg(feature = "rtn")]
    pub use crate::BodyExtRtn as _;
}

pub use crate::body::{
    Body, BodyExt, Boxed, BoxedLocal, Chunked, Deferred, Full, Hint, IntoBody, PollBody, Void,
};

#[cfg(feature = "rtn")]
pub use crate::body::{BodyExtRtn, SendBody};
