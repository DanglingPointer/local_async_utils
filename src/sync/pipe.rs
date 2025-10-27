use crate::shared::UnsafeShared;
use std::cell::UnsafeCell;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};
use std::{cmp, io};
use std::{collections::VecDeque, pin::Pin};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// Unidirectional in-memory stream of bytes implementing `AsyncRead` and `AsyncWrite`.
/// Non-thread-safe equivalent of [`tokio::io::SimplexStream`](https://docs.rs/tokio/latest/tokio/io/struct.SimplexStream.html).
#[derive(Debug)]
pub struct Pipe {
    buffer: VecDeque<u8>,
    is_closed: bool,
    max_buf_size: usize,
    read_waker: Option<Waker>,
    write_waker: Option<Waker>,
}

impl Pipe {
    /// Creates a new `Pipe` with the specified maximum buffer size.
    pub fn new(max_buf_size: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(max_buf_size),
            is_closed: false,
            max_buf_size,
            read_waker: None,
            write_waker: None,
        }
    }

    fn close_write(&mut self) {
        self.is_closed = true;
        if let Some(waker) = self.read_waker.take() {
            waker.wake();
        }
    }

    fn close_read(&mut self) {
        self.is_closed = true;
        if let Some(waker) = self.write_waker.take() {
            waker.wake();
        }
    }

    fn poll_read_internal(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut ReadBuf,
    ) -> Poll<io::Result<()>> {
        if !self.buffer.is_empty() {
            let (head, tail) = self.buffer.as_slices();
            let bytes_copied = copy_slice(buf, head) + copy_slice(buf, tail);
            if bytes_copied > 0 {
                truncate_front(&mut self.buffer, bytes_copied);
                if let Some(waker) = self.write_waker.take() {
                    waker.wake();
                }
            }
            Poll::Ready(Ok(()))
        } else if self.is_closed {
            Poll::Ready(Ok(()))
        } else {
            self.read_waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }

    fn poll_write_internal(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if self.is_closed {
            return Poll::Ready(Err(io::ErrorKind::BrokenPipe.into()));
        }
        let available = self.max_buf_size - self.buffer.len();
        if available == 0 {
            self.write_waker = Some(cx.waker().clone());
            return Poll::Pending;
        }

        let bytes_to_copy = cmp::min(buf.len(), available);
        self.buffer.extend(&buf[..bytes_to_copy]);
        if let Some(waker) = self.read_waker.take() {
            waker.wake();
        }
        Poll::Ready(Ok(bytes_to_copy))
    }

    fn poll_write_vectored_internal(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        if self.is_closed {
            return Poll::Ready(Err(io::ErrorKind::BrokenPipe.into()));
        }
        let available = self.max_buf_size - self.buffer.len();
        if available == 0 {
            self.write_waker = Some(cx.waker().clone());
            return Poll::Pending;
        }

        let mut remaining = available;
        for buf in bufs {
            if remaining == 0 {
                break;
            }

            let len = cmp::min(buf.len(), remaining);
            self.buffer.extend(&buf[..len]);
            remaining -= len;
        }

        if let Some(waker) = self.read_waker.take() {
            waker.wake();
        }
        Poll::Ready(Ok(available - remaining))
    }
}

fn copy_slice(dest: &mut ReadBuf, src: &[u8]) -> usize {
    let bytes_to_copy = cmp::min(dest.remaining(), src.len());
    if bytes_to_copy != 0 {
        dest.put_slice(&src[..bytes_to_copy]);
    }
    bytes_to_copy
}

fn truncate_front(deq: &mut VecDeque<u8>, count: usize) {
    assert!(deq.len() >= count);
    let keep = deq.len() - count;
    deq.rotate_left(count);
    deq.truncate(keep);
}

impl AsyncRead for Pipe {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.poll_read_internal(cx, buf)
    }
}

impl AsyncWrite for Pipe {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        self.poll_write_internal(cx, buf)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<Result<usize, io::Error>> {
        self.poll_write_vectored_internal(cx, bufs)
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        self.close_write();
        Poll::Ready(Ok(()))
    }

    fn is_write_vectored(&self) -> bool {
        true
    }
}

/// Creates a unidirectional in-memory pipe with the specified maximum buffer size.
/// Returns the readable and writable halves of the pipe.
/// Non-thread-safe equivalent of [`tokio::io::simplex`](https://docs.rs/tokio/latest/tokio/io/fn.simplex.html).
pub fn pipe(max_buf_size: usize) -> (PipeReader, PipeWriter) {
    let pipe = Rc::new(UnsafeCell::new(Pipe::new(max_buf_size)));
    (PipeReader(pipe.clone()), PipeWriter(pipe))
}

/// The readable half of a value returned from [`pipe`].
pub struct PipeReader(Rc<UnsafeCell<Pipe>>);

/// The writable half of a value returned from [`pipe`].
pub struct PipeWriter(Rc<UnsafeCell<Pipe>>);

impl AsyncRead for PipeReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        // SAFETY: exclusive access is guaranteed by the single-threaded context
        unsafe { self.0.with_unchecked(|pipe| Pin::new(pipe).poll_read(cx, buf)) }
    }
}

