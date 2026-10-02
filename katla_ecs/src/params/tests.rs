use super::*;
use crate::{Read, System, SystemExecutionOrder, TypedSystem, Write};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Barrier, Mutex};

#[derive(Component, Default)]
struct Position(i32);
#[derive(Component, Default)]
struct Velocity(i32);
#[derive(Default)]
struct Total(i32);

struct Move;
impl TypedSystem for Move {
    type Params = (
        Query<(Write<Position>, Read<Velocity>)>,
        ResMut<Total>,
        Local<i32>,
        Option<Res<bool>>,
    );
    fn run(
        &mut self,
        (mut query, mut total, mut frames, optional): <Self::Params as SystemParam>::Item<'_>,
        _: f32,
    ) {
        *frames += 1;
        assert!(optional.is_none());
        for (_, (position, velocity)) in query.iter_mut() {
            position.0 += velocity.0;
            total.0 += *frames;
        }
    }
}

#[test]
fn test_param_query_resources_local_and_optional() {
    let mut world = World::new();
    let entity = world.spawn((Position(1), Velocity(2)));
    world.insert_resource(Total::default());
    world.register_typed_system(Move, SystemExecutionOrder::NORMAL);
    world.update(0.1);
    world.update(0.1);
    assert_eq!(world.get_component::<Position>(entity).unwrap().0, 5);
    assert_eq!(world.get_resource::<Total>().unwrap().0, 3);
}

struct DuplicateReads;
impl TypedSystem for DuplicateReads {
    type Params = (
        Res<Total>,
        Res<Total>,
        Query<Read<Position>>,
        Query<Read<Position>>,
    );
    fn run(&mut self, (a, b, qa, qb): <Self::Params as SystemParam>::Item<'_>, _: f32) {
        assert!(std::ptr::eq(&*a, &*b));
        assert_eq!(
            qa.iter().map(|(_, p)| p.0).sum::<i32>(),
            qb.iter().map(|(_, p)| p.0).sum::<i32>()
        );
    }
}
#[test]
fn test_param_duplicate_reads_share_typed_data() {
    let mut world = World::new();
    world.spawn((Position(7),));
    world.insert_resource(Total(7));
    world.register_typed_system(DuplicateReads, SystemExecutionOrder::NORMAL);
    world.update(0.0);
}

struct AliasResource;
impl TypedSystem for AliasResource {
    type Params = (Res<Total>, (Option<ResMut<Total>>,));
    fn run(&mut self, _: <Self::Params as SystemParam>::Item<'_>, _: f32) {
        panic!("alias reached execution");
    }
}
struct AliasQuery;
impl TypedSystem for AliasQuery {
    type Params = (Query<Read<Position>>, Query<Write<Position>>);
    fn run(&mut self, _: <Self::Params as SystemParam>::Item<'_>, _: f32) {
        panic!("alias reached execution");
    }
}
#[test]
fn test_param_aliases_fail_at_registration() {
    let mut world = World::new();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || world.register_typed_system(AliasResource, SystemExecutionOrder::NORMAL)
        ))
        .is_err()
    );
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || world.register_typed_system(AliasQuery, SystemExecutionOrder::NORMAL)
        ))
        .is_err()
    );
    assert_eq!(world.system_count(), 0);
}

struct SpawnRows(i32);
impl TypedSystem for SpawnRows {
    type Params = Commands;
    fn run(&mut self, mut commands: <Self::Params as SystemParam>::Item<'_>, _: f32) {
        commands.spawn((Position(self.0),));
        commands.spawn((Position(self.0 + 1),));
    }
}
#[test]
fn test_param_commands_fifo_and_registration_order() {
    let mut world = World::new();
    world.register_typed_system(SpawnRows(10), SystemExecutionOrder::NORMAL);
    world.register_typed_system(SpawnRows(20), SystemExecutionOrder::NORMAL);
    world.update(0.0);
    assert_eq!(
        world
            .query::<&Position>()
            .map(|(_, p)| p.0)
            .collect::<Vec<_>>(),
        [10, 11, 20, 21]
    );
}

