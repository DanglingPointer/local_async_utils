use super::Shared;

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
