# Character Controller Design

**Status**: Proposed  
**Date**: 2026-07-08  
**Related TODO**: `TODO.md` → Physics → Phase 5.5 → Explore character controller design

## Context

Katla already uses Rapier3D through the `katla_physics::PhysicsWorld` wrapper. The ECS-facing API is intentionally Katla-owned: components such as `RigidBody`, `ColliderShape`, `PhysicsMaterial`, `CollisionFilter`, and `TriggerVolume` are converted to Rapier handles by `RapierPhysicsSystem`.

The existing physics loop does the important groundwork for a character controller:

1. `RapierPhysicsSystem` creates missing Rapier bodies/colliders from ECS components.
2. Kinematic bodies are already supported through `BodyType::Kinematic`.
3. Kinematic transforms are pushed from ECS to Rapier before simulation.
4. Dynamic transforms and velocities are read back after simulation.
5. Scene queries already exist through `PhysicsWorld::raycast()` and `PhysicsWorld::shape_cast()`.

A player/NPC character controller should build on this bridge rather than exposing Rapier types directly to game code.

## Decision

Add a Katla-level `CharacterController` component backed by Rapier's `KinematicCharacterController` internally.

The ECS-facing model should be:

- `CharacterController` stores movement tuning and runtime state.
- The controlled entity also has `RigidBody::kinematic()` and a capsule-shaped `ColliderShape`.
- A physics-side movement helper wraps Rapier's controller and returns a Katla-owned result type.
- The app-layer physics system applies character motion before kinematic transforms are synced into Rapier for the frame.

This keeps Rapier as an implementation detail and preserves the current component/serialization style.

## Component shape

Initial component proposal:

```rust
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct CharacterController {
    pub enabled: bool,

    // Movement tuning.
    pub max_speed: f32,
    pub acceleration: f32,
    pub air_acceleration: f32,
    pub jump_speed: f32,
    pub gravity: f32,

    // Rapier controller tuning, stored in Katla-friendly units.
    pub up: Vec3,
    pub offset: f32,
    pub slide: bool,
    pub max_slope_climb_angle: f32,
    pub min_slope_slide_angle: f32,
    pub snap_to_ground: Option<f32>,
    pub autostep: Option<CharacterAutostepSettings>,
    pub normal_nudge_factor: f32,

    // Runtime state; skipped by serialization.
    #[serde(skip)]
    pub velocity: Vec3,
    #[serde(skip)]
    pub grounded: bool,
}
```

`CharacterAutostepSettings` should mirror only the fields Katla wants to expose, not Rapier's full API. The first useful subset is:

```rust
pub struct CharacterAutostepSettings {
    pub max_height: f32,
    pub min_width: f32,
    pub include_dynamic_bodies: bool,
}
```

Defaults should target editor/gameplay usability:

- capsule collider, roughly human scale
- slide enabled
- snap-to-ground enabled with a small threshold
- autostep disabled by default because it is more expensive than simple sliding
- slope angles stored in radians to match the rest of the low-level math/physics layer

## Input model

Do not bake keyboard/gamepad input directly into the physics component.

Use a separate component or scripting API to submit frame-local desired movement:

```rust
pub struct CharacterControllerInput {
    pub desired_horizontal_velocity: Vec3,
    pub jump_requested: bool,
}
```

This lets the same controller work for:

- player input
- AI steering
- scripted cutscenes
- editor test harnesses

The controller system consumes the input, updates `CharacterController::velocity`, and clears one-shot fields such as `jump_requested` after use.

## PhysicsWorld API

Add a wrapper method instead of calling Rapier directly from `katla_app`:

```rust
pub struct CharacterMoveResult {
    pub effective_translation: Vec3,
    pub grounded: bool,
    pub collisions: Vec<CharacterCollision>,
}

impl PhysicsWorld {
    pub fn move_character(
        &self,
        controller: &CharacterController,
        shape: &ColliderShape,
        mesh_data: Option<&MeshColliderData>,
        transform: &Transform,
        desired_translation: Vec3,
        exclude_collider: Option<ColliderHandle>,
        dt: f32,
    ) -> CharacterMoveResult;
}
```

