//! Collections that never leak references to their content, and therefore can be safely accessed via shared references.

mod queue;
mod set;
mod utils;

pub use queue::Queue;
pub use set::Set;
