use super::shared_state::{SharedState, Source};
use crate::sealed;
use crate::sync::error::SendError;
use std::cell::Cell;
use std::ops::ControlFlow;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

struct Data<T> {
    queue: sealed::Queue<T>,
    sender_count: Cell<usize>,
    has_receiver: Cell<bool>,
}

impl<T> Source for Data<T> {
    type Item = T;

    fn try_yield_one(&self) -> ControlFlow<Option<Self::Item>> {
        if let Some(item) = self.queue.pop() {
            ControlFlow::Break(Some(item))
        } else if self.sender_count.get() == 0 {
            ControlFlow::Break(None)
        } else {
            ControlFlow::Continue(())
        }
    }
}

type StateRc<T> = Rc<SharedState<Data<T>>>;

pub struct Sender<T>(StateRc<T>);

pub struct Receiver<T>(StateRc<T>);

/// Unbounded MPSC channel
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let state = SharedState::new(Data {
        queue: Default::default(),
        sender_count: Cell::new(1),
        has_receiver: Cell::new(true),
    });
    (Sender(state.clone()), Receiver(state))
}

impl<T> Sender<T> {
    pub fn is_closed(&self) -> bool {
        !self.0.has_receiver.get()
    }

    pub fn send(&self, item: T) -> Result<(), SendError<T>> {
        if self.is_closed() {
            Err(SendError::Closed(item))
        } else {
            self.0.queue.push(item);
            self.0.notify();
            Ok(())
        }
    }

    pub fn queue(&self) -> &sealed::Queue<T> {
        &self.0.queue
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let prev_count = self.0.sender_count.get();
        self.0.sender_count.set(prev_count - 1);
        self.0.notify();
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        let prev_count = self.0.sender_count.get();
        self.0.sender_count.set(prev_count + 1);
        Self(self.0.clone())
    }
}

impl<T> Receiver<T> {
    pub fn is_closed(&self) -> bool {
        self.0.sender_count.get() == 0
    }

    pub fn queue(&self) -> &sealed::Queue<T> {
        &self.0.queue
    }
}

impl<T> futures::Stream for Receiver<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.get_mut().0.poll_wait(cx)
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        self.0.receiver_dropped();
        self.0.has_receiver.set(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use static_assertions::*;
    use std::sync::Arc;
    use tokio_test::task::spawn;
    use tokio_test::{assert_pending, assert_ready};

    #[test]
    fn test_channel_static_properties() {
        assert_not_impl_any!(Arc<Sender<usize>>: std::marker::Send, Sync);
        assert_not_impl_any!(Arc<Receiver<usize>>: std::marker::Send, Sync);
        assert_not_impl_any!(Sender<usize>: std::marker::Send, Sync);
        assert_not_impl_any!(Receiver<usize>: std::marker::Send, Sync);
    }

    #[test]
    fn test_sender_notifies_receiver() {
        let (sender, receiver) = channel::<i32>();

        let mut receiver = spawn(receiver);
        assert_pending!(receiver.poll_next());

        sender.send(42).unwrap();
        assert!(receiver.is_woken());
        assert_eq!(Some(42), assert_ready!(receiver.poll_next()));
        assert_pending!(receiver.poll_next());

        drop(sender);
        assert!(receiver.is_woken());
        assert_eq!(None, assert_ready!(receiver.poll_next()));
    }

    #[test]
    fn test_receiver_drains_queue_after_sender_dies() {
        let (sender, receiver) = channel::<i32>();

        for i in 0..42 {
            sender.send(i).unwrap();
        }
        drop(sender);

        let mut receiver = spawn(receiver);
        for i in 0..42 {
            let received = assert_ready!(receiver.poll_next());
            assert_eq!(Some(i), received);
        }
        assert_eq!(None, assert_ready!(receiver.poll_next()));
    }

    #[test]
    fn test_sender_is_closed() {
        let (sender, receiver) = channel::<i32>();
        assert!(!sender.is_closed());

        drop(receiver);
        assert!(sender.is_closed());
    }

    #[test]
    fn test_receiver_is_closed() {
        let (sender, receiver) = channel::<i32>();
        assert!(!receiver.is_closed());

        let sender2 = sender.clone();
        assert!(!receiver.is_closed());

        drop(sender);
        assert!(!receiver.is_closed());

        drop(sender2);
        assert!(receiver.is_closed());
    }
}
