use super::utils::UnsafeWrapper;
use std::borrow::Borrow;
use std::collections::{HashSet, hash_set};
use std::fmt;
use std::hash::Hash;

/// Unordered set that never leaks references to its content
pub struct Set<T>(UnsafeWrapper<HashSet<T>>);

impl<T: Eq + Hash> Set<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self(UnsafeWrapper::new(HashSet::with_capacity(capacity)))
    }

    pub fn contains<Q>(&self, value: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        // SAFETY: `with()` is never invoked recursively
        unsafe { self.0.with(|inner| inner.contains(value)) }
    }

    pub fn insert(&self, value: T) -> bool {
        // SAFETY: `with()` is never invoked recursively
        unsafe { self.0.with(|inner| inner.insert(value)) }
    }

    pub fn remove<Q>(&self, value: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        // SAFETY: `with()` is never invoked recursively
        unsafe { self.0.with(|inner| inner.remove(value)) }
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
        // SAFETY: `with()` is never invoked recursively
        unsafe { self.0.with(|inner| inner.is_empty()) }
    }

    pub fn into_inner(self) -> HashSet<T> {
        self.0.into_inner()
    }
}

impl<T> From<HashSet<T>> for Set<T> {
    fn from(hash_set: HashSet<T>) -> Self {
        Self(UnsafeWrapper::new(hash_set))
    }
}

impl<T: fmt::Debug> fmt::Debug for Set<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // SAFETY: `with()` is never invoked recursively
        unsafe { self.0.with(|inner| inner.fmt(f)) }
    }
}

impl<T> Default for Set<T> {
    fn default() -> Self {
        Self(UnsafeWrapper::new(HashSet::default()))
    }
}

impl<T: Clone> Clone for Set<T> {
    fn clone(&self) -> Self {
        // SAFETY: `with()` is never invoked recursively
        unsafe { self.0.with(|inner| Self(UnsafeWrapper::new(inner.clone()))) }
    }
}

impl<T> IntoIterator for Set<T> {
    type Item = T;
    type IntoIter = hash_set::IntoIter<T>;

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
    fn test_set_is_send_but_not_sync() {
        assert_impl_all!(Set<usize>: std::marker::Send);
        assert_not_impl_any!(Set<Rc<usize>>: std::marker::Send);
        assert_not_impl_any!(Set<Arc<usize>>: Sync);
        assert_not_impl_any!(Arc<Set<usize>>: std::marker::Send, Sync);
    }
}