impl Drop for PipeReader {
    fn drop(&mut self) {
        // SAFETY: exclusive access is guaranteed by the single-threaded context
        unsafe { self.0.with_unchecked(|pipe| pipe.close_read()) }
    }
}

impl AsyncWrite for PipeWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        // SAFETY: exclusive access is guaranteed by the single-threaded context
        unsafe { self.0.with_unchecked(|pipe| Pin::new(pipe).poll_write(cx, buf)) }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        // SAFETY: exclusive access is guaranteed by the single-threaded context
        unsafe { self.0.with_unchecked(|pipe| Pin::new(pipe).poll_flush(cx)) }
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        // SAFETY: exclusive access is guaranteed by the single-threaded context
        unsafe { self.0.with_unchecked(|pipe| Pin::new(pipe).poll_shutdown(cx)) }
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<Result<usize, io::Error>> {
        // SAFETY: exclusive access is guaranteed by the single-threaded context
        unsafe { self.0.with_unchecked(|pipe| Pin::new(pipe).poll_write_vectored(cx, bufs)) }
    }

    fn is_write_vectored(&self) -> bool {
        true
    }
}

impl Drop for PipeWriter {
    fn drop(&mut self) {
        // SAFETY: exclusive access is guaranteed by the single-threaded context
        unsafe { self.0.with_unchecked(|pipe| pipe.close_write()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_test::{assert_pending, assert_ready, task::spawn};

    #[test]
    fn test_write_then_read() {
        let (mut reader, mut writer) = pipe(1024);

        let data = b"Hello, world!";
        let mut write_task = spawn(writer.write_all(data));
        let write_ret = assert_ready!(write_task.poll());
        assert!(write_ret.is_ok());
        drop(write_task);

        let mut buf = Vec::new();
        let mut read_task = spawn(reader.read_buf(&mut buf));
        let read_ret = assert_ready!(read_task.poll());
        assert!(read_ret.is_ok());
        drop(read_task);
        assert_eq!(&buf[..], data);
    }

    #[test]
    fn test_reader_notifies_writer() {
        let (mut reader, mut writer) = pipe(7);

        let data = b"Hello, world!";
        let mut write_task = spawn(writer.write_all(data));
        assert_pending!(write_task.poll());

        let mut buf = Vec::new();
        let mut read_task = spawn(reader.read_buf(&mut buf));
        let read_ret = assert_ready!(read_task.poll());
        assert!(read_ret.is_ok());
        drop(read_task);
        assert_eq!(&buf[..], b"Hello, ");
        assert!(write_task.is_woken());

        let write_ret = assert_ready!(write_task.poll());
        assert!(write_ret.is_ok());
        drop(write_task);

        let mut read_task = spawn(reader.read_buf(&mut buf));
        let read_ret = assert_ready!(read_task.poll());
        assert!(read_ret.is_ok());
        drop(read_task);
        assert_eq!(&buf[..], data);
    }

    #[test]
    fn test_writer_notifies_reader() {
        let (mut reader, mut writer) = pipe(1024);

        let mut buf = Vec::new();
        let mut read_task = spawn(reader.read_buf(&mut buf));
        assert_pending!(read_task.poll());

        let data = b"Hello, world!";
        let mut write_task = spawn(writer.write_all(data));
        let write_ret = assert_ready!(write_task.poll());
        assert!(write_ret.is_ok());
        drop(write_task);
        assert!(read_task.is_woken());

        let read_ret = assert_ready!(read_task.poll());
        assert!(read_ret.is_ok());
        drop(read_task);
        assert_eq!(&buf[..], data);
    }

    #[test]
    fn test_drop_writer() {
        let (mut reader, writer) = pipe(1024);

        drop(writer);
        let mut buf = Vec::new();
        let mut read_eof_task = spawn(reader.read_to_end(&mut buf));
        let read_eof_ret = assert_ready!(read_eof_task.poll());
        assert!(read_eof_ret.is_ok());
        drop(read_eof_task);
        assert!(buf.is_empty());
    }

    #[test]
    fn test_drop_writer_notify_reader() {
        let (mut reader, writer) = pipe(1024);

        let mut buf = Vec::new();
        let mut read_task = spawn(reader.read_buf(&mut buf));
        assert_pending!(read_task.poll());

        drop(writer);
        assert!(read_task.is_woken());

        let read_ret = assert_ready!(read_task.poll());
        assert!(read_ret.is_ok());
        assert!(buf.is_empty());
    }

    #[test]
    fn test_drop_reader() {
        let (reader, mut writer) = pipe(1024);

        drop(reader);
        let data = b"Hello, world!";
        let mut write_task = spawn(writer.write_all(data));
        let write_ret = assert_ready!(write_task.poll());
        let err = write_ret.err().unwrap();
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    }

    #[test]
    fn test_drop_reader_notify_writer() {
        let (reader, mut writer) = pipe(5);

        let data = b"Hello, world!";
        let mut write_task = spawn(writer.write_all(data));
        assert_pending!(write_task.poll());

        drop(reader);
        assert!(write_task.is_woken());

        let write_ret = assert_ready!(write_task.poll());
        let err = write_ret.err().unwrap();
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    }
}
