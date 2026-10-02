//! Sealed typed system parameters and deferred structural commands.

use crate::resource::ResourceStorage;
use crate::storage::ComponentStorageManager;
use crate::system::{ComponentAccess, ResourceAccess};
use crate::typed_query::{PreparedQuery, Query, QueryCache, QueryDescriptor, QueryView};
use crate::{Resource, World};

mod commands;
mod events;
mod local;
mod resources;

pub use commands::{CommandQueue, Commands};
pub use events::{EventCursor, EventRead, EventReader, EventWrite, EventWriter, Events};
pub use local::{Local, LocalRef};
pub use resources::{Res, ResMut, ResourceMut, ResourceRef};

mod sealed {
    pub trait Sealed {}
}

/// Derived access claims used when registering a typed system.
#[doc(hidden)]
#[derive(Default, Clone)]
pub struct ParamAccess {
    pub(crate) components: Vec<ComponentAccess>,
    pub(crate) resources: Vec<ResourceAccess>,
    commands: bool,
}

impl ParamAccess {
    pub(crate) fn add_components(&mut self, accesses: Vec<ComponentAccess>) {
        for access in accesses {
            assert!(
                !self
                    .components
                    .iter()
                    .any(|other| access.conflicts_with(*other)),
                "conflicting component parameters in the same system"
            );
            self.components.push(access);
        }
    }
    fn resource<R: Resource>(&mut self, write: bool) {
        let access = if write {
            ResourceAccess::write::<R>()
        } else {
            ResourceAccess::read::<R>()
        };
        assert!(
            !self
                .resources
                .iter()
                .any(|other| access.conflicts_with(*other)),
            "conflicting resource parameters in the same system"
        );
        self.resources.push(access);
    }
}

/// Owner-thread preparation context. No reference to it is sent to workers.
#[doc(hidden)]
pub struct ParamContext<'w> {
    pub(crate) storage: &'w ComponentStorageManager,
    pub(crate) resources: &'w ResourceStorage,
    pub(crate) epoch: u64,
}

pub(crate) type DeferredCommand = Box<dyn FnOnce(&mut World) + Send>;

/// A parameter descriptor whose access claims and preparation are engine-owned.
///
/// Parameters compose as tuples up to eight entries. Mutable aliases within a
/// tuple are rejected when the system is registered, before any borrow occurs.
pub trait SystemParam: sealed::Sealed + 'static {
    /// State belonging to a single registered system.
    type State: Send;
    /// Borrowed data passed to the system for one invocation.
    type Item<'w>: Send;
    /// Derives the parameter's access claims.
    #[doc(hidden)]
    fn access(access: &mut ParamAccess);
    /// Initializes persistent state on the caller thread.
    #[doc(hidden)]
    fn init(world: &mut World) -> Self::State;
    /// Prepares independent typed data while all registries are frozen.
    ///
    /// # Safety
    /// All declared claims must be validated and held until the item is dropped.
    /// Mutable claims must exclude every other read and write of that type.
    #[doc(hidden)]
    unsafe fn prepare<'w>(context: &ParamContext<'w>, state: &'w mut Self::State)
    -> Self::Item<'w>;
    /// Takes commands queued by this parameter.
    #[doc(hidden)]
    fn drain_commands(_state: &mut Self::State) -> Vec<DeferredCommand> {
        Vec::new()
    }
}

impl sealed::Sealed for () {}
impl SystemParam for () {
    type State = ();
    type Item<'w> = ();
    fn access(_: &mut ParamAccess) {}
    fn init(_: &mut World) {}
    unsafe fn prepare<'w>(_: &ParamContext<'w>, _: &'w mut ()) {}
}

impl<D: QueryDescriptor, F: crate::query::QueryFilter + 'static> sealed::Sealed for Query<D, F> {}
impl<D: QueryDescriptor, F: crate::query::QueryFilter + 'static> SystemParam for Query<D, F> {
    type State = QueryCache<D>;
    type Item<'w> = QueryView<'w, D, F>;
    fn access(access: &mut ParamAccess) {
        access.add_components(Query::<D, F>::accesses());
    }
    fn init(_: &mut World) -> QueryCache<D> {
        QueryCache::default()
    }
    unsafe fn prepare<'w>(
        context: &ParamContext<'w>,
        state: &'w mut QueryCache<D>,
    ) -> Self::Item<'w> {
        // SAFETY: Scheduler holds every derived query access for this batch.
        unsafe { PreparedQuery::<D, F>::prepare(context.storage, context.epoch, state).into_view() }
    }
}

macro_rules! impl_param_tuple {
    ($($P:ident:$index:tt),+) => {
        impl<$($P: SystemParam),+> sealed::Sealed for ($($P,)+) {}
        impl<$($P: SystemParam),+> SystemParam for ($($P,)+) {
            type State = ($($P::State,)+);
            type Item<'w> = ($($P::Item<'w>,)+);
            fn access(access: &mut ParamAccess) { $($P::access(access);)+ }
            fn init(world: &mut World) -> Self::State { ($($P::init(world),)+) }
            unsafe fn prepare<'w>(context: &ParamContext<'w>, state: &'w mut Self::State) -> Self::Item<'w> {
                // SAFETY: Registration rejects aliases across the entire tuple.
                unsafe { ($($P::prepare(context, &mut state.$index),)+) }
            }
            fn drain_commands(state: &mut Self::State) -> Vec<DeferredCommand> {
                let mut commands = Vec::new();
                $(commands.extend($P::drain_commands(&mut state.$index));)+
                commands
            }
        }
    };
}
impl_param_tuple!(A:0);
impl_param_tuple!(A:0, B:1);
impl_param_tuple!(A:0, B:1, C:2);
impl_param_tuple!(A:0, B:1, C:2, D:3);
impl_param_tuple!(A:0, B:1, C:2, D:3, E:4);
impl_param_tuple!(A:0, B:1, C:2, D:3, E:4, F:5);
impl_param_tuple!(A:0, B:1, C:2, D:3, E:4, F:5, G:6);
impl_param_tuple!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7);

#[cfg(test)]
mod tests;