struct SpawnBeforeRead;
impl TypedSystem for SpawnBeforeRead {
    type Params = (Query<Write<Position>>, Commands);
    fn run(&mut self, (_, mut commands): <Self::Params as SystemParam>::Item<'_>, _: f32) {
        commands.spawn((Position(42),));
    }
}
struct CollectRows;
impl TypedSystem for CollectRows {
    type Params = (Query<Read<Position>>, ResMut<Vec<i32>>);
    fn run(&mut self, (query, mut values): <Self::Params as SystemParam>::Item<'_>, _: f32) {
        values.clear();
        values.extend(query.iter().map(|(_, p)| p.0));
    }
}
#[test]
fn test_param_commands_visible_to_next_batch_and_query_cache() {
    let mut world = World::new();
    world.insert_resource(Vec::<i32>::new());
    world.register_typed_system(SpawnBeforeRead, SystemExecutionOrder::NORMAL);
    world.register_typed_system(CollectRows, SystemExecutionOrder::NORMAL);
    world.update(0.0);
    assert_eq!(world.get_resource::<Vec<i32>>().unwrap(), &[42]);
    world.update(0.0);
    assert_eq!(world.get_resource::<Vec<i32>>().unwrap(), &[42, 42]);
}

struct PanicOnce(bool);
impl TypedSystem for PanicOnce {
    type Params = Commands;
    fn run(&mut self, mut commands: <Self::Params as SystemParam>::Item<'_>, _: f32) {
        commands.spawn((Position(4),));
        if !self.0 {
            self.0 = true;
            panic!("intentional worker failure");
        }
    }
}
#[test]
fn test_param_panic_restores_systems_and_discards_batch_commands() {
    let mut world = World::new();
    world.register_typed_system(PanicOnce(false), SystemExecutionOrder::NORMAL);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| world.update(0.0))).is_err());
    assert_eq!(world.system_count(), 1);
    assert_eq!(world.entity_count(), 0);
    world.update(0.0);
    assert_eq!(world.entity_count(), 1);
}

struct ReadEvents;
impl TypedSystem for ReadEvents {
    type Params = (EventReader<i32>, ResMut<Vec<i32>>);
    fn run(&mut self, (mut reader, mut received): <Self::Params as SystemParam>::Item<'_>, _: f32) {
        received.extend(reader.read().copied());
        assert_eq!(reader.read().count(), 0);
    }
}
#[test]
fn test_param_events_clear_and_replacement_keep_new_events_visible() {
    let mut world = World::new();
    world.insert_resource(Vec::<i32>::new());
    world.register_typed_system(ReadEvents, SystemExecutionOrder::NORMAL);
    world.get_resource_mut::<Events<i32>>().unwrap().send(1);
    world.update(0.0);
    let events = world.get_resource_mut::<Events<i32>>().unwrap();
    events.clear();
    events.send(2);
    world.update(0.0);
    let mut replacement = Events::default();
    replacement.send(3);
    world.insert_resource(replacement);
    world.update(0.0);
    assert_eq!(world.get_resource::<Vec<i32>>().unwrap(), &[1, 2, 3]);
}
struct PublishEvent;
impl TypedSystem for PublishEvent {
    type Params = EventWriter<i32>;
    fn run(&mut self, mut writer: <Self::Params as SystemParam>::Item<'_>, _: f32) {
        writer.send(9);
    }
}
#[test]
fn test_param_event_writer_orders_reader() {
    let mut world = World::new();
    world.insert_resource(Vec::<i32>::new());
    world.register_typed_system(PublishEvent, SystemExecutionOrder::NORMAL);
    world.register_typed_system(ReadEvents, SystemExecutionOrder::NORMAL);
    world.update(0.0);
    assert_eq!(world.get_resource::<Vec<i32>>().unwrap(), &[9]);
}

struct ObserveOrder {
    value: i32,
    observed: Arc<Mutex<Vec<i32>>>,
}
impl TypedSystem for ObserveOrder {
    type Params = ();
    fn run(&mut self, _: (), _: f32) {
        self.observed.lock().unwrap().push(self.value);
    }
}
#[test]
fn test_param_absolute_order_barrier_for_disjoint_systems() {
    let mut world = World::new();
    let observed = Arc::new(Mutex::new(Vec::new()));
    world.register_typed_system(
        ObserveOrder {
            value: 2,
            observed: observed.clone(),
        },
        SystemExecutionOrder::LATE,
    );
    world.register_typed_system(
        ObserveOrder {
            value: 1,
            observed: observed.clone(),
        },
        SystemExecutionOrder::EARLY,
    );
    world.update(0.0);
    assert_eq!(*observed.lock().unwrap(), [1, 2]);
}

