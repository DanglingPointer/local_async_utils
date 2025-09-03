use crate::shared::UnsafeShared;
use futures::Stream;
use std::cell::UnsafeCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};
use std::{fmt, io};
use std::{future::poll_fn, pin::Pin};

struct State<T> {
    queue: VecDeque<T>,
    tx_waker: Option<Waker>,
    rx_waker: Option<Waker>,
    has_tx: bool,
    has_rx: bool,
}

fn replace_waker(old_waker: &mut Option<Waker>, cx: &mut Context) {
    if old_waker.as_ref().is_none_or(|w| !w.will_wake(cx.waker())) {
        old_waker.replace(cx.waker().clone());
    }
}

fn take_and_wake(waker: &mut Option<Waker>) {
    waker.take().inspect(Waker::wake_by_ref);
}

#[derive(PartialEq, Eq)]
pub enum TrySendError<T> {
    Full(T),
    Closed(T),
}

#[derive(PartialEq, Eq)]
pub enum SendError<T> {
    Closed(T),
}

/// Bounded SPSC channel
pub fn channel<T>(limit: usize) -> (Sender<T>, Receiver<T>) {
    let shared = Rc::new(UnsafeCell::new(State {
        queue: VecDeque::with_capacity(limit),
        tx_waker: None,
        rx_waker: None,
        has_tx: true,
        has_rx: true,
    }));
    (Sender(shared.clone()), Receiver(shared))
}

pub struct Sender<T>(Rc<UnsafeCell<State<T>>>);

impl<T> Sender<T> {
    pub async fn send(&mut self, item: T) -> Result<(), SendError<T>> {
        let can_send = poll_fn(|cx| self.poll_ready(cx)).await;
        if can_send {
            unsafe {
                self.0.with_unchecked(|state| {
                    state.queue.push_back(item);
                    take_and_wake(&mut state.rx_waker);
                })
            }
            Ok(())
        } else {
            Err(SendError::Closed(item))
        }
    }

    pub async fn closed(&mut self) {
        poll_fn(|cx| self.poll_closed(cx)).await
    }

    pub fn try_send(&mut self, item: T) -> Result<(), TrySendError<T>> {
        unsafe {
            self.0.with_unchecked(|state| {
                if !state.has_rx {
                    Err(TrySendError::Closed(item))
                } else if state.queue.len() < state.queue.capacity() {
                    state.queue.push_back(item);
                    take_and_wake(&mut state.rx_waker);
                    Ok(())
                } else {
                    Err(TrySendError::Full(item))
                }
            })
        }
    }

    pub fn is_closed(&self) -> bool {
        unsafe { !(&*self.0.get()).has_rx }
    }

    fn poll_ready(&mut self, cx: &mut Context) -> Poll<bool> {
        unsafe {
            self.0.with_unchecked(|state| {
                if !state.has_rx {
                    Poll::Ready(false)
                } else if state.queue.len() < state.queue.capacity() {
                    Poll::Ready(true)
                } else {
                    replace_waker(&mut state.tx_waker, cx);
                    Poll::Pending
                }
            })
        }
    }

    fn poll_closed(&mut self, cx: &mut Context) -> Poll<()> {
        unsafe {
            self.0.with_unchecked(|state| {
                if !state.has_rx {
                    Poll::Ready(())
                } else {
                    replace_waker(&mut state.tx_waker, cx);
                    Poll::Pending
                }
            })
        }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        unsafe {
            self.0.with_unchecked(|state| {
                state.has_tx = false;
                state.tx_waker = None;
                take_and_wake(&mut state.rx_waker);
            })
        }
    }
}

pub struct Receiver<T>(Rc<UnsafeCell<State<T>>>);

impl<T> Receiver<T> {
    pub fn is_closed(&self) -> bool {
        unsafe { !(&*self.0.get()).has_tx }
    }
}

impl<T> Stream for Receiver<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        unsafe {
            self.0.with_unchecked(|state| {
                if let Some(item) = state.queue.pop_front() {
                    take_and_wake(&mut state.tx_waker);
                    Poll::Ready(Some(item))
                } else if !state.has_tx {
                    Poll::Ready(None)
                } else {
                    replace_waker(&mut state.rx_waker, cx);
                    Poll::Pending
                }
            })
        }
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        unsafe {
            self.0.with_unchecked(|state| {
                state.has_rx = false;
                take_and_wake(&mut state.tx_waker);
            })
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use static_assertions::assert_not_impl_any;
    use std::sync::Arc;
    use tokio_test::task::spawn;
    use tokio_test::{assert_pending, assert_ready};

    #[test]
    fn test_channel_static_properties() {
        assert_not_impl_any!(Arc<Sender<usize>>: std::marker::Send, Sync);
        assert_not_impl_any!(Arc<Receiver<usize>>: std::marker::Send, Sync);
        assert_not_impl_any!(Sender<usize>: std::marker::Send, Sync, Clone);
        assert_not_impl_any!(Receiver<usize>: std::marker::Send, Sync, Clone);
    }

    #[test]
    fn test_sender_notifies_receiver() {
        let (mut sender, receiver) = channel::<i32>(2);

        let mut receiver = spawn(receiver);
        assert_pending!(receiver.poll_next());

        assert_eq!(Ok(()), assert_ready!(spawn(sender.send(42)).poll()));
        assert!(receiver.is_woken());
        assert_eq!(Some(42), assert_ready!(receiver.poll_next()));
        assert_pending!(receiver.poll_next());

        drop(sender);
        assert!(receiver.is_woken());
        assert_eq!(None, assert_ready!(receiver.poll_next()));
        assert!(receiver.is_closed());
    }

    #[test]
    fn test_receiver_notifies_sender() {
        let (mut sender, receiver) = channel::<i32>(1);

        let mut receiver = spawn(receiver);
        assert_pending!(receiver.poll_next());

        assert_eq!(Ok(()), assert_ready!(spawn(sender.send(41)).poll()));
        let mut send = spawn(sender.send(42));
        assert_pending!(send.poll());

        assert!(receiver.is_woken());
        assert_eq!(Some(41), assert_ready!(receiver.poll_next()));
        assert_pending!(receiver.poll_next());

        assert!(send.is_woken());
        assert_eq!(Ok(()), assert_ready!(send.poll()));
        drop(send);

        assert!(receiver.is_woken());
        assert_eq!(Some(42), assert_ready!(receiver.poll_next()));

        assert_eq!(Ok(()), assert_ready!(spawn(sender.send(43)).poll()));
        let mut send = spawn(sender.send(44));
        assert_pending!(send.poll());

        drop(receiver);
        assert!(send.is_woken());
        assert_eq!(Err(SendError::Closed(44)), assert_ready!(send.poll()));
        drop(send);
        assert!(sender.is_closed());
    }
}
