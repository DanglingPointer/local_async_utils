use std::{fmt, io};

#[derive(PartialEq, Eq)]
pub enum TrySendError<T> {
    Full(T),
    Closed(T),
}

#[derive(PartialEq, Eq)]
pub enum SendError<T> {
    Closed(T),
}

impl<T> fmt::Debug for TrySendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TrySendError::Full(_) => f.write_str("TrySendError::Full(..)"),
            TrySendError::Closed(_) => f.write_str("TrySendError::Closed(..)"),
        }
    }
}

impl<T> fmt::Display for TrySendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TrySendError::Full(_) => f.write_str("channel is full"),
            TrySendError::Closed(_) => f.write_str("channel is closed"),
        }
    }
}

impl<T> std::error::Error for TrySendError<T> {}

impl<T> From<TrySendError<T>> for io::Error {
    fn from(err: TrySendError<T>) -> Self {
        let source = format!("{err}");
        match err {
            TrySendError::Full(_) => io::Error::new(io::ErrorKind::StorageFull, source),
            TrySendError::Closed(_) => io::Error::new(io::ErrorKind::BrokenPipe, source),
        }
    }
}

impl<T> fmt::Debug for SendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SendError::Closed(_) => f.write_str("SendError::Closed(..)"),
        }
    }
}

impl<T> fmt::Display for SendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SendError::Closed(_) => f.write_str("channel is closed"),
        }
    }
}

impl<T> std::error::Error for SendError<T> {}

impl<T> From<SendError<T>> for io::Error {
    fn from(err: SendError<T>) -> Self {
        let source = format!("{err}");
        match err {
            SendError::Closed(_) => io::Error::new(io::ErrorKind::BrokenPipe, source),
        }
    }
}
