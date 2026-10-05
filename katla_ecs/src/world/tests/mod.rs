use crate::components::Component;
use crate::{ComponentEvent, EntityEvent, EntityId, System, SystemExecutionOrder, World};

#[derive(Component, Default)]
struct TestComponent {
    value: i32,
}

mod changes;
mod entities;
mod events;
mod parallel_queries;
mod queries;
mod systems;
mod tracking;
mod validation;
