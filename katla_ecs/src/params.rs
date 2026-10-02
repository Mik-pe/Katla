//! Sealed typed system parameters and deferred structural commands.

use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::resource::ResourceStorage;
use crate::storage::ComponentStorageManager;
use crate::system::{ComponentAccess, ResourceAccess};
use crate::typed_query::{PreparedQuery, Query, QueryCache, QueryDescriptor, QueryView};
use crate::{Component, EntityId, Resource, Spawnable, World};

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

/// Required shared resource parameter. R must support shared worker access.
pub struct Res<R>(PhantomData<R>);
/// Required mutable resource parameter. R must support worker ownership.
///
/// Caller-thread-only resources remain accessible through exclusive systems:
///
/// ```compile_fail
/// use std::rc::Rc;
/// use katla_ecs::{ResMut, SystemParam, TypedSystem};
/// struct InvalidResourceSystem;
/// impl TypedSystem for InvalidResourceSystem {
///     type Params = ResMut<Rc<()>>;
///     fn run(&mut self, _: <Self::Params as SystemParam>::Item<'_>, _: f32) {}
/// }
/// ```
pub struct ResMut<R>(PhantomData<R>);
/// A resource borrowed for one invocation.
pub struct ResourceRef<'w, R> {
    resource: &'w R,
}
/// A resource exclusively borrowed for one invocation.
pub struct ResourceMut<'w, R> {
    resource: &'w mut R,
}
impl<R> Deref for ResourceRef<'_, R> {
    type Target = R;
    fn deref(&self) -> &R {
        self.resource
    }
}
impl<R> Deref for ResourceMut<'_, R> {
    type Target = R;
    fn deref(&self) -> &R {
        self.resource
    }
}
impl<R> DerefMut for ResourceMut<'_, R> {
    fn deref_mut(&mut self) -> &mut R {
        self.resource
    }
}

impl<R: Resource + Sync> sealed::Sealed for Res<R> {}
impl<R: Resource + Sync> SystemParam for Res<R> {
    type State = ();
    type Item<'w> = ResourceRef<'w, R>;
    fn access(access: &mut ParamAccess) {
        access.resource::<R>(false);
    }
    fn init(_: &mut World) {}
    unsafe fn prepare<'w>(context: &ParamContext<'w>, _: &'w mut ()) -> Self::Item<'w> {
        ResourceRef {
            resource: context.resources.get::<R>().unwrap_or_else(|| {
                panic!(
                    "required resource {} is missing",
                    std::any::type_name::<R>()
                )
            }),
        }
    }
}
impl<R: Resource + Send> sealed::Sealed for ResMut<R> {}
impl<R: Resource + Send> SystemParam for ResMut<R> {
    type State = ();
    type Item<'w> = ResourceMut<'w, R>;
    fn access(access: &mut ParamAccess) {
        access.resource::<R>(true);
    }
    fn init(_: &mut World) {}
    unsafe fn prepare<'w>(context: &ParamContext<'w>, _: &'w mut ()) -> Self::Item<'w> {
        // SAFETY: Scheduler holds an exclusive claim to this resource cell.
        ResourceMut {
            resource: unsafe { context.resources.prepare_ptr::<R>().map(|ptr| &mut *ptr) }
                .unwrap_or_else(|| {
                    panic!(
                        "required resource {} is missing",
                        std::any::type_name::<R>()
                    )
                }),
        }
    }
}
impl<R: Resource + Sync> sealed::Sealed for Option<Res<R>> {}
impl<R: Resource + Sync> SystemParam for Option<Res<R>> {
    type State = ();
    type Item<'w> = Option<ResourceRef<'w, R>>;
    fn access(access: &mut ParamAccess) {
        access.resource::<R>(false);
    }
    fn init(_: &mut World) {}
    unsafe fn prepare<'w>(context: &ParamContext<'w>, _: &'w mut ()) -> Self::Item<'w> {
        context
            .resources
            .get::<R>()
            .map(|resource| ResourceRef { resource })
    }
}
impl<R: Resource + Send> sealed::Sealed for Option<ResMut<R>> {}
impl<R: Resource + Send> SystemParam for Option<ResMut<R>> {
    type State = ();
    type Item<'w> = Option<ResourceMut<'w, R>>;
    fn access(access: &mut ParamAccess) {
        access.resource::<R>(true);
    }
    fn init(_: &mut World) {}
    unsafe fn prepare<'w>(context: &ParamContext<'w>, _: &'w mut ()) -> Self::Item<'w> {
        // SAFETY: Even an absent optional resource retains its exclusive claim.
        unsafe {
            context.resources.prepare_ptr::<R>().map(|ptr| ResourceMut {
                resource: &mut *ptr,
            })
        }
    }
}

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

