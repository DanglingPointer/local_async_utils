/// Shortcut for [`std::time::Duration::from_secs`]. Example usage:
/// ```
/// # use local_async_utils::prelude::*;
/// let duration = sec!(5);
/// assert_eq!(duration, std::time::Duration::from_secs(5));
/// ```
#[macro_export]
macro_rules! sec {
    ($arg:expr) => {{ std::time::Duration::from_secs($arg) }};
}

/// Shortcut for [`std::time::Duration::from_millis`]. Example usage:
/// ```
/// # use local_async_utils::prelude::*;
/// let duration = millisec!(1500);
/// assert_eq!(duration, std::time::Duration::from_millis(1500));
/// ```
#[macro_export]
macro_rules! millisec {
    ($arg:expr) => {{ std::time::Duration::from_millis($arg) }};
}

/// Shortcut for [`std::time::Duration::from_secs`]. Example usage:
/// ```
/// # use local_async_utils::prelude::*;
/// let duration = min!(2);
/// assert_eq!(duration, std::time::Duration::from_secs(120));
/// ```
#[macro_export]
macro_rules! min {
    ($arg:expr) => {{ std::time::Duration::from_secs($arg * 60) }};
}
