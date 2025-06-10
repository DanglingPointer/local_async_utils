pub mod local_shared;
pub mod projected_shared;

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
