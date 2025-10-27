use std::cell::RefCell;
use std::io;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// The readable half of a value returned from [`split`].
pub struct ReadHalf<T: AsyncRead>(Rc<RefCell<T>>);

/// The writable half of a value returned from [`split`].
pub struct WriteHalf<T: AsyncWrite>(Rc<RefCell<T>>);

/// Splits a single value implementing `AsyncRead + AsyncWrite` into separate `AsyncRead` and `AsyncWrite` handles.
/// Non-thread-safe equivalent of [`tokio::io::split`](https://docs.rs/tokio/latest/tokio/io/fn.split.html) without the overhead of a mutex.
pub fn split<T: AsyncRead + AsyncWrite>(value: T) -> (ReadHalf<T>, WriteHalf<T>) {
    let shared = Rc::new(RefCell::new(value));
    (ReadHalf(shared.clone()), WriteHalf(shared))
}

fn with_pin<T, R>(half: &RefCell<T>, f: impl FnOnce(Pin<&mut T>) -> R) -> R {
    let mut guard = half.borrow_mut();

    // SAFETY: we do not move the stream
    let stream = unsafe { Pin::new_unchecked(&mut *guard) };

    f(stream)
}

impl<T: AsyncRead> AsyncRead for ReadHalf<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        with_pin(&self.0, |inner| inner.poll_read(cx, buf))
    }
}

impl<T: AsyncWrite> AsyncWrite for WriteHalf<T> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        with_pin(&self.0, |inner| inner.poll_write(cx, buf))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        with_pin(&self.0, |inner| inner.poll_flush(cx))
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        with_pin(&self.0, |inner| inner.poll_shutdown(cx))
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<Result<usize, io::Error>> {
        with_pin(&self.0, |inner| inner.poll_write_vectored(cx, bufs))
    }

    fn is_write_vectored(&self) -> bool {
        self.0.borrow().is_write_vectored()
    }
}
