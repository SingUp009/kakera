//! Internal iteration helper. Uses rayon under the `parallel` feature,
//! plain iterators otherwise. Not part of the public API.

#[cfg(feature = "parallel")]
pub(crate) fn map_collect<T, U, F>(items: &[T], f: F) -> Vec<U>
where
    T: Sync,
    U: Send,
    F: Fn(&T) -> U + Sync + Send,
{
    use rayon::prelude::*;
    items.par_iter().map(f).collect()
}

#[cfg(not(feature = "parallel"))]
pub(crate) fn map_collect<T, U, F>(items: &[T], f: F) -> Vec<U>
where
    F: Fn(&T) -> U,
{
    items.iter().map(f).collect()
}