The wrapper should:

1. Convert `CharacterController` settings to Rapier's controller type.
2. Convert `ColliderShape` to a Rapier shape using the existing collider conversion path.
3. Build a query pipeline from the existing physics state.
4. Exclude the character's own collider from the query filter.
5. Return Katla-owned movement/collision data.

The first implementation can return only `effective_translation` and `grounded`; collision details can be added when gameplay needs them.

## System integration

The most robust MVP is to integrate character movement into `RapierPhysicsSystem`, between body spawning and kinematic transform sync:

1. Clean up destroyed bodies/joints.
2. Spawn new bodies and joints.
3. Apply character controllers:
   - query `CharacterController`, `CharacterControllerInput`, `RigidBody`, `ColliderShape`, and `TransformComponent`
   - skip disabled controllers and entities without spawned collider handles
   - compute horizontal motion from input
   - apply gravity/jump to vertical velocity
   - call `PhysicsWorld::move_character()`
   - write the corrected transform back to `TransformComponent`
   - update grounded/runtime state
4. Sync kinematic transforms into Rapier.
5. Step physics if `PhysicsActive` is true.
6. Read dynamic bodies back.
7. Process trigger events.

This order lets the character controller use the previous frame's physics state to choose a corrected transform, then syncs that corrected transform into the kinematic body before the next simulation step.

A later cleanup can split character movement into its own ECS system once the scheduler has explicit ordering and resource access declarations for this path.

## Scripting API

Expose high-level movement methods rather than raw Rapier controls:

```lua
world:set_character_move(entity_id, x, y, z)
world:request_character_jump(entity_id)
local grounded = world:is_character_grounded(entity_id)
```

Scripts should not need to know whether the implementation uses Rapier, a custom query pipeline, or a future backend.

## Editor UX

Add an inspector section for `CharacterController` with:

- enabled toggle
- max speed / acceleration / air acceleration
- jump speed / gravity
- slope climb / slide angles
- snap-to-ground threshold
- autostep toggle and fields
- read-only grounded state

When adding a `CharacterController` component in the editor, offer to add missing required components:

- `RigidBody::kinematic()`
- `ColliderShape::Capsule(...)`
- `CharacterControllerInput`

## Testing plan

Add tests in small steps:

1. Unit test default `CharacterController` values.
2. Unit test conversion from Katla settings to Rapier settings.
3. Integration test: character moves on flat ground without penetrating it.
4. Integration test: character slides along a wall instead of passing through.
5. Integration test: snap-to-ground keeps the character grounded over small drops.
6. Integration test: jump only applies while grounded.
7. Regression test: character controller ignores its own collider in scene queries.

## Implementation TODOs

- [ ] Add `CharacterController` and `CharacterAutostepSettings` components to `katla_physics`.
- [ ] Add `CharacterControllerInput` component or equivalent frame-local movement request type.
- [ ] Re-export character controller types from `katla_physics::lib`.
- [ ] Add `CharacterMoveResult` and initial `PhysicsWorld::move_character()` wrapper.
- [ ] Add own-collider exclusion to character movement queries.
- [ ] Wire character movement into `RapierPhysicsSystem` before kinematic transform sync.
- [ ] Add editor inspector UI for `CharacterController`.
- [ ] Add scene serialization descriptors for `CharacterController` and required settings.
- [ ] Add Luau scripting bindings for move, jump, and grounded queries.
- [ ] Add flat-ground, wall-slide, snap-to-ground, jump, and own-collider regression tests.

## Open questions

- Should gravity be a per-controller field, or should controllers read global `PhysicsWorld` gravity?
- Should controller input be a serializable component, a transient component, or a resource keyed by entity?
- Should dynamic body pushing use Rapier's approximate impulse helper in the MVP, or wait until after basic movement is stable?
- Should the editor auto-create capsule colliders from visual mesh bounds once collider auto-fit work lands?
