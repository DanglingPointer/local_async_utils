use super::{Shared, UnsafeShared};

pub struct ProjectedShared<T, F> {
    pub(super) inner: T,
    pub(super) proj_fn: F,
}

impl<From, To, Inner, Proj> Shared for ProjectedShared<Inner, Proj>
where
    Inner: Shared<Target = From>,
    Proj: Fn(&mut From) -> &mut To + Clone,
{
    type Target = To;

    #[inline(always)]
    fn with<R, F>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut Self::Target) -> R,
    {
        let proj_fn = &self.proj_fn;
        self.inner.with(|from| f(proj_fn(from)))
    }
}

impl<T, F> Clone for ProjectedShared<T, F>
where
    T: Clone,
    F: Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            proj_fn: self.proj_fn.clone(),
        }
    }
}

impl<From, To, Inner, Proj> UnsafeShared for ProjectedShared<Inner, Proj>
where
    Inner: UnsafeShared<Target = From>,
    Proj: Fn(*mut From) -> *mut To + Clone,
{
    type Target = To;

    #[inline(always)]
    fn with<R, F>(&mut self, f: F) -> R
    where
        F: FnOnce(*mut Self::Target) -> R,
    {
        let proj_fn = &self.proj_fn;
        self.inner.with(|from| f(proj_fn(from)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::LocalShared;

    #[test]
    fn test_projected_shared() {
        let mut shared = LocalShared::new((1, 2));
        let mut projected = shared.project(|data| &mut data.0);

        projected.with(|data| {
            *data += 10;
        });

        let result = shared.with(|data| data.0);
        assert_eq!(result, 11);
    }
}
