# katla_ecs

Custom Entity Component System framework.

## Core Structures

### ComponentStorage (`storage.rs`)
- Wraps `SparseSet<EntityId, Component>`
- One storage per component type
- Methods: `iter()`, `iter_mut()`, `get()`, `get_mut()`

### ComponentStorageManager (`storage.rs`)
- `HashMap<TypeId, Box<dyn Any>>` for each component type
- Type-safe: `add_component()`, `get_component()`, `get_storage()`

### World (`world.rs`)
- Central manager: entities, components, systems, input
- Methods: `create_entity()`, `add_component()`, `update()`, `register_system()`

## Query System

QueryData trait provides type-safe component access:

```rust
// Single component
for (entity, transform) in world.storage.query::<&mut TransformComponent>() {
    transform.position += Vec3::new(0.0, 1.0, 0.0);
}

// Multiple components (up to 3)
for (entity, vel, force) in world.storage.query::<(&mut Velocity, &Force)>() {
    vel.acceleration = force.value / vel.mass;
}
```

## Creating Components

```rust
use katla_ecs::Component;

#[derive(Component)]
pub struct MyComponent {
    pub value: f32,
}
```

## Creating Systems

```rust
use katla_ecs::System;

struct MySystem;

impl System for MySystem {
    fn update(&mut self, world: &mut World, delta_time: f32) {
        for (entity, comp) in world.storage.query::<&mut MyComponent>() {
            comp.value += delta_time;
        }
    }
}

world.register_system(Box::new(MySystem), SystemExecutionOrder::NORMAL);
```

## System Execution Order

`FIRST` → `EARLY` → `NORMAL` → `LATE` → `LAST`

## Dependencies

Must NOT depend on: `katla_app`, `katla_vulkan`, `katla_math`, `katla_ui`
