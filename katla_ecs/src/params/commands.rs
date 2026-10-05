//! Structural commands applied after typed borrows end.

use super::{DeferredCommand, ParamAccess, ParamContext, SystemParam, sealed};
use crate::{Component, EntityId, Resource, Spawnable, World};

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
