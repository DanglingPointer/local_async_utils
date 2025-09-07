use crate::sealed;
use crate::sync::error::{SendError, TrySendError};
use crate::sync::waker_cell::WakerCell;
use futures::Stream;
use std::cell::Cell;
use std::rc::Rc;
use std::task::{Context, Poll};
use std::{future::poll_fn, pin::Pin};

struct State<T> {
    queue: sealed::Queue<T>,
    tx_waker: WakerCell,
    rx_waker: WakerCell,
    has_tx: Cell<bool>,
    has_rx: Cell<bool>,
    capacity: usize,
}

/// Bounded SPSC channel
pub fn channel<T>(limit: usize) -> (Sender<T>, Receiver<T>) {
    let shared = Rc::new(State {
        queue: sealed::Queue::with_capacity(limit),
        tx_waker: Default::default(),
        rx_waker: Default::default(),
        has_tx: Cell::new(true),
        has_rx: Cell::new(true),
        capacity: limit,
    });
    (Sender(shared.clone()), Receiver(shared))
}

pub struct Sender<T>(Rc<State<T>>);

impl<T> Sender<T> {
    pub async fn send(&mut self, item: T) -> Result<(), SendError<T>> {
        let can_send = poll_fn(|cx| self.poll_ready(cx)).await;
        if can_send {
            self.0.queue.push(item);
            self.0.rx_waker.take_and_wake();
            Ok(())
        } else {
            Err(SendError::Closed(item))
        }
    }

    pub async fn closed(&mut self) {
        poll_fn(|cx| self.poll_closed(cx)).await
    }

    pub fn try_send(&mut self, item: T) -> Result<(), TrySendError<T>> {
        if !self.0.has_rx.get() {
            Err(TrySendError::Closed(item))
        } else if self.0.queue.len() < self.0.capacity {
            self.0.queue.push(item);
            self.0.rx_waker.take_and_wake();
            Ok(())
        } else {
            Err(TrySendError::Full(item))
        }
    }

    pub fn is_closed(&self) -> bool {
        !self.0.has_rx.get()
    }

    pub fn queue(&self) -> &sealed::Queue<T> {
        &self.0.queue
    }

    fn poll_ready(&mut self, cx: &mut Context) -> Poll<bool> {
        if !self.0.has_rx.get() {
            Poll::Ready(false)
        } else if self.0.queue.len() < self.0.queue.capacity() {
            Poll::Ready(true)
        } else {
            self.0.tx_waker.update(cx);
            Poll::Pending
        }
    }

    fn poll_closed(&mut self, cx: &mut Context) -> Poll<()> {
        if !self.0.has_rx.get() {
            Poll::Ready(())
        } else {
            self.0.tx_waker.update(cx);
            Poll::Pending
        }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        self.0.has_tx.set(false);
        self.0.tx_waker.reset();
        self.0.rx_waker.take_and_wake();
    }
}

pub struct Receiver<T>(Rc<State<T>>);

impl<T> Receiver<T> {
    pub fn is_closed(&self) -> bool {
        !self.0.has_tx.get()
    }

    pub fn queue(&self) -> &sealed::Queue<T> {
        &self.0.queue
    }
}

impl<T> Stream for Receiver<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(item) = self.0.queue.pop() {
            self.0.tx_waker.take_and_wake();
            Poll::Ready(Some(item))
        } else if !self.0.has_tx.get() {
            Poll::Ready(None)
        } else {
            self.0.rx_waker.update(cx);
            Poll::Pending
        }
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        self.0.has_rx.set(false);
        self.0.tx_waker.take_and_wake();
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
