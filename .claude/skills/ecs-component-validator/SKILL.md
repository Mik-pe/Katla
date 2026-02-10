---
name: ecs-component-validator
description: Validate ECS component usage, storage patterns, system registration, and query system correctness in Katla's custom ECS framework. Use when adding new components, systems, or debugging ECS issues.
allowed-tools: Read, Grep, Glob, Bash
---

# ECS Component Validator

## Overview

This skill validates ECS component and system implementation in `katla_ecs/` and usage in `katla_app/` to ensure correct patterns, proper storage access, and system execution order.

## ECS Architecture

### Core Data Structures

**SparseSet** (`katla_ecs/src/sparse_set.rs`):
- O(1) lookup, insert, remove operations
- Maintains contiguous `dense: Vec<(K, V)>` for iteration
- Uses `sparse: HashMap<K, usize>` for key→index mapping
- Excellent cache locality for iteration

**ComponentStorage** (`katla_ecs/src/storage.rs`):
- Wraps SparseSet<EntityId, Component>
- Each component type has separate storage
- Provides `iter()`, `iter_mut()`, `get()`, `get_mut()`

**ComponentStorageManager** (`katla_ecs/src/storage.rs`):
- HashMap storing `TypeId → Box<dyn Any>` for each component type
- Provides type-safe `add_component()`, `get_component()`, `get_storage()`
- Uses unsafe code internally for multi-borrow (sound due to TypeId uniqueness)

### Query System

**QueryData Trait** (`katla_ecs/src/query/`):
- Type-safe query API for component access
- Implemented for tuples: `&T`, `&mut T`, `(&T, &U)`, `(&mut T, &mut U)`, etc.
- Supports up to 3-component queries (iter1, iter2, iter3 modules)
- Automatically filters entities without all required components

## Validation Commands

### Component Trait Usage

```bash
# Find all component definitions
grep -rn "derive(Component)" katla_app/src/ katla_ecs/src/

# Check for manual Component impls
grep -rn "impl Component for" katla_app/src/ katla_ecs/src/

# Verify all components have Component derive or impl
# All structs stored in ComponentStorage should derive Component
```

### Storage Access Patterns

```bash
# Check for direct ComponentStorageManager access (should use query)
grep -rn "storage\.get_storage\|storage\.get_component\|storage\.add_component" \
  katla_app/src/ systems/

# Check for query usage (preferred)
grep -rn "\.query::<" katla_app/src/systems/

# Check for world.storage usage in systems
grep -rn "world\.storage\.query" katla_app/src/systems/
```

### System Registration

```bash
# Find all system registrations
grep -rn "register_system\|add_system" katla_app/src/

# Check for System trait implementations
grep -rn "impl System for" katla_app/src/

# Verify system execution order specification
grep -rn "SystemExecutionOrder::" katla_app/src/
```

## Common ECS Issues

### Issue 1: Missing Component Derive

**Problem:** Struct stored in ECS but doesn't derive Component.

```rust
// WRONG: No Component derive
pub struct Transform {
    pub position: Vec3,
}

world.add_component(entity, Transform { position: Vec3::zero() });
// Error: the trait `Component` is not implemented for `Transform`

// CORRECT: Derive Component
#[derive(Component)]
pub struct Transform {
    pub position: Vec3,
}
```

**Detection:**
```bash
# Find structs added to ECS without Component derive
# 1. Find add_component calls
grep -rn "add_component<" katla_app/src/ | \
  sed 's/.*add_component<\(.*\)>.*/\1/' | \
  sort -u

# 2. Check each has Component derive or impl
# For each type above:
grep -rn "impl Component for Type\|derive(Component)" | grep "Type"
```

### Issue 2: Direct Storage Access Instead of Query

**Problem:** Accessing storage directly instead of using type-safe queries.

```rust
// WRONG: Direct storage access (unsafe, error-prone)
impl System for MySystem {
    fn update(&mut self, world: &mut World, _delta_time: f32) {
        let storage = world.storage.get_storage::<Transform>();
        // Unsafe manual iteration
        for (entity, transform) in storage.iter_mut() {
            // ...
        }
    }
}

// CORRECT: Use query API
impl System for MySystem {
    fn update(&mut self, world: &mut World, _delta_time: f32) {
        for (entity, transform) in world.storage.query::<&mut Transform>() {
            // ...
        }
    }
}
```

**Detection:**
```bash
# Find direct storage access in systems
grep -rn "get_storage\|get_component\|add_component" \
  katla_app/src/systems/ katla_ecs/src/
# Should only appear in storage.rs implementation, not in systems
```

### Issue 3: Multiple Mutable Borrows of Same Component

