use crate::sync::waker_cell::WakerCell;
use std::ops::{ControlFlow, Deref};
use std::rc::Rc;
use std::task::{Context, Poll};

pub(super) trait Source {
    type Item;
    fn try_yield_one(&self) -> ControlFlow<Option<Self::Item>>;
}

pub(super) struct SharedState<T> {
    waker: WakerCell,
    inner: T,
}

impl<T: Source> SharedState<T> {
    pub(super) fn new(inner: T) -> Rc<Self> {
        Rc::new(Self {
            waker: Default::default(),
            inner,
        })
    }

    pub(super) fn notify(&self) {
        self.waker.take_and_wake();
    }

    pub(super) fn receiver_dropped(&self) {
        // remove waker so that we don't unnecessarily wake anyone when Sender is dropped
        self.waker.reset();
    }

    // This should NEVER be called concurrently from different futures/tasks,
    // because we store only 1 waker
    pub(super) fn poll_wait(self: &mut Rc<Self>, cx: &mut Context<'_>) -> Poll<Option<T::Item>> {
        if let ControlFlow::Break(output) = self.inner.try_yield_one() {
            Poll::Ready(output)
        } else {
            self.waker.update(cx);
            Poll::Pending
        }
    }
}

impl<T> Deref for SharedState<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
