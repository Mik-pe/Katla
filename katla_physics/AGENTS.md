# katla_physics

Rapier3D physics wrapper with ECS components.

## Rules

- Game code interacts through `PhysicsWorld`, never directly with Rapier handles.
- `PhysicsActive(bool)` resource gates simulation behind play mode. Defaults to `false`.
- `CollisionFilter` uses `(a.layers & b.mask) != 0 && (b.layers & a.mask) != 0` — bitfields, not groups.
- Mesh colliders (`Trimesh`, `ConvexHull`) reference meshes by `MeshHandle`. The app-layer must populate `MeshColliderData` before constructing Rapier colliders.

## Conventions

- Shapes are in local space, transformed to world via entity's `TransformComponent`.
- Physics state sync happens in katla_app's physics system, not in this crate.
- Read `memory-bank/systemPatterns.md` for the full shape types and resource list.