**Problem:** Attempting to borrow same component type mutably multiple times.

```rust
// WRONG: Multiple &mut Transform queries
for (entity1, transform1) in world.storage.query::<&mut Transform>() {
    for (entity2, transform2) in world.storage.query::<&mut Transform>() {
        // ERROR: cannot borrow world.storage mutably more than once
    }
}

// CORRECT: Single query with filter
for (entity, transform) in world.storage.query::<&mut Transform>() {
    // Process all transforms in one pass
}
```

**Detection:**
```bash
# Check for nested queries of same type
grep -A 5 "query::<.*mut" katla_app/src/systems/ | \
  grep -B 3 "query::<.*mut"
# Should not see nested queries with &mut of same type
```

### Issue 4: Incorrect System Execution Order

**Problem:** System depends on another system's output but runs before it.

```rust
// WRONG: PhysicsSystem runs before TransformSystem
// but depends on updated transforms
world.register_system(Box::new(PhysicsSystem), SystemExecutionOrder::NORMAL);
world.register_system(Box::new(TransformSystem), SystemExecutionOrder::NORMAL);
// Order is undefined!

// CORRECT: Specify execution order
world.register_system(Box::new(TransformSystem), SystemExecutionOrder::EARLY);
world.register_system(Box::new(PhysicsSystem), SystemExecutionOrder::NORMAL);
// TransformSystem runs first
```

**Detection:**
```bash
# Check system registrations
grep -rn "register_system.*SystemExecutionOrder" katla_app/src/

# Verify dependencies are satisfied
# - Input handling: EARLY
# - Game logic: EARLY or NORMAL
# - Physics: NORMAL
# - Rendering: LATE
```

### Issue 5: Query Not Filtering Entities

**Problem:** Query returns entities that don't have all required components.

```rust
// User code expects behavior that only occurs when all components present
for (entity, transform) in world.storage.query::<&Transform>() {
    // Assumes velocity exists, but might not!
    let vel = world.storage.get_component::<Velocity>(entity).unwrap();
    // Could panic if entity doesn't have Velocity
}

// CORRECT: Query for all required components
for (entity, (transform, vel)) in world.storage.query::<(&Transform, &Velocity)>() {
    // Only entities with both Transform and Velocity
}
```

**Detection:**
```bash
# Find get_component calls inside query loops
grep -A 3 "query::<" katla_app/src/systems/ | \
  grep "get_component\|get_mut"
# These should be part of the query instead
```

### Issue 6: Component Not Removed When Entity Destroyed

**Problem:** Destroying entity but leaving components in storage.

```rust
// WRONG: Only removing entity from entities set
impl World {
    pub fn destroy_entity(&mut self, entity: EntityId) {
        self.entities.remove(&entity);
        // Components still in storage!
    }
}

// CORRECT: Remove from all component storages
impl World {
    pub fn destroy_entity(&mut self, entity: EntityId) {
        self.entities.remove(&entity);
        self.storage.remove_entity(entity);  // Removes from all storages
    }
}
```

**Detection:**
```bash
# Check entity destruction implementation
grep -A 10 "fn destroy_entity\|fn remove_entity" katla_ecs/src/world.rs

# Verify storage cleanup
grep -A 10 "fn remove_entity" katla_ecs/src/storage.rs
```

## Component Patterns

### Defining Components

```rust
use katla_ecs::Component;

#[derive(Component)]
pub struct Transform {
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

#[derive(Component)]
pub struct Velocity {
    pub linear: Vec3,
    pub angular: Vec3,
}
```

### Querying Components

```rust
// Single component, immutable
for (entity, transform) in world.storage.query::<&Transform>() {
    println!("Entity {:?} at {:?}", entity, transform.position);
}

// Single component, mutable
for (entity, transform) in world.storage.query::<&mut Transform>() {
    transform.position += Vec3::new(0.0, 1.0, 0.0);
}

// Multiple components, mixed mutability
for (entity, (transform, vel)) in world.storage.query::<(&mut Transform, &Velocity)>() {
    transform.position += vel.linear * delta_time;
}

// Three components
for (entity, (transform, vel, acc)) in world.storage.query::<(&mut Transform, &Velocity, &Acceleration)>() {
    vel.linear += acc.value * delta_time;
    transform.position += vel.linear * delta_time;
}
```

### Creating Entities with Components

```rust
// Create entity
let entity = world.create_entity();

// Add components
world.add_component(entity, Transform {
    position: Vec3::new(0.0, 0.0, 0.0),
    rotation: Quat::identity(),
    scale: Vec3::new(1.0, 1.0, 1.0),
});

world.add_component(entity, Velocity {
    linear: Vec3::zero(),
    angular: Vec3::zero(),
});
```

