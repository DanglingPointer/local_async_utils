macro_rules! debug_stopwatch {
    ($($arg:tt)+) => {
        local_async_utils::debug_stopwatch!(local_async_utils::millisec!(1), $($arg)+)
    };
}

macro_rules! trace_stopwatch {
    ($($arg:tt)+) => {
        local_async_utils::trace_stopwatch!(std::time::Duration::ZERO, $($arg)+)
    };
}

macro_rules! info_stopwatch {
    ($($arg:tt)+) => {
        local_async_utils::info_stopwatch!(local_async_utils::millisec!(1), $($arg)+)
    };
}

macro_rules! warn_stopwatch {
    ($($arg:tt)+) => {
        local_async_utils::warn_stopwatch!(local_async_utils::sec!(1), $($arg)+)
    };
}

macro_rules! error_stopwatch {
    ($($arg:tt)+) => {
        local_async_utils::error_stopwatch!(local_async_utils::sec!(10), $($arg)+)
    };
}

#[test]
fn test_construct_stopwatches() {
    let _trace = trace_stopwatch!("Trace");
    let _debug = debug_stopwatch!("Debug");
    let _info = info_stopwatch!("Info");
    let _warn = warn_stopwatch!("Warning");
    let _err = error_stopwatch!("Error");
}
