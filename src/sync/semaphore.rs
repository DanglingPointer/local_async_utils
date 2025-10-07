use super::shared_state::{SharedState, Source};
use futures::FutureExt;
use std::cell::Cell;
use std::fmt;
use std::future::{Future, poll_fn};
use std::ops::ControlFlow;
use std::rc::Rc;
use std::task::{Context, Poll};

struct MpscData {
    capacity: Cell<usize>,
    has_sender: Cell<bool>,
    #[cfg(debug_assertions)]
    has_receiver: Cell<bool>,
}

impl Source for MpscData {
    type Item = ();

    fn try_yield_one(&self) -> ControlFlow<Option<Self::Item>> {
        if !self.has_sender.get() {
            ControlFlow::Break(None)
        } else if self.capacity.get() > 0 {
            self.capacity.update(|cap| cap - 1);
            ControlFlow::Break(Some(()))
        } else {
            ControlFlow::Continue(())
        }
    }
}

type MpscStateRc = Rc<SharedState<MpscData>>;

#[derive(Clone)]
pub struct Sender(MpscStateRc);

pub struct Receiver(MpscStateRc);

pub fn mpsc_semaphore(initial_capacity: usize) -> (Sender, Receiver) {
    let state = SharedState::new(MpscData {
        capacity: Cell::new(initial_capacity),
        has_sender: Cell::new(true),
        #[cfg(debug_assertions)]
        has_receiver: Cell::new(true),
    });
    (Sender(state.clone()), Receiver(state))
}

impl Sender {
    pub fn signal_one(&self) {
        #[cfg(debug_assertions)]
        debug_assert!(self.0.has_receiver.get());
        let current_capacity = self.0.capacity.get();
        self.0.capacity.set(current_capacity + 1);
        self.0.notify();
    }
}

impl Drop for Sender {
    fn drop(&mut self) {
        self.0.has_sender.set(false);
        self.0.notify();
    }
}

impl Receiver {
    pub fn acquire_one(&mut self) -> impl Future<Output = bool> + '_ {
        poll_fn(|cx| self.0.poll_wait(cx)).map(|v| v.is_some())
    }

    pub fn drain(&mut self) -> usize {
        self.0.capacity.replace(0)
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        self.0.receiver_dropped();
        #[cfg(debug_assertions)]
        self.0.has_receiver.set(false);
    }
}

// ------------------------------------------------------------------------------------------------

struct SemData {
    capacity: Cell<usize>,
}

impl Source for SemData {
    type Item = ();

    fn try_yield_one(&self) -> ControlFlow<Option<Self::Item>> {
        if self.capacity.get() != 0 {
            self.capacity.update(|c| c - 1);
            ControlFlow::Break(Some(()))
        } else {
            ControlFlow::Continue(())
        }
    }
}

type SemStateRc = Rc<SharedState<SemData>>;

pub struct Permit(SemStateRc);

impl fmt::Debug for Permit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Permit").finish()
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        self.0.capacity.update(|c| c + 1);
        self.0.notify();
    }
}

pub struct Semaphore(SemStateRc);

impl Semaphore {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "zero capacity semaphore is not allowed");
        Self(SharedState::new(SemData {
            capacity: Cell::new(capacity),
        }))
    }

    pub async fn acquire_permit(&mut self) -> Permit {
        poll_fn(|cx| self.0.poll_wait(cx)).await;
        Permit(self.0.clone())
    }

    pub fn try_acquire_permit(&self) -> Option<Permit> {
        match self.0.try_yield_one() {
            ControlFlow::Break(Some(())) => Some(Permit(self.0.clone())),
            _ => None,
        }
    }

    pub fn poll_acquire_permit(&mut self, cx: &mut Context<'_>) -> Poll<Permit> {
        match self.0.poll_wait(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Some(())) => Poll::Ready(Permit(self.0.clone())),
            Poll::Ready(None) => unreachable!(),
        }
    }
}

impl fmt::Debug for Semaphore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Semaphore").field(&self.0.capacity).finish()
    }
}

impl Drop for Semaphore {
    fn drop(&mut self) {
        self.0.receiver_dropped();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_test::task::spawn;
    use tokio_test::{assert_pending, assert_ready};

    #[test]
    fn test_mpsc_semaphore() {
        let (notifier, mut waiter) = mpsc_semaphore(2);

        let ret = assert_ready!(spawn(waiter.acquire_one()).poll());
        assert!(ret);
        let ret = assert_ready!(spawn(waiter.acquire_one()).poll());
        assert!(ret);
        let mut wait_fut = spawn(waiter.acquire_one());
        assert_pending!(wait_fut.poll());

        notifier.signal_one();
        assert!(wait_fut.is_woken());

        let ret = assert_ready!(wait_fut.poll());
        assert!(ret);
        drop(wait_fut);
        let mut wait_fut = spawn(waiter.acquire_one());
        assert_pending!(wait_fut.poll());

        notifier.signal_one();
        notifier.signal_one();
        assert!(wait_fut.is_woken());

        let ret = assert_ready!(wait_fut.poll());
        assert!(ret);
        drop(wait_fut);
        let ret = assert_ready!(spawn(waiter.acquire_one()).poll());
        assert!(ret);

        let mut wait_fut = spawn(waiter.acquire_one());
        assert_pending!(wait_fut.poll());

        drop(notifier);
        assert!(wait_fut.is_woken());
        let ret = assert_ready!(wait_fut.poll());
        assert!(!ret);
    }

    #[test]
    fn test_mpsc_semaphore_ignores_capacity_when_notifier_dies() {
        let (notifier, mut waiter) = mpsc_semaphore(2);
        drop(notifier);

        let ret = assert_ready!(spawn(waiter.acquire_one()).poll());
        assert!(!ret);
    }

    #[test]
    fn test_mpsc_drain_semaphore() {
        let (notifier, mut waiter) = mpsc_semaphore(3);

        let ret = assert_ready!(spawn(waiter.acquire_one()).poll());
        assert!(ret);

        assert_eq!(2, waiter.drain());
        let mut wait_fut = spawn(waiter.acquire_one());
        assert_pending!(wait_fut.poll());

        notifier.signal_one();
        assert!(wait_fut.is_woken());
        let ret = assert_ready!(wait_fut.poll());
        assert!(ret);
    }

    #[test]
    fn test_semaphore() {
        let mut sem = Semaphore::new(2);

        // when
        let permit1 = assert_ready!(spawn(sem.acquire_permit()).poll());
        let permit2 = assert_ready!(spawn(sem.acquire_permit()).poll());

        // then
        assert!(sem.try_acquire_permit().is_none());
        let mut get_permit_fut = spawn(sem.acquire_permit());
        assert_pending!(get_permit_fut.poll());

        // when
        drop(permit1);

        // then
        let permit3 = assert_ready!(get_permit_fut.poll());
        drop(get_permit_fut);
        // and
        assert_pending!(spawn(sem.acquire_permit()).poll());
        assert!(sem.try_acquire_permit().is_none());

        // when
        drop(permit3);
        drop(permit2);

        // then
        let _permit4 = sem.try_acquire_permit().unwrap();
        let _permit5 = sem.try_acquire_permit().unwrap();
        // and
        assert_pending!(spawn(sem.acquire_permit()).poll());
        assert!(sem.try_acquire_permit().is_none());

        drop(sem);
    }
}
