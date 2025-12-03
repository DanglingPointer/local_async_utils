use crate::shared::UnsafeShared;
use std::cell::UnsafeCell;
use std::io::BufRead;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};
use std::{cmp, io};
use std::{collections::VecDeque, pin::Pin};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// Unidirectional in-memory pipe implementing `AsyncRead` and `AsyncWrite`.
/// A more efficient version of [`tokio::io::SimplexStream`](https://docs.rs/tokio/latest/tokio/io/struct.SimplexStream.html)
/// optimized for single-threaded use cases.
#[derive(Debug)]
pub struct Pipe {
    buffer: VecDeque<u8>,
    is_closed: bool,
    max_buf_size: usize,
    read_waker: Option<Waker>,
    write_waker: Option<Waker>,
}

impl Pipe {
    /// Create a new `Pipe` with a fixed-size pre-allocated buffer of `max_buf_size` bytes.
    pub fn new(max_buf_size: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(max_buf_size),
            is_closed: false,
            max_buf_size,
            read_waker: None,
            write_waker: None,
        }
    }

    /// Split the pipe into non-[`Send`] owned readable and writable ends.
    pub fn into_split(self) -> (ReadEnd, WriteEnd) {
        let pipe = Rc::new(UnsafeCell::new(self));
        (ReadEnd(pipe.clone()), WriteEnd(pipe))
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
                self.buffer.consume(bytes_copied);
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

/// The readable end of a [`Pipe`]. Not thread-safe.
pub struct ReadEnd(Rc<UnsafeCell<Pipe>>);

/// The writable end of a [`Pipe`]. Not thread-safe.
pub struct WriteEnd(Rc<UnsafeCell<Pipe>>);

impl AsyncRead for ReadEnd {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        // SAFETY: exclusive access is guaranteed by the single-threaded context
        unsafe { self.0.with_unchecked(|pipe| Pin::new(pipe).poll_read(cx, buf)) }
    }
}

impl Drop for ReadEnd {
    fn drop(&mut self) {
        // SAFETY: exclusive access is guaranteed by the single-threaded context
        unsafe { self.0.with_unchecked(|pipe| pipe.close_read()) }
    }
}

impl AsyncWrite for WriteEnd {
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

impl Drop for WriteEnd {
    fn drop(&mut self) {
        // SAFETY: exclusive access is guaranteed by the single-threaded context
        unsafe { self.0.with_unchecked(|pipe| pipe.close_write()) }
    }
}

/// Create a bi-directional in-memory stream of bytes using two [`Pipe`]s in opposite directions.
/// Non-thread-safe equivalent of [`tokio::io::duplex`](https://docs.rs/tokio/latest/tokio/io/fn.duplex.html).
/// # Returns
/// A tuple containing two connected [`DuplexEnd`]s. Each end can be used for both reading and writing.
/// Data written to one end can be read from the other end and vice versa.
pub fn duplex_pipe(max_buf_size: usize) -> (DuplexEnd, DuplexEnd) {
    let (read1, write1) = Pipe::new(max_buf_size).into_split();
    let (read2, write2) = Pipe::new(max_buf_size).into_split();
    (DuplexEnd(read1, write2), DuplexEnd(read2, write1))
}

/// Bidirectional in-memory stream of bytes implementing `AsyncRead` and `AsyncWrite`.
/// Non-thread-safe equivalent of [`tokio::io::DuplexStream`](https://docs.rs/tokio/latest/tokio/io/struct.DuplexStream.html).
pub struct DuplexEnd(ReadEnd, WriteEnd);

impl DuplexEnd {
    /// Splits the [`DuplexEnd`] into owned readable and writable halves.
    pub fn into_split(self) -> (ReadEnd, WriteEnd) {
        let DuplexEnd(read, write) = self;
        (read, write)
    }

    /// Splits the [`DuplexEnd`] into mutable references to the readable and writable halves.
    pub fn split(&mut self) -> (&mut ReadEnd, &mut WriteEnd) {
        let DuplexEnd(read, write) = self;
        (read, write)
    }
}

impl AsyncRead for DuplexEnd {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let DuplexEnd(read, _write) = self.get_mut();
        Pin::new(read).poll_read(cx, buf)
    }
}

impl AsyncWrite for DuplexEnd {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        let DuplexEnd(_read, write) = self.get_mut();
        Pin::new(write).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        let DuplexEnd(_read, write) = self.get_mut();
        Pin::new(write).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        let DuplexEnd(_read, write) = self.get_mut();
        Pin::new(write).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_test::{assert_pending, assert_ready, task::spawn};

