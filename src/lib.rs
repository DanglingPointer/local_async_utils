pub mod sealed;
pub mod shared;
#[cfg(feature = "tokio")]
pub mod split;
pub mod stopwatch;
pub mod sync;
mod time;

pub mod prelude {
    pub use crate::sealed;
    pub use crate::shared::*;
    pub use crate::stopwatch::Stopwatch;
    pub use crate::sync::bounded as local_bounded;
    pub use crate::sync::condvar as local_condvar;
    pub use crate::sync::error as local_sync_error;
    pub use crate::sync::oneshot as local_oneshot;
    #[cfg(feature = "tokio")]
    pub use crate::sync::pipe as local_pipe;
    pub use crate::sync::semaphore as local_semaphore;
    pub use crate::sync::unbounded as local_unbounded;
    pub use crate::{
        debug_stopwatch, error_stopwatch, info_stopwatch, trace_stopwatch, warn_stopwatch,
    };
    pub use crate::{define_with, define_with_unchecked};
    pub use crate::{millisec, min, sec};
}