/// Queues structural changes for deterministic application after the batch.
pub struct Commands;
/// One system's FIFO deferred command queue.
pub struct CommandQueue<'w>(&'w mut Vec<DeferredCommand>);
impl CommandQueue<'_> {
    /// Queues a component bundle to be spawned when this batch completes.
    pub fn spawn<B: Spawnable + Send + 'static>(&mut self, bundle: B) {
        self.0.push(Box::new(move |world| {
            world.spawn(bundle);
        }));
    }
    /// Queues destruction. Stale entity IDs have no effect.
    pub fn despawn(&mut self, id: EntityId) {
        self.0.push(Box::new(move |world| {
            world.destroy_entity(id);
        }));
    }
    /// Queues component insertion. Stale entity IDs have no effect.
    pub fn insert<T: Component>(&mut self, id: EntityId, component: T) {
        self.0
            .push(Box::new(move |world| world.add_component(id, component)));
    }
    /// Queues component removal. Stale entity IDs have no effect.
    pub fn remove<T: Component>(&mut self, id: EntityId) {
        self.0.push(Box::new(move |world| {
            world.remove_component::<T>(id);
        }));
    }
    /// Queues a resource insertion on the caller thread.
    pub fn insert_resource<R: Resource + Send>(&mut self, resource: R) {
        self.0
            .push(Box::new(move |world| world.insert_resource(resource)));
    }
    /// Queues a resource removal on the caller thread.
    pub fn remove_resource<R: Resource>(&mut self) {
        self.0.push(Box::new(|world| {
            world.remove_resource::<R>();
        }));
    }
}
impl sealed::Sealed for Commands {}
impl SystemParam for Commands {
    type State = Vec<DeferredCommand>;
    type Item<'w> = CommandQueue<'w>;
    fn access(access: &mut ParamAccess) {
        assert!(
            !access.commands,
            "a system can have only one Commands parameter"
        );
        access.commands = true;
    }
    fn init(_: &mut World) -> Self::State {
        Vec::new()
    }
    unsafe fn prepare<'w>(_: &ParamContext<'w>, state: &'w mut Self::State) -> Self::Item<'w> {
        CommandQueue(state)
    }
    fn drain_commands(state: &mut Self::State) -> Vec<DeferredCommand> {
        std::mem::take(state)
    }
}

static NEXT_EVENT_LOG: AtomicU64 = AtomicU64::new(1);

/// Typed event log with independent reader cursors.
///
/// Call [`clear`](Self::clear) after consumers finish a retention interval to
/// release retained events. Clearing preserves sequence numbers, so later events
/// remain visible to existing readers. Replacing the log resets every reader.
pub struct Events<E> {
    events: Vec<E>,
    identity: u64,
    first_sequence: u64,
}
impl<E> Default for Events<E> {
    fn default() -> Self {
        let identity = NEXT_EVENT_LOG
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .expect("event log identities exhausted");
        Self {
            events: Vec::new(),
            identity,
            first_sequence: 0,
        }
    }
}
impl<E> Events<E> {
    /// Appends an event to the log.
    pub fn send(&mut self, event: E) {
        self.events.push(event);
    }
    /// Returns retained events.
    pub fn as_slice(&self) -> &[E] {
        &self.events
    }
    /// Releases retained events while preserving reader positions.
    pub fn clear(&mut self) {
        self.first_sequence = self
            .first_sequence
            .checked_add(self.events.len() as u64)
            .expect("event sequence exhausted");
        self.events.clear();
    }
}
#[doc(hidden)]
#[derive(Default)]
pub struct EventCursor {
    identity: u64,
    next_sequence: u64,
}
/// Reads events published since this system's previous invocation.
pub struct EventReader<E>(PhantomData<E>);
/// Publishes typed events. Writers run before dependent readers in later batches.
pub struct EventWriter<E>(PhantomData<E>);
/// Borrowed event reader with its per-system cursor.
pub struct EventRead<'w, E> {
    events: &'w Events<E>,
    cursor: &'w mut EventCursor,
}
impl<E> EventRead<'_, E> {
    /// Returns unread events and advances this reader's cursor.
    pub fn read(&mut self) -> impl Iterator<Item = &E> {
        let log = self.events;
        if self.cursor.identity != log.identity {
            self.cursor.identity = log.identity;
            self.cursor.next_sequence = log.first_sequence;
        }
        let start = self
            .cursor
            .next_sequence
            .saturating_sub(log.first_sequence)
            .min(log.events.len() as u64) as usize;
        self.cursor.next_sequence = log
            .first_sequence
            .checked_add(log.events.len() as u64)
            .expect("event sequence exhausted");
        self.events.events[start..].iter()
    }
}
/// Borrowed event publisher.
pub struct EventWrite<'w, E>(&'w mut Events<E>);
impl<E> EventWrite<'_, E> {
    /// Publishes an event to dependent readers in the next batch.
    pub fn send(&mut self, event: E) {
        self.0.send(event);
    }
}
impl<E: Send + Sync + 'static> sealed::Sealed for EventReader<E> {}
impl<E: Send + Sync + 'static> SystemParam for EventReader<E> {
    type State = EventCursor;
    type Item<'w> = EventRead<'w, E>;
    fn access(access: &mut ParamAccess) {
        access.resource::<Events<E>>(false);
    }
    fn init(world: &mut World) -> EventCursor {
        world.get_resource_mut_or_insert_with::<Events<E>>();
        EventCursor::default()
    }
    unsafe fn prepare<'w>(
        context: &ParamContext<'w>,
        cursor: &'w mut EventCursor,
    ) -> Self::Item<'w> {
        EventRead {
            events: context
                .resources
                .get::<Events<E>>()
                .expect("event log must remain registered"),
            cursor,
        }
    }
}
impl<E: Send + 'static> sealed::Sealed for EventWriter<E> {}
impl<E: Send + 'static> SystemParam for EventWriter<E> {
    type State = ();
    type Item<'w> = EventWrite<'w, E>;
    fn access(access: &mut ParamAccess) {
        access.resource::<Events<E>>(true);
    }
    fn init(world: &mut World) {
        world.get_resource_mut_or_insert_with::<Events<E>>();
    }
    unsafe fn prepare<'w>(context: &ParamContext<'w>, _: &'w mut ()) -> Self::Item<'w> {
        // SAFETY: Scheduler holds the exclusive event-log claim.
        EventWrite(
            unsafe {
                context
                    .resources
                    .prepare_ptr::<Events<E>>()
                    .map(|ptr| &mut *ptr)
            }
            .expect("event log must remain registered"),
        )
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