struct ExclusiveThread {
    caller: std::thread::ThreadId,
    called: Rc<RefCell<bool>>,
}
impl System for ExclusiveThread {
    fn update(&mut self, _: &mut World, _: f32) {
        assert_eq!(std::thread::current().id(), self.caller);
        *self.called.borrow_mut() = true;
    }
}
#[test]
fn test_param_non_send_exclusive_runs_on_caller() {
    let mut world = World::new();
    let called = Rc::new(RefCell::new(false));
    world.register_exclusive_system(
        Box::new(ExclusiveThread {
            caller: std::thread::current().id(),
            called: called.clone(),
        }),
        SystemExecutionOrder::NORMAL,
    );
    world.update_parallel(0.0);
    assert!(*called.borrow());
}

struct WorkerA(Arc<Barrier>);
struct WorkerB(Arc<Barrier>);
impl TypedSystem for WorkerA {
    type Params = (ResMut<Position>, Commands);
    fn run(&mut self, (mut value, mut commands): <Self::Params as SystemParam>::Item<'_>, _: f32) {
        self.0.wait();
        value.0 += 1;
        commands.spawn((Position(10),));
    }
}
impl TypedSystem for WorkerB {
    type Params = (ResMut<Velocity>, Commands);
    fn run(&mut self, (mut value, mut commands): <Self::Params as SystemParam>::Item<'_>, _: f32) {
        self.0.wait();
        value.0 += 2;
        commands.spawn((Position(20),));
    }
}
#[test]
fn test_parallel_params_transfer_only_disjoint_data_and_flush_in_registration_order() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .unwrap();
    let mut world = World::new();
    world.insert_resource(Position(0));
    world.insert_resource(Velocity(0));
    world.set_parallel_work_threshold(0);
    let barrier = Arc::new(Barrier::new(2));
    world.register_typed_system(WorkerA(barrier.clone()), SystemExecutionOrder::NORMAL);
    world.register_typed_system(WorkerB(barrier), SystemExecutionOrder::NORMAL);
    // World contains caller-thread resources and therefore cannot be moved into
    // Rayon. A scoped worker only receives the already prepared typed jobs.
    pool.in_place_scope(|_| world.update_parallel(0.0));
    assert_eq!(world.get_resource::<Position>().unwrap().0, 1);
    assert_eq!(world.get_resource::<Velocity>().unwrap().0, 2);
    assert_eq!(
        world
            .query::<&Position>()
            .map(|(_, p)| p.0)
            .collect::<Vec<_>>(),
        [10, 20]
    );
}

struct ClearDuringUpdate(Arc<std::sync::atomic::AtomicUsize>);
impl System for ClearDuringUpdate {
    fn update(&mut self, world: &mut World, _: f32) {
        world.clear_systems();
    }
    fn shutdown(&mut self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}
struct MustNotRun(Arc<std::sync::atomic::AtomicUsize>);
impl TypedSystem for MustNotRun {
    type Params = ();
    fn run(&mut self, _: (), _: f32) {
        panic!("system after clear must not execute");
    }
    fn shutdown(&mut self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}
#[test]
fn test_param_clear_systems_during_exclusive_update_stops_and_shuts_down_once() {
    let mut world = World::new();
    let shutdowns = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    world.register_exclusive_system(
        Box::new(ClearDuringUpdate(shutdowns.clone())),
        SystemExecutionOrder::NORMAL,
    );
    world.register_typed_system(MustNotRun(shutdowns.clone()), SystemExecutionOrder::NORMAL);
    world.update(0.0);
    assert_eq!(world.system_count(), 0);
    assert_eq!(shutdowns.load(Ordering::Relaxed), 2);
    world.update(0.0);
    drop(world);
    assert_eq!(shutdowns.load(Ordering::Relaxed), 2);
}

#[test]
fn test_parallel_param_panic_restores_systems_and_discards_all_batch_commands() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .unwrap();
    let mut world = World::new();
    world.set_parallel_work_threshold(0);
    world.register_typed_system(PanicOnce(false), SystemExecutionOrder::NORMAL);
    world.register_typed_system(SpawnRows(20), SystemExecutionOrder::NORMAL);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pool.in_place_scope(|_| world.update_parallel(0.0));
        }))
        .is_err()
    );
    assert_eq!(world.system_count(), 2);
    assert_eq!(world.entity_count(), 0);
    pool.in_place_scope(|_| world.update_parallel(0.0));
    assert_eq!(
        world
            .query::<&Position>()
            .map(|(_, p)| p.0)
            .collect::<Vec<_>>(),
        [4, 20, 21]
    );
}
