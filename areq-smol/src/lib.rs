#[cfg(feature = "rtn")]
mod ex;

#[cfg(feature = "rtn")]
pub use crate::ex::{Spawned, spawn_on, spawn_on_local};
