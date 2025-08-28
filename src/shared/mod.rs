pub mod local_shared;
pub mod projected_shared;

use std::cell::UnsafeCell;
use std::sync::{Arc, Mutex};
use std::{cell::RefCell, rc::Rc};

pub use local_shared::LocalShared;
pub use projected_shared::ProjectedShared;

pub trait Shared: Clone {
    type Target;

    fn with<R, F>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut Self::Target) -> R;

    fn project<To, Proj>(&self, f: Proj) -> ProjectedShared<Self, Proj>
    where
        Proj: Fn(&mut Self::Target) -> &mut To + Clone,
    {
        ProjectedShared {
            inner: self.clone(),
            proj_fn: f,
        }
    }
}

impl<T> Shared for Rc<RefCell<T>> {
    type Target = T;

    #[inline(always)]
    fn with<R, F>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut Self::Target) -> R,
    {
        f(&mut self.borrow_mut())
    }
}

impl<T> Shared for Arc<Mutex<T>> {
    type Target = T;

    #[inline(always)]
    fn with<R, F>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut Self::Target) -> R,
    {
        f(&mut self.lock().unwrap())
    }
}

pub trait UnsafeShared: Clone {
    type Target;

    fn with<R, F>(&mut self, f: F) -> R
    where
        F: FnOnce(*mut Self::Target) -> R;

    /// # Safety
    /// Calls to `with_unchecked()` can't be nested inside each other.
    #[inline(always)]
    unsafe fn with_unchecked<R, F>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut Self::Target) -> R,
    {
        self.with(move |t_ptr| f(unsafe { &mut *t_ptr }))
    }

    fn project<To, Proj>(&self, f: Proj) -> ProjectedShared<Self, Proj>
    where
        Proj: Fn(*mut Self::Target) -> *mut To + Clone,
    {
        ProjectedShared {
            inner: self.clone(),
            proj_fn: f,
        }
    }
}

impl<T> UnsafeShared for Rc<UnsafeCell<T>> {
    type Target = T;

    #[inline(always)]
    fn with<R, F>(&mut self, f: F) -> R
    where
        F: FnOnce(*mut Self::Target) -> R,
    {
        f(self.get())
    }
}
