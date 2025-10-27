//! Synchronization primitives for single-threaded async programming.

pub mod bounded;
pub mod condvar;
pub mod error;
pub mod oneshot;
#[cfg(feature = "tokio-time")]
pub mod pipe;
pub mod semaphore;
mod shared_state;
pub mod unbounded;
mod waker_cell;
