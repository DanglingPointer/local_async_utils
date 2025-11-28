use super::utils::UnsafeWrapper;
use std::collections::{VecDeque, vec_deque};
use std::fmt;

/// FIFO queue that never leaks references to its content
pub struct Queue<T>(UnsafeWrapper<VecDeque<T>>);

impl<T> Queue<T> {
    pub fn new() -> Self {
        Self(UnsafeWrapper::new(VecDeque::new()))
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self(UnsafeWrapper::new(VecDeque::with_capacity(capacity)))
    }

    pub fn push(&self, item: T) {
        // SAFETY: `with()` is never invoked recursively
        unsafe { self.0.with(|inner| inner.push_back(item)) }
    }

    pub fn pop(&self) -> Option<T> {
        // SAFETY: `with()` is never invoked recursively
        unsafe { self.0.with(|inner| inner.pop_front()) }
    }

    pub fn contains(&self, item: &T) -> bool
    where
        T: PartialEq<T>,
    {
        // SAFETY: `with()` is never invoked recursively
        unsafe { self.0.with(|inner| inner.contains(item)) }
    }

    pub fn remove_all(&self, item: &T) -> bool
    where
        T: PartialEq<T>,
    {
        // SAFETY: `with()` is never invoked recursively
        unsafe {
            self.0.with(|inner| {
                let initial_len = inner.len();
                inner.retain(|e| e != item);
                inner.len() != initial_len
            })
        }
    }

    pub fn clear(&self) {
        // SAFETY: `with()` is never invoked recursively
        unsafe { self.0.with(|inner| inner.clear()) }
    }

    pub fn len(&self) -> usize {
        // SAFETY: `with()` is never invoked recursively
        unsafe { self.0.with(|inner| inner.len()) }
    }

    pub fn capacity(&self) -> usize {
        // SAFETY: `with()` is never invoked recursively
        unsafe { self.0.with(|inner| inner.capacity()) }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn into_inner(self) -> VecDeque<T> {
        self.0.into_inner()
    }
}

impl<T> From<VecDeque<T>> for Queue<T> {
    fn from(vec_deque: VecDeque<T>) -> Self {
        Self(UnsafeWrapper::new(vec_deque))
    }
}

impl<T: fmt::Debug> fmt::Debug for Queue<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // SAFETY: `with()` is never invoked recursively
        unsafe { self.0.with(|inner| inner.fmt(f)) }
    }
}

impl<T> Default for Queue<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone> Clone for Queue<T> {
    fn clone(&self) -> Self {
        // SAFETY: `with()` is never invoked recursively
        unsafe { self.0.with(|inner| Self(UnsafeWrapper::new(inner.clone()))) }
    }
}

impl<T> IntoIterator for Queue<T> {
    type Item = T;
    type IntoIter = vec_deque::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_inner().into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use static_assertions::{assert_impl_all, assert_not_impl_any};
    use std::{rc::Rc, sync::Arc};

    #[test]
    fn test_queue_is_send_but_not_sync() {
        assert_impl_all!(Queue<usize>: std::marker::Send);
        assert_not_impl_any!(Queue<Rc<usize>>: std::marker::Send);
        assert_not_impl_any!(Queue<Arc<usize>>: Sync);
        assert_not_impl_any!(Arc<Queue<usize>>: std::marker::Send, Sync);
    }
}
