use std::cell::UnsafeCell;

/// A (hopefully) zero-cost wrapper that simplifies working with unsafe code.
pub struct UnsafeWrapper<T>(UnsafeCell<T>);

impl<T> UnsafeWrapper<T> {
    pub fn new(inner: T) -> Self {
        Self(UnsafeCell::new(inner))
    }

    /// # Safety
    /// Calls to `with()` can't be nested.
    #[inline(always)]
    pub unsafe fn with<R, F>(&self, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        f(unsafe { &mut *self.0.get() })
    }

    pub fn into_inner(self) -> T {
        self.0.into_inner()
    }
}
