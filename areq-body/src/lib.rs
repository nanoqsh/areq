#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![cfg_attr(feature = "rtn", feature(return_type_notation))]
#![cfg_attr(not(feature = "rtn"), allow(async_fn_in_trait))]

mod body;
#[cfg(feature = "rtn")]
#[cfg_attr(docsrs, doc(cfg(feature = "rtn")))]
mod body_rtn;

/// The crate's prelude.
pub mod prelude {
    pub use crate::{Body, BodyExt as _, Hint, IntoBody};

    #[cfg(feature = "rtn")]
    #[cfg_attr(docsrs, doc(cfg(feature = "rtn")))]
    pub use crate::SendBodyExt as _;
}

pub use crate::body::{
    Body, BodyExt, Boxed, BoxedLocal, Chunked, Deferred, Full, Hint, IntoBody, PollBody, Void,
};

#[cfg(feature = "rtn")]
#[cfg_attr(docsrs, doc(cfg(feature = "rtn")))]
pub use crate::body::{SendBody, SendBodyExt};
