use super::{Shared, UnsafeShared};
use std::cell::{RefCell, UnsafeCell};
use std::rc::Rc;

pub struct LocalShared<T>(Rc<RefCell<T>>);

impl<T> LocalShared<T> {
    pub fn new(inner: T) -> Self {
        Self(Rc::new(RefCell::new(inner)))
    }
}

impl<T> Shared for LocalShared<T> {
    type Target = T;

    #[inline(always)]
    fn with<R, F>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        self.0.with(f)
    }
}

impl<T> Clone for LocalShared<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

pub struct LocalUnsafeShared<T>(Rc<UnsafeCell<T>>);

impl<T> UnsafeShared for LocalUnsafeShared<T> {
    type Target = T;

    #[inline(always)]
    fn with<R, F>(&mut self, f: F) -> R
    where
        F: FnOnce(*mut Self::Target) -> R,
    {
        self.0.with(f)
    }
}

impl<T> Clone for LocalUnsafeShared<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
