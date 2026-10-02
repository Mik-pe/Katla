//! Typed event logs and per-system reader cursors.

use std::marker::PhantomData;
use std::sync::atomic::{AtomicU64, Ordering};

use super::{ParamAccess, ParamContext, SystemParam, sealed};
use crate::World;

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
