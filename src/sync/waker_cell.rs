use std::cell::UnsafeCell;
use std::task::{Context, Waker};

#[derive(Default)]
pub(super) struct WakerCell(UnsafeCell<Option<Waker>>);

impl WakerCell {
    pub(super) fn update(&self, cx: &mut Context) {
        let waker = unsafe { &mut *self.0.get() };
        if waker.as_ref().is_none_or(|w| !w.will_wake(cx.waker())) {
            waker.replace(cx.waker().clone());
        }
    }

    pub(super) fn take_and_wake(&self) {
        let waker = unsafe { &mut *self.0.get() };
        waker.take().inspect(Waker::wake_by_ref);
    }

    pub(super) fn reset(&self) {
        let waker = unsafe { &mut *self.0.get() };
        *waker = None;
    }
}
