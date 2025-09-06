use std::cell::UnsafeCell;
use std::ops::{ControlFlow, Deref};
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

pub(super) unsafe fn replace_waker(waker: &UnsafeCell<Option<Waker>>, cx: &mut Context) {
    let waker = unsafe { &mut *waker.get() };
    if waker.as_ref().is_none_or(|w| !w.will_wake(cx.waker())) {
        waker.replace(cx.waker().clone());
    }
}

pub(super) unsafe fn take_and_wake(waker: &UnsafeCell<Option<Waker>>) {
    let waker = unsafe { &mut *waker.get() };
    waker.take().inspect(Waker::wake_by_ref);
}

pub(super) trait Source {
    type Item;
    fn try_yield_one(&self) -> ControlFlow<Option<Self::Item>>;
}

pub(super) struct SharedState<T> {
    waker: UnsafeCell<Option<Waker>>,
    inner: T,
}

impl<T: Source> SharedState<T> {
    pub(super) fn new(inner: T) -> Rc<Self> {
        Rc::new(Self {
            waker: UnsafeCell::new(None),
            inner,
        })
    }

    pub(super) fn notify(&self) {
        unsafe { take_and_wake(&self.waker) }
    }

    pub(super) fn receiver_dropped(&self) {
        // remove waker so that we don't unnecessarily wake anyone when Sender is dropped
        let waker_mut = unsafe { &mut *self.waker.get() };
        waker_mut.take();
    }

    // This should NEVER be called concurrently from different futures/tasks,
    // because we store only 1 waker
    pub(super) fn poll_wait(self: &mut Rc<Self>, cx: &mut Context<'_>) -> Poll<Option<T::Item>> {
        if let ControlFlow::Break(output) = self.inner.try_yield_one() {
            Poll::Ready(output)
        } else {
            unsafe { replace_waker(&self.waker, cx) }
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
