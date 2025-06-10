pub mod sealed;
pub mod shared;
pub mod stopwatch;
pub mod sync;
pub mod time;

pub mod prelude {
    pub use crate::sealed;
    pub use crate::shared::*;
    pub use crate::stopwatch::Stopwatch;
    pub use crate::sync::channel as local_channel;
    pub use crate::sync::condvar as local_condvar;
    pub use crate::sync::oneshot as local_oneshot;
    pub use crate::sync::semaphore as local_semaphore;
    pub use crate::{
        debug_stopwatch, error_stopwatch, info_stopwatch, trace_stopwatch, warn_stopwatch,
    };
    pub use crate::{millisec, min, sec};
}