    #[test]
    fn test_write_then_read() {
        let (mut reader, mut writer) = Pipe::new(1024).into_split();

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
        let (mut reader, mut writer) = Pipe::new(7).into_split();

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
        let (mut reader, mut writer) = Pipe::new(1024).into_split();

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
    fn test_partial_read() {
        let (mut reader, mut writer) = Pipe::new(1024).into_split();

        let data = b"Hello, world!";
        let mut write_task = spawn(writer.write_all(data));
        let write_ret = assert_ready!(write_task.poll());
        assert!(write_ret.is_ok());
        drop(write_task);

        let mut buf = [0u8; 7];

        let mut read_task = spawn(reader.read_exact(&mut buf));
        let read_ret = assert_ready!(read_task.poll());
        assert!(read_ret.is_ok());
        drop(read_task);
        assert_eq!(&buf[..], b"Hello, ");

        let mut buf_ref = &mut buf[..];
        let mut read_task = spawn(reader.read_buf(&mut buf_ref));
        let read_ret = assert_ready!(read_task.poll());
        assert!(read_ret.is_ok());
        assert_eq!(&buf[..], b"world! ");
    }

    #[test]
    fn test_drop_writer() {
        let (mut reader, mut writer) = Pipe::new(1024).into_split();
        assert_ready!(spawn(writer.write_all(b"Hello, world!")).poll()).unwrap();

        drop(writer);
        let mut buf = Vec::new();
        let mut read_eof_task = spawn(reader.read_to_end(&mut buf));
        let read_eof_ret = assert_ready!(read_eof_task.poll());
        assert!(read_eof_ret.is_ok());
        drop(read_eof_task);
        assert_eq!(&buf[..], b"Hello, world!");
    }

    #[test]
    fn test_drop_writer_notify_reader() {
        let (mut reader, writer) = Pipe::new(1024).into_split();

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
        let (reader, mut writer) = Pipe::new(1024).into_split();

        drop(reader);
        let data = b"Hello, world!";
        let mut write_task = spawn(writer.write_all(data));
        let write_ret = assert_ready!(write_task.poll());
        let err = write_ret.err().unwrap();
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    }

    #[test]
    fn test_drop_reader_notify_writer() {
        let (reader, mut writer) = Pipe::new(5).into_split();

        let data = b"Hello, world!";
        let mut write_task = spawn(writer.write_all(data));
        assert_pending!(write_task.poll());

        drop(reader);
        assert!(write_task.is_woken());

        let write_ret = assert_ready!(write_task.poll());
        let err = write_ret.err().unwrap();
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    }

    #[test]
    fn test_non_contiguous_internal_buffer() {
        let (mut reader, mut writer) = Pipe::new(4).into_split();

        assert_ready!(spawn(writer.write_all(b"1234")).poll()).unwrap();

        let mut buf = [0u8; 2];
        assert_ready!(spawn(reader.read_exact(&mut buf)).poll()).unwrap();
        assert_eq!(&buf[..], b"12");

        assert_ready!(spawn(writer.write_all(b"56")).poll()).unwrap();

        unsafe {
            reader.0.with_unchecked(|pipe| {
                let (head, tail) = pipe.buffer.as_slices();
                assert!(!head.is_empty());
                assert!(!tail.is_empty());
            });
        }

        let mut buf = Vec::new();
        let read_ret = assert_ready!(spawn(reader.read_buf(&mut buf)).poll());
        assert!(read_ret.is_ok());
        assert_eq!(&buf[..], b"3456");
    }

    #[test]
    fn test_duplex_pipe() {
        let (mut stream1, mut stream2) = duplex_pipe(1024);

        let data = b"Hello, world!";
        let mut write_task = spawn(stream1.write_all(data));
        let write_ret = assert_ready!(write_task.poll());
        assert!(write_ret.is_ok());
        drop(write_task);

        assert_pending!(spawn(stream1.read_u8()).poll());

        let mut buf = Vec::new();
        let mut read_task = spawn(stream2.read_buf(&mut buf));
        let read_ret = assert_ready!(read_task.poll());
        assert!(read_ret.is_ok());
        drop(read_task);
        assert_eq!(&buf[..], data);

        let data = b"Goodbye, world!";
        let mut write_task = spawn(stream2.write_all(data));
        let write_ret = assert_ready!(write_task.poll());
        assert!(write_ret.is_ok());
        drop(write_task);

        assert_pending!(spawn(stream2.read_u8()).poll());

        let mut buf = Vec::new();
        let mut read_task = spawn(stream1.read_buf(&mut buf));
        let read_ret = assert_ready!(read_task.poll());
        assert!(read_ret.is_ok());
        drop(read_task);
        assert_eq!(&buf[..], data);
    }
}