## System Patterns

### System Trait Implementation

```rust
use katla_ecs::{System, World, SystemExecutionOrder};

struct MovementSystem;

impl System for MovementSystem {
    fn update(&mut self, world: &mut World, delta_time: f32) {
        for (entity, (transform, vel)) in world.storage.query::<(&mut Transform, &Velocity)>() {
            transform.position += vel.linear * delta_time;
            transform.rotation *= Quat::from_axis_angle(
                vel.angular.normalize(),
                vel.angular.length() * delta_time
            );
        }
    }

    fn name(&self) -> &str {
        "MovementSystem"
    }
}
```

### System Registration

```rust
// Register with execution order
world.register_system(
    Box::new(MovementSystem),
    SystemExecutionOrder::NORMAL
);
```

### System Execution Order

```rust
pub enum SystemExecutionOrder {
    FIRST,   // Input handling, startup tasks
    EARLY,   // Transform updates, preprocessing
    NORMAL,  // Game logic, physics
    LATE,    // Post-processing, cleanup
    LAST,    // Rendering, UI
}
```

**Typical Order:**
1. **FIRST** - Input collection, state changes
2. **EARLY** - Transform hierarchy updates, animation
3. **NORMAL** - Physics, game logic, AI
4. **LATE** - Audio, particle cleanup
5. **LAST** - Rendering (should be separate from ECS update)

## ECS Validation Checklist

### Component Definition
- [ ] All components derive `Component` or implement `Component` trait
- [ ] Component is Copy/Clone if needed for querying
- [ ] Component doesn't contain non-Send types (no Rc, Arc with !Sync)
- [ ] Component fields are public if accessed directly

### Storage Access
- [ ] Systems use `query::<>()` API (not direct storage access)
- [ ] Query tuples match required components
- [ ] No nested mutable queries of same component type
- [ ] All get_component() calls in queries are moved to query tuple

### System Implementation
- [ ] System implements `System` trait
- [ ] `update()` method uses correct delta_time
- [ ] System registered with appropriate execution order
- [ ] System doesn't access world.storage outside of query

### Entity Management
- [ ] Entities created with `world.create_entity()`
- [ ] Entities removed with `world.destroy_entity()` (removes components)
- [ ] No orphaned components (entity destroyed, components remain)

### Query Usage
- [ ] Query only returns entities with all required components
- [ ] Immutable queries (`&T`) used when not modifying
- [ ] Mutable queries (`&mut T`) only when needed
- [ ] No double mutable borrows of same component

## Katla-Specific ECS Usage

### Application Layer Systems

Located in `katla_app/src/systems/`:

- **FlyCameraSystem** - Camera input handling (EARLY)
- **TransformSystem** - Transform hierarchy updates (EARLY)
- **AnimationSystem** - Animation updates (NORMAL)
- **PhysicsSystem** - Physics simulation (NORMAL)

### Component Location

** katla_app/src/components/**:
- Transform
- Drawable
- FlyCamera
- Light

**Entities created in** `katla_app/src/entities/`:
- Camera entities
- Model entities
- Light entities

## Debugging ECS Issues

### Enable Query Logging

```rust
// In system
fn update(&mut self, world: &mut World, delta_time: f32) {
    let count = world.storage.query::<(&Transform, &Velocity)>().count();
    println!("MovementSystem updating {} entities", count);

    for (entity, (transform, vel)) in world.storage.query::<(&Transform, &Velocity)>() {
        // ...
    }
}
```

### Check Entity Counts

```rust
println!("Total entities: {}", world.entities.len());
println!("Transform components: {}",
    world.storage.get_storage::<Transform>().len());
println!("Velocity components: {}",
    world.storage.get_storage::<Velocity>().len());
```

### Verify System Execution

```rust
// In World::update()
for system in &mut self.systems {
    println!("Executing system: {}", system.system.name());
    system.system.update(self, delta_time);
}
```

## Code Review Checklist for ECS

- [ ] Components derive Component trait
- [ ] Systems use query API, not direct storage access
- [ ] System execution order is correct
- [ ] No double mutable borrows of same component
- [ ] Queries include all required components
- [ ] Entity destruction removes all components
- [ ] No use of unwrap() on query results (iterate instead)
- [ ] Systems don't access world.storage outside queries
- [ ] Component types don't have lifetime parameters
- [ ] Systems are stateless or store state correctly

## Resources

- `katla_ecs/src/` - ECS framework implementation
- `katla_app/src/systems/` - Application systems
- `CLAUDE.md` - ECS architecture documentation
