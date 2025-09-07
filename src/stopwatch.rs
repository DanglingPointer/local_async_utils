use std::fmt;
use std::time::Instant;

#[cfg(feature = "tokio-time")]
use tokio::time::Duration;

#[cfg(not(feature = "tokio-time"))]
use std::time::Duration;

/// Utility for measuring the duration of an operation. When dropped, it will log the time elapsed since its creation.
pub struct Stopwatch {
    lvl: log::Level,
    threshold: Duration,
    starttime: Instant,
    location: &'static str,
    what: String,
}

impl Stopwatch {
    pub fn new(
        lvl: log::Level,
        threshold: Duration,
        location: &'static str,
        args: fmt::Arguments,
    ) -> Self {
        Self {
            lvl,
            threshold,
            starttime: Instant::now(),
            location,
            what: fmt::format(args),
        }
    }
}

impl Drop for Stopwatch {
    fn drop(&mut self) {
        let duration = self.starttime.elapsed();
        if duration > self.threshold {
            log::log!(target: self.location, self.lvl, "{} finished in {:?}", self.what, duration);
        }
    }
}

impl fmt::Debug for Stopwatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Stopwatch").finish()
    }
}

/// Creates a [`Stopwatch`] that will log a trace message if the elapsed time exceeds the threshold.
#[macro_export]
macro_rules! trace_stopwatch {
    ($threshold:expr, $($arg:tt)+) => {
        $crate::stopwatch::Stopwatch::new(log::Level::Trace, $threshold, module_path!(), format_args!($($arg)+))
    };
}

/// Creates a [`Stopwatch`] that will log a debug message if the elapsed time exceeds the threshold.
#[macro_export]
macro_rules! debug_stopwatch {
    ($threshold:expr, $($arg:tt)+) => {
        $crate::stopwatch::Stopwatch::new(log::Level::Debug, $threshold, module_path!(), format_args!($($arg)+))
    };
}

/// Creates a [`Stopwatch`] that will log an info message if the elapsed time exceeds the threshold.
#[macro_export]
macro_rules! info_stopwatch {
    ($threshold:expr, $($arg:tt)+) => {
        $crate::stopwatch::Stopwatch::new(log::Level::Info, $threshold, module_path!(), format_args!($($arg)+))
    };
}

/// Creates a [`Stopwatch`] that will log a warning message if the elapsed time exceeds the threshold.
#[macro_export]
macro_rules! warn_stopwatch {
    ($threshold:expr, $($arg:tt)+) => {
        $crate::stopwatch::Stopwatch::new(log::Level::Warn, $threshold, module_path!(), format_args!($($arg)+))
    };
}

/// Creates a [`Stopwatch`] that will log an error message if the elapsed time exceeds the threshold.
#[macro_export]
macro_rules! error_stopwatch {
    ($threshold:expr, $($arg:tt)+) => {
        $crate::stopwatch::Stopwatch::new(log::Level::Error, $threshold, module_path!(), format_args!($($arg)+))
    };
}
