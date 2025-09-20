pub mod local_shared;
pub mod projected_shared;

use std::cell::UnsafeCell;
use std::sync::{Arc, Mutex, PoisonError};
use std::{cell::RefCell, rc::Rc};

pub use local_shared::LocalShared;
pub use projected_shared::ProjectedShared;

/// An abstraction for accessing data shared between multiple tasks. In particular, this helps prevent
/// holding references to such data across suspension points.
pub trait Shared: Clone {
    type Target;

    /// Perform operations on the shared data.
    fn with<R, F>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut Self::Target) -> R;

    /// Get a `Shared` object for accessing part of `self`
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
        f(&mut self.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

/// An unsafe abstraction for accessing data shared between multiple tasks. In particular,
/// this helps prevent holding references to such data across suspension points.
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

    /// Get a `UnsafeShared` object for accessing part of `self`
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

/// Convenience macro for invoking [`Shared::with()`] method. Example usage:
/// ```
/// # use local_async_utils::prelude::*;
/// # use std::{cell::RefCell, rc::Rc};
/// let mut shared_data = Rc::new(RefCell::new(vec![1, 2, 3]));
/// define_with!(shared_data);
///
/// with!(|data| data.push(42));
///
/// let copy: Vec<_> = with!(|data| data.clone());
///
/// assert_eq!(copy, vec![1, 2, 3, 42]);
/// ```
#[macro_export]
macro_rules! define_with {
    ($shared:expr) => {
        macro_rules! with {
            ($f:expr) => {{
                use $crate::shared::Shared;
                $shared.with(
                    #[inline(always)]
                    $f,
                )
            }};
        }
    };
}

/// Convenience macro for invoking [`UnsafeShared::with_unchecked()`] method. Example usage:
/// ```
/// # use local_async_utils::prelude::*;
/// # use std::{cell::UnsafeCell, rc::Rc};
/// let mut shared_data = Rc::new(UnsafeCell::new(vec![1, 2, 3]));
/// define_with_unchecked!(shared_data);
///
/// unsafe { with_unchecked!(|data| data.push(42)) }
///
/// let copy: Vec<_> = unsafe { with_unchecked!(|data| data.clone()) };
///
/// assert_eq!(copy, vec![1, 2, 3, 42]);
/// ```
#[macro_export]
macro_rules! define_with_unchecked {
    ($shared:expr) => {
        macro_rules! with_unchecked {
            ($f:expr) => {{
                use $crate::shared::UnsafeShared;
                $shared.with_unchecked(
                    #[inline(always)]
                    $f,
                )
            }};
        }
    };
}
