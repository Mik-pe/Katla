# ADR: Physics Engine Selection

**Status**: Accepted  
**Date**: 2026-05-25  
**Decision**: Use Rapier3D as the physics backend

## Context

Katla needs a physics system supporting rigid body dynamics, collision detection (broadphase + narrowphase), constraints, and raycasting. The engine uses a custom ECS, targets macOS (Metal) and desktop (Vulkan), and must integrate cleanly with the existing component model.

## Evaluated Options

### 1. Rapier3D

Pure Rust, cross-platform physics engine by Dimforge. Active development (2025 review + 2026 roadmap). Features rigid bodies, colliders, joints, CCD, scene queries, island-based sleeping.

| Aspect | Rating |
|--------|--------|
| Maturity | High — production used, Bevy integration, web support |
| ECS compatibility | Good — uses its own ECS-agnostic pipeline, maps well to custom ECS via handle-based API |
| Features | Rigid bodies, joints, CCD, scene queries, serialization |
| Performance | WASM SIMD optimizations, parallel pipeline, island-based sleeping |
| Maintenance | Active — Dimforge is a dedicated physics company |
| License | Apache-2.0 / MIT |
| Build complexity | Pure Rust, no C++ dependency |

### 2. Avian3D (formerly bevy_xpbd)

ECS-driven XPBD physics engine tightly coupled to Bevy's ECS.

| Aspect | Rating |
|--------|--------|
| Maturity | Medium — v0.4, rapidly improving |
| ECS compatibility | Poor for Katla — deeply integrated with Bevy ECS (components, schedules, resources) |
| Features | Rigid bodies, joints, collision, spatial queries (growing) |
| Performance | XPBD solver, less battle-tested than impulse-based |
| License | Apache-2.0 / MIT |
| Build complexity | Pure Rust |

**Verdict**: Rejected. Requires Bevy ECS. Extracting the core would be a significant effort.

### 3. Jolt Physics (via Rust bindings)

AAA C++ physics engine (Horizon Forbidden West, Death Stranding 2). Rust bindings: `rolt` (safe wrapper via JoltC), `jolt-rs` (raw bindings).

| Aspect | Rating |
|--------|--------|
| Maturity | Very high (C++ core) / Low (Rust bindings) |
| ECS compatibility | Moderate — handle-based API, would need Katla wrapper |
| Features | Full AAA: rigid bodies, soft bodies, vehicles, ragdolls, character controllers |
| Performance | Excellent — multithreaded, SIMD, job system |
| License | MIT (Jolt) / Various (bindings) |
| Build complexity | High — C++ compilation, no easy cross-compilation |

**Verdict**: Rejected. Rust bindings are immature, C++ build adds significant complexity for macOS + cross-platform.

### 4. PhysX 5

NVIDIA's physics engine. Rust bindings exist (`physx-rs`) but are outdated.

| Aspect | Rating |
|--------|--------|
| Maturity | Very high |
| ECS compatibility | Handle-based, possible but heavy wrapper needed |
| Features | Full AAA + GPU acceleration |
| Performance | Excellent (GPU-accelerated) |
| License | BSD-3 (open source since PhysX 5) |
| Build complexity | Very high — C++ SDK, GPU driver dependency |

**Verdict**: Rejected. Heavy build dependency, overkill for Katla's scope, bindings are stale.

### 5. Custom implementation

Build broadphase (SAP), narrowphase (GJK/EPA), rigid body solver, and constraints from scratch.

| Aspect | Rating |
|--------|--------|
| Maturity | N/A |
| ECS compatibility | Perfect — designed for Katla's ECS |
| Features | Only what we build |
| Performance | Unknown — depends on implementation quality |
| Maintenance | High ongoing cost |
| Build complexity | None (pure Rust, no deps) |

**Verdict**: Rejected for now. Collision detection and rigid body dynamics are complex to get right (GJK/EPA, persistent manifolds, iterative solvers). Custom collision shapes + AABB broadphase can coexist as a lightweight layer for simple trigger/query needs, but full rigid body simulation should use Rapier.

## Decision

Use **Rapier3D** as the primary physics backend.

### Integration Strategy

1. **Rapier owns the simulation state**: `rapier3d::RigidBodySet`, `ColliderSet`, `JointSet`, `IslandManager` managed by a `PhysicsWorld` wrapper in `katla_physics`
2. **ECS bridge via components**: `ColliderShape` (already exists) maps to `rapier3d::ColliderBuilder`, `RigidBody` component maps to `rapier3d::RigidBodyBuilder`
3. **Sync layer**: `PhysicsSystem` reads `ColliderShape`/`RigidBody` components, creates/updates Rapier handles, steps simulation, writes transforms back to `TransformComponent`
4. **Scene queries exposed via resource**: `PhysicsWorld::raycast()`, `shape_cast()` accessible from systems and scripts
5. **Custom collision shapes remain**: Lightweight `ColliderShape` enum stays for serialization and inspector UI; converted to Rapier colliders at runtime

### What This Means for Existing Code

- The existing `ColliderShape`, `ColliderState`, `CollisionFilter` components in `katla_physics` remain as the ECS-facing API
- Rapier handles are stored internally, not exposed to game code
- The custom AABB broadphase (Phase 2 TODO items) becomes optional — Rapier provides its own broadphase
- Raycasting (Phase 4) delegates to Rapier's scene query pipeline

## Consequences

- **Positive**: Battle-tested physics, minimal maintenance burden, good performance, pure Rust, easy cross-platform
- **Positive**: Feature-rich — joints, CCD, character controllers available when needed
- **Negative**: Additional dependency (~200KB), less control over solver internals
- **Negative**: Rapier's API may not perfectly align with every Katla convention, requiring adapter code
