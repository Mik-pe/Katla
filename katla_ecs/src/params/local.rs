//! Persistent state owned by one typed system.

use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

use super::{ParamAccess, ParamContext, SystemParam, sealed};
use crate::World;

/// Persistent mutable state isolated to one system.
pub struct Local<T>(PhantomData<T>);
/// One invocation's borrow of persistent system state.
pub struct LocalRef<'w, T>(&'w mut T);
impl<T> Deref for LocalRef<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.0
    }
}
impl<T> DerefMut for LocalRef<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.0
    }
}
impl<T: Default + Send + 'static> sealed::Sealed for Local<T> {}
impl<T: Default + Send + 'static> SystemParam for Local<T> {
    type State = T;
    type Item<'w> = LocalRef<'w, T>;
    fn access(_: &mut ParamAccess) {}
    fn init(_: &mut World) -> T {
        T::default()
    }
    unsafe fn prepare<'w>(_: &ParamContext<'w>, state: &'w mut T) -> Self::Item<'w> {
        LocalRef(state)
    }
}
