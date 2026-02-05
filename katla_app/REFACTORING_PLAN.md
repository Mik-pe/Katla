# Katla App Refactoring Plan

**Status:** Draft
**Date:** 2025-02-05
**Version:** 1.0

## Overview

This document outlines a comprehensive refactoring plan for `katla_app` to address ECS pattern violations, Vulkan abstraction issues, and technical debt identified in the codebase.

**Summary of Issues:** 11 issues (3 critical, 5 medium, 3 minor)

---

## Table of Contents

1. [Priority Matrix](#priority-matrix)
2. [Phase 1: Critical Safety & Architecture](#phase-1-critical-safety--architecture)
3. [Phase 2: ECS Pattern Fixes](#phase-2-ecs-pattern-fixes)
4. [Phase 3: Vulkan Abstraction](#phase-3-vulkan-abstraction)
5. [Phase 4: Code Quality](#phase-4-code-quality)
6. [Testing Strategy](#testing-strategy)
7. [Risk Assessment](#risk-assessment)

---

## Priority Matrix

| Issue | Impact | Effort | Priority | Phase |
|-------|--------|--------|----------|-------|
| Dual Transform Storage | High | Medium | **P0** | 2 |
| Unsafe Raw Pointers | Critical | Low | **P0** | 1 |
| Mixed Abstraction Layers | High | High | **P1** | 3 |
| Mesh Builder Anti-Pattern | Medium | Low | **P1** | 2 |
| Camera Wrapper Pattern | Medium | Medium | **P1** | 2 |
| Material Per Mesh | Low | Medium | **P2** | 3 |
| Model/ModelEntity Redundancy | Medium | Low | **P2** | 2 |
| Hardcoded Shaders | Medium | Low | **P2** | 4 |
| Naming Inconsistency | Low | Low | **P3** | 4 |
| GLTF Parsing Complexity | Low | High | **P3** | 4 |
| Unused Parameters | Low | Low | **P3** | 4 |

---

## Phase 1: Critical Safety & Architecture

> **Goal:** Address safety-critical issues and improve architectural foundations
> **Estimated Effort:** 2-3 days
> **Risk:** Low-Medium

### 1.1 Remove Unsafe Raw Pointers (CRITICAL)

**Current Implementation:**

```rust
// application/mod.rs
struct AppRenderCallback {
    world: *mut World,
    camera: Rc<RefCell<Camera>>,
}

unsafe impl Send for AppRenderCallback {}

impl RenderCallback for AppRenderCallback {
    fn render(&mut self, command_buffer: &CommandBuffer, dt: f32) {
        let world = unsafe { &mut *self.world }; // UNSAFE
        // ...
    }
}
```

**Problem:** Undefined behavior if lifetimes violated, difficult to reason about.

**Solution Options:**

#### Option A: Store Index-Based Entity Reference (RECOMMENDED)

```rust
struct AppRenderCallback {
    camera_entity: EntityId,
    drawable_query: QueryState<&mut DrawableComponent>,
}

impl RenderCallback for AppRenderCallback {
    fn render(&mut self, command_buffer: &CommandBuffer, dt: f32, world: &mut World) {
        // Get camera components
        let (transform, perspective) = world
            .get_components::<(TransformComponent, Perspective)>(self.camera_entity)
            .unwrap();

        let view = Self::compute_view_matrix(&transform);
        let proj = Self::compute_proj_matrix(&perspective);

        // Draw entities
        for (_, drawable) in world.query_mut::<&mut DrawableComponent>() {
            drawable.0.update(&view, &proj, dt);
            drawable.0.draw(command_buffer);
        }
    }
}
```

**Pros:** Type-safe, no unsafe, explicit lifetimes
**Cons:** Requires adding `world` parameter to `RenderCallback` trait
**Changes Required:**
- Modify `RenderCallback` trait in `katla_vulkan`
- Update `VulkanRenderer` to pass world to callback
- Update callback implementations

#### Option B: Use Arena/Index-Based ECS Access

```rust
struct AppRenderCallback {
    world_slot: Arc<AtomicUsize>, // Index into World array
}
```

**Pros:** Minimal trait changes
**Cons:** Still complex, less ergonomic
**Verdict:** Use Option A

---

**Implementation Steps:**

1. **Update `RenderCallback` trait** in `katla_vulkan/src/lib.rs`:

```rust
pub trait RenderCallback {
    fn render(&mut self, command_buffer: &CommandBuffer, dt: f32, world: &mut World);
}
```

2. **Update `AppRenderCallback`** in `katla_app/src/application/mod.rs`:

```rust
struct AppRenderCallback {
    camera_entity: EntityId,
}

impl AppRenderCallback {
    fn new(camera_entity: EntityId) -> Self {
        Self { camera_entity }
    }
}

impl RenderCallback for AppRenderCallback {
    fn render(&mut self, command_buffer: &CommandBuffer, dt: f32, world: &mut World) {
        // Get camera components
        let transform = world.get_component::<TransformComponent>(self.camera_entity);
        let perspective = world.get_component::<Perspective>(self.camera_entity);

        let (transform, perspective) = match (transform, perspective) {
            (Some(t), Some(p)) => (t, p),
            _ => return,
        };

        let view = Self::compute_view(&transform.transform);
        let proj = Self::compute_proj(&perspective);

        // Draw all drawable entities
        for (_, drawable) in world.query_mut::<&mut DrawableComponent>() {
            drawable.0.update(&view, &proj, dt);
            drawable.0.draw(command_buffer);
        }
    }
}

impl AppRenderCallback {
    fn compute_view(transform: &katla_math::Transform) -> katla_math::Mat4 {
        let fwd = katla_math::Vec3::new(0.0, 0.0, -1.0);
        let to = katla_math::mat4_mul_vec3(&transform.rotation.make_mat4(), &fwd);
        katla_math::Mat4::create_lookat(
            transform.position,
            transform.position + to,
            katla_math::Vec3::new(0.0, 1.0, 0.0),
        )
    }

    fn compute_proj(perspective: &Perspective) -> katla_math::Mat4 {
        katla_math::Mat4::create_proj(
            perspective.fov,
            perspective.aspect_ratio,
            perspective.near,
            perspective.far,
        )
    }
}
```

3. **Update `VulkanRenderer`** to pass world to callback:

```rust
// In render_frame() method
self.callback
    .as_mut()
    .render(&mut command_buffer, dt, &mut world);
```

4. **Update `Application`** to store camera entity ID:

```rust
pub struct Application {
    // ...
    camera_entity: EntityId,
    // Remove: camera: Rc<RefCell<Camera>>,
}

// In resumed()
let camera_entity = Camera::new(&mut self.world).entity;
self.camera_entity = camera_entity;

// In setup_render_graph()
let callback = AppRenderCallback::new(self.camera_entity);
```

**Testing:**
- [ ] Run fox demo, verify camera controls work
- [ ] Verify all meshes render correctly
- [ ] Test window resize (aspect ratio changes)
- [ ] Run with `miri` to check for undefined behavior

**Rollback Plan:** Keep old implementation in a separate branch, can revert if issues arise.

---

## Phase 2: ECS Pattern Fixes

> **Goal:** Align codebase with proper ECS architecture
> **Estimated Effort:** 3-4 days
> **Risk:** Medium

### 2.1 Fix Dual Transform Storage (HIGH PRIORITY)

**Current Problem:**

```
Model
├─ transform: Transform          ❌ Source #1
└─ DrawableComponent
    └─ Model
       └─ transform: Transform   ❌ Source #2 (duplicate)
```

**Solution:** Remove `transform` from `Model`, use ECS component exclusively.

#### Step 1: Update `Model` struct

**File:** `katla_app/src/application/model.rs`

```rust
// BEFORE
pub struct Model {
    pub meshes: Vec<Mesh>,
    pub material: Material,
    pub transform: Transform,  // ❌ REMOVE
}

// AFTER
pub struct Model {
    pub meshes: Vec<Mesh>,
    pub material: Material,
}

impl Model {
    pub fn new(meshes: Vec<Mesh>, material: Material) -> Self {
        Self {
            meshes,
            material,
        }
    }

    pub fn new_from_gltf(
        model: Rc<GLTFModel>,
        context: Rc<VulkanContext>,
        render_pass: &RenderPass,
        // REMOVE: position: Vec3,
    ) -> Self {
        let material = Material::new(model.clone(), context.clone(), render_pass);
        let mesh = Mesh::new_from_model(model, context.clone());
        Self {
            meshes: vec![mesh],
            material,
        }
    }
}
```

#### Step 2: Update `Model::update()` to accept transform parameter

```rust
// BEFORE
impl Drawable for Model {
    fn update(&mut self, view: &Mat4, proj: &Mat4, _dt: f32) {
        let model = self.transform.make_mat4();  // ❌ Uses internal transform
        self.material.upload_pipeline_data(view.clone(), proj.clone(), model);
    }
}

// AFTER
impl Drawable for Model {
    fn update(&mut self, view: &Mat4, proj: &Mat4, model_matrix: &Mat4) {
        self.material.upload_pipeline_data(view.clone(), proj.clone(), model_matrix.clone());
    }
}
```

#### Step 3: Update render callback to compute model matrix

```rust
// In AppRenderCallback::render()
for (entity, drawable) in world.query_mut::<(&TransformComponent, &mut DrawableComponent)>() {
    let model_matrix = entity.transform.make_mat4();
    drawable.0.update(&view, &proj, &model_matrix);
    drawable.0.draw(command_buffer);
}
```

#### Step 4: Update `Drawable` trait

**File:** `katla_app/src/rendering/drawable.rs`

```rust
// BEFORE
pub trait Drawable {
    fn update(&mut self, view: &Mat4, proj: &Mat4, dt: f32);
    fn draw(&self, command_buffer: &CommandBuffer);
}

// AFTER
pub trait Drawable {
    fn update(&mut self, view: &Mat4, proj: &Mat4, model_matrix: &Mat4);
    fn draw(&self, command_buffer: &CommandBuffer);
}
```

#### Step 5: Update `MeshBuilder`

**File:** `katla_app/src/rendering/mesh/builder.rs`

```rust
// BEFORE
pub fn create_cube(self) -> ModelEntity {
    let position = self.options.position.unwrap_or(Vec3::new(0.0, 0.0, 0.0));
    let transform = Transform {
        position,
        rotation: katla_math::Quat::new(),
        scale: Vec3::new(1.0, 1.0, 1.0),
    };
    let model = Model::new(vec![mesh], material, transform);  // ❌
    ModelEntity::new(self.world, model)
}

// AFTER
pub fn create_cube(self) -> ModelEntity {
    let position = self.options.position.unwrap_or(Vec3::new(0.0, 0.0, 0.0));
    let transform = Transform {
        position,
        rotation: katla_math::Quat::new(),
        scale: Vec3::new(1.0, 1.0, 1.0),
    };
    let model = Model::new(vec![mesh], material);
    ModelEntity::new_with_transform(self.world, model, transform)
}
```

**Testing:**
- [ ] Verify fox model renders at correct position
- [ ] Verify primitive shapes render at correct positions
- [ ] Test transform updates (if any systems modify transforms)
- [ ] Verify camera still works correctly

---

### 2.2 Fix Mesh Builder Anti-Pattern

**Current Problem:** Builder holds `&mut World`, blocking all access during fluent chain.

**Solution:** Defer entity creation until build phase.

#### Step 1: Create `MeshConfig` struct

**File:** `katla_app/src/rendering/mesh/builder.rs`

```rust
pub struct MeshOptions {
    pub size: Option<Vec3>,
    pub radius: Option<f32>,
    pub height: Option<f32>,
    pub segments: Option<u32>,
    pub rings: Option<u32>,
    pub position: Option<Vec3>,
    pub color: Option<[f32; 3]>,
}

pub struct MeshBuilder {
    options: MeshOptions,
    // REMOVE: world: &'a mut World,
    context: Rc<VulkanContext>,
    render_pass: RenderPass,  // Store RenderPass instead of reference
}

impl MeshBuilder {
    pub fn new(
        // REMOVE: world: &'a mut World,
        context: Rc<VulkanContext>,
        render_pass: &RenderPass,  // Take reference, clone internally
    ) -> Self {
        Self {
            options: MeshOptions::default(),
            context,
            render_pass: render_pass.clone(),  // Clone if cheap, or use Rc
        }
    }

    // ... chain methods remain the same ...

    pub fn build(self, world: &mut World) -> ModelEntity {
        // Build happens here, in single call
        let mesh = self.build_mesh();
        let material = self.build_material();
        let transform = self.build_transform();
        let model = Model::new(vec![mesh], material);
        ModelEntity::new_with_transform(world, model, transform)
    }
}

// Usage
let _cube = MeshBuilder::new(renderer.context.clone(), &renderer.render_pass)
    .position(Vec3::new(0.0, 5.0, 0.0))
    .color([1.0, 0.3, 0.3])
    .build(&mut self.world);  // Single mutable borrow
```

**Note:** This requires `RenderPass` to be cloneable or wrapped in `Rc`.

**Alternative:** Keep builder methods but make `build()` take `&mut World`:

```rust
impl MeshBuilder<'a> {
    pub fn build(self, world: &'a mut World) -> ModelEntity {
        // Existing creation logic here
    }
}
```

---

### 2.3 Refactor Camera Pattern ✅ (COMPLETED)

**Status:** COMPLETED (2025-02-05)

**Current Problem:** `Camera` was a wrapper struct with `Rc<RefCell<>>`, not pure ECS.

**Solution Implemented:** Camera is now a pure entity following ECS patterns. The `Camera` wrapper struct still exists but uses entity ID internally.

**Changes:**
- Camera is created as a regular entity with `TransformComponent`, `PerspectiveComponent`, etc.
- Application stores `Rc<RefCell<Camera>>` for convenience but internally uses entity ID
- All camera operations go through the ECS component system

---

### 2.4 Remove Model/ModelEntity Redundancy (TODO)

**Status:** PENDING

**Current Problem:** After removing the `Drawable` trait, `Model` is now just a data holder with minimal functionality, while `ModelEntity` is a factory for creating ECS entities. This creates confusion about which to use and when.

#### Option A: Pure Entity (RECOMMENDED)

```rust
// REMOVE: entities/camera.rs wrapper

// Create camera factory function
pub fn create_camera(world: &mut World, position: Vec3) -> EntityId {
    let entity = world.create_entity();
    let transform = Transform::new_from_position(position);

    add_components!(
        world,
        entity,
        TransformComponent::new(transform),
        VelocityComponent::default(),
        ForceComponent::default(),
        DragComponent::new(0.25),
        Perspective::default(),
        FlyCameraController::default(),
        FlyCameraLook::default(),
    );

    entity
}

// In Application
struct Application {
    camera_entity: EntityId,  // Just store the ID
    // REMOVE: camera: Rc<RefCell<Camera>>,
}

// Initialize
self.camera_entity = create_camera(&mut self.world, Vec3::new(0.0, 50.0, 450.0));

// Aspect ratio change
pub fn update_camera_aspect(&mut self, aspect_ratio: f32) {
    if let Some(perspective) = self.world.get_component_mut::<Perspective>(self.camera_entity) {
        perspective.aspect_ratio = aspect_ratio;
    }
}

// In render callback
let perspective = world.get_component::<Perspective>(self.camera_entity).unwrap();
let proj = Self::compute_proj(&perspective);
```

**Pros:** Simpler, no RefCell, pure ECS
**Cons:** Need helper functions for camera operations

#### Option B: Keep as Subsystem

If keeping `Camera` struct, at least remove `Rc<RefCell<>>`:

```rust
pub struct Camera {
    pub entity: EntityId,
}

impl Camera {
    pub fn aspect_ratio(&self, world: &World) -> f32 {
        world.get_component::<Perspective>(self.entity)
            .map(|p| p.aspect_ratio)
            .unwrap_or(16.0 / 9.0)
    }

    pub fn set_aspect_ratio(&self, world: &mut World, aspect_ratio: f32) {
        if let Some(p) = world.get_component_mut::<Perspective>(self.entity) {
            p.aspect_ratio = aspect_ratio;
        }
    }
}

// In Application
struct Application {
    camera: Camera,  // Not Rc<RefCell<>>
}
```

**Recommendation:** Go with Option A (pure entity) for consistency.

---

### 2.4 Remove Model/ModelEntity Redundancy

**Current Problem:** After removing the `Drawable` trait, `Model` is now just a data holder with minimal functionality, while `ModelEntity` is a factory for creating ECS entities. This creates confusion about which to use and when.

**Current State:**

```rust
// application/model.rs
pub struct Model {
    pub meshes: Vec<Mesh>,
    pub material: Material,
    pub mesh_handle: Option<MeshHandle>,
    pub material_handle: Option<MaterialHandle>,
}

// entities/model.rs
pub struct ModelEntity {
    _entity: EntityId,
}

impl ModelEntity {
    pub fn new_with_renderer(
        world: &mut World,
        mut model: Model,
        renderer: Option<&mut VulkanRenderer>,
        transform: Transform,
    ) -> Self { ... }
}
```

**Issues:**
1. `Model` no longer has any behavior (no `draw()`, no `update()`)
2. `ModelEntity` is just a factory that returns `Self` but wraps an entity ID
3. The naming suggests inheritance/ownership but they're actually separate concerns
4. Users must understand when to use `Model` vs `ModelEntity`

**Solution Options:**

#### Option A: Merge into Single Factory Function (RECOMMENDED)

```rust
// Remove ModelEntity struct entirely
// Keep Model as a pure data struct for passing around

// entities/model.rs
pub fn create_model_entity(
    world: &mut World,
    model: Model,
    renderer: Option<&mut VulkanRenderer>,
    transform: Transform,
) -> EntityId {
    let entity = world.create_entity();

    // Register assets
    let (mesh_handle, material_handle) = if let Some(r) = renderer {
        let mesh_h = r.register_mesh(...);
        let mat_h = r.create_material(...);
        (Some(mesh_h), Some(mat_h))
    } else {
        (Some(MeshHandle(0)), Some(MaterialHandle(0)))
    };

    world.add_component(entity, TransformComponent::new(transform));
    world.add_component(entity, DrawableComponent::with_handles(
        mesh_handle.unwrap(),
        material_handle.unwrap(),
    ));
    world.add_component(entity, NameComponent::new("Model"));

    entity
}

// Usage
let model = Model::new(meshes, material);
let entity_id = create_model_entity(&mut world, model, Some(&mut renderer), transform);
```

**Pros:**
- Clear separation: `Model` is data, function creates entity
- No confusing wrapper struct
- Returns actual `EntityId` like other ECS functions
- Consistent with `create_camera()` pattern

**Cons:**
- Need to update all call sites

#### Option B: Keep ModelEntity but Clarify Purpose

```rust
pub struct ModelEntity(pub EntityId);

impl ModelEntity {
    /// Create a new model entity from a Model.
    /// The Model is consumed during registration.
    pub fn from_model(
        world: &mut World,
        model: Model,
        renderer: Option<&mut VulkanRenderer>,
        transform: Transform,
    ) -> Self {
        // ... implementation
    }

    /// Get the underlying entity ID
    pub fn id(&self) -> EntityId {
        self.0
    }
}
```

**Pros:**
- Less changes to existing code
- Still provides type safety (can't mix up entity IDs)

**Cons:**
- Still a wrapper with minimal value
- Newtype pattern might be overkill here

#### Option C: Use Builder Pattern on Model

```rust
impl Model {
    pub fn spawn(
        self,
        world: &mut World,
        renderer: Option<&mut VulkanRenderer>,
        transform: Transform,
    ) -> EntityId {
        // Register and create entity
        // Returns entity ID
    }
}

// Usage
let entity = Model::new(meshes, material)
    .spawn(&mut world, Some(&mut renderer), transform);
```

**Pros:**
- Fluent API
- Clear ownership (Model is consumed)
- No separate entity type needed

**Cons:**
- Model needs to know about World/Renderer (tighter coupling)

**Recommendation:** **Option A** (merge into single factory function) for clarity and consistency with ECS patterns.

**Files to update:**
- `entities/model.rs` - Remove `ModelEntity` struct, replace with `create_model_entity()` function
- `application/mod.rs` - Update all `ModelEntity::new_with_renderer()` calls
- `components/` - Consider if other entity types should follow same pattern

---

## Phase 3: Vulkan Abstraction

> **Goal:** Improve separation between application and Vulkan layers
> **Estimated Effort:** 4-5 days
> **Risk:** Medium-High

### 3.1 Separate High-Level and Low-Level Drawing ✅ (COMPLETED)

**Status:** COMPLETED (2025-02-05)

**Current Problem:** `Drawable` trait mixed high-level logic with low-level Vulkan.

**Solution Implemented:**
- Removed the `Drawable` trait entirely from `rendering/drawable.rs`
- Removed `impl Drawable for Model` (both `update()` and `draw()` methods)
- Updated `DrawableComponent` to remove `drawable: Box<dyn Drawable>` field
- Rendering now uses high-level `DrawCall`/`DrawList` system with mesh/material handles
- Low-level `CommandBuffer` operations are handled internally in render graph execution

**Benefits:**
- Clean separation: Application layer uses handles, render graph handles Vulkan commands
- No more leaky abstraction exposing `CommandBuffer` to application code
- Eliminated unnecessary boxing of `Model` objects
- Simpler architecture with clearer boundaries

**Previous Options (Not Used):**

#### Option A: Split Traits

```rust
/// High-level rendering interface
pub trait Renderable {
    fn get_meshes(&self) -> &[Mesh];
    fn get_material(&self) -> &Material;
    fn get_transform(&self) -> &Transform;
}

/// Low-level Vulkan drawing (internal to katla_vulkan)
pub trait VulkanDrawable {
    fn record_draw_commands(&self, command_buffer: &CommandBuffer);
}

// Implementation
impl Renderable for Model {
    fn get_meshes(&self) -> &[Mesh] {
        &self.meshes
    }

    fn get_material(&self) -> &Material {
        &self.material
    }

    fn get_transform(&self) -> &Transform {
        &self.transform
    }
}

// Renderer uses high-level interface
fn render_entity<R: Renderable>(
    entity: &R,
    view: &Mat4,
    proj: &Mat4,
    transform: &Mat4,
    command_buffer: &mut CommandBuffer,
) {
    entity.get_material().upload_uniforms(view, proj, transform);
    entity.get_material().bind_pipeline(command_buffer);

    for mesh in entity.get_meshes() {
        mesh.record_draw_commands(command_buffer);
    }
}
```

#### Option B: Draw Context (RECOMMENDED)

```rust
/// Abstraction over command buffer
pub struct DrawContext<'a> {
    command_buffer: &'a CommandBuffer,
    // Could add more abstraction layers here
}

impl<'a> DrawContext<'a> {
    pub fn bind_pipeline(&mut self, pipeline: &MaterialPipeline) {
        pipeline.bind(self.command_buffer.vk_command_buffer());
    }

    pub fn set_uniforms(&mut self, view: &Mat4, proj: &Mat4, model: &Mat4) {
        // Abstract away uniform buffer updates
    }

    pub fn draw_mesh(&mut self, mesh: &Mesh) {
        mesh.draw(self.command_buffer);
    }
}

/// Updated Drawable trait
pub trait Drawable {
    fn render(&self, context: &mut DrawContext, view: &Mat4, proj: &Mat4, model: &Mat4);
}

impl Drawable for Model {
    fn render(&self, ctx: &mut DrawContext, view: &Mat4, proj: &Mat4, model: &Mat4) {
        ctx.set_uniforms(view, proj, model);
        ctx.bind_pipeline(&self.material.material_pipeline);

        for mesh in &self.meshes {
            ctx.draw_mesh(mesh);
        }
    }
}
```

**Pros:** Better abstraction, easier to test, can swap backends
**Cons:** More upfront work
**Recommendation:** Option B for cleaner separation

---

### 3.2 Material Sharing System ✅ (COMPLETED)

**Status:** COMPLETED (2025-02-05)

**Current Problem:** Each `Model` owned its own `Material`, preventing sharing.

**Solution Implemented:**
- Created `MaterialManager` with name-based material registration
- Added `create_checkerboard_material()` helper function
- All primitive shapes now share the same "checkerboard" material
- Materials use `Rc<RefCell<MaterialPipeline>>` for cheap cloning
- Proper cleanup on shutdown via `MaterialManager::destroy()`

**Changes:**
- New file: `rendering/material_manager.rs`
- New file: `rendering/material_helpers.rs`
- Updated `MeshBuilder` to support optional shared materials
- All meshes in application use `.with_shared_material("checkerboard")`

---

### 3.3 [Next Vulkan Abstraction Item]

#### Step 1: Create Material Manager

```rust
// rendering/material_manager.rs
use std::collections::HashMap;
use std::sync::Arc;
use katla_vulkan::VulkanContext;

pub struct MaterialId(pub usize);

pub struct MaterialManager {
    materials: Vec<Material>,
    by_name: HashMap<String, MaterialId>,
}

impl MaterialManager {
    pub fn new() -> Self {
        Self {
            materials: Vec::new(),
            by_name: HashMap::new(),
        }
    }

    pub fn create_material(
        &mut self,
        name: impl Into<String>,
        context: Rc<VulkanContext>,
        render_pass: &RenderPass,
        config: MaterialConfig,
    ) -> MaterialId {
        let name = name.into();
        let material = Material::new(context, render_pass, config);
        let id = MaterialId(self.materials.len());
        self.materials.push(material);
        self.by_name.insert(name, id);
        id
    }

    pub fn get(&self, id: MaterialId) -> Option<&Material> {
        self.materials.get(id.0)
    }

    pub fn get_mut(&mut self, id: MaterialId) -> Option<&mut Material> {
        self.materials.get_mut(id.0)
    }

    pub fn get_by_name(&self, name: &str) -> Option<MaterialId> {
        self.by_name.get(name).copied()
    }
}

pub struct MaterialConfig {
    pub vertex_shader: Vec<u8>,
    pub fragment_shader: Vec<u8>,
    pub texture: Option<Arc<Texture>>,
    pub depth_test: bool,
    pub depth_write: bool,
    pub backface_culling: bool,
}
```

#### Step 2: Update Model to use MaterialId

```rust
pub struct Model {
    pub meshes: Vec<Mesh>,
    pub material_id: MaterialId,  // Instead of Material
}

impl Model {
    pub fn new(meshes: Vec<Mesh>, material_id: MaterialId) -> Self {
        Self { meshes, material_id }
    }

    pub fn get_material(&self) -> Option<&Material> {
        // MaterialManager would be stored in World or passed in
        world.get_resource::<MaterialManager>()
            .and_then(|m| m.get(self.material_id))
    }
}
```

#### Step 3: Store MaterialManager in World

```rust
// In application setup
world.add_resource(MaterialManager::new());

// Create shared materials
let checkerboard_id = world
    .get_resource_mut::<MaterialManager>()
    .create_material(
        "checkerboard",
        context.clone(),
        &render_pass,
        MaterialConfig {
            vertex_shader: include_bytes!("../shaders/model_pbr.vert.spv").to_vec(),
            fragment_shader: include_bytes!("../shaders/model.frag.spv").to_vec(),
            texture: Some(checkerboard_texture),
            depth_test: true,
            depth_write: true,
            backface_culling: true,
        },
    );

// Use material for multiple meshes
let cube = Model::new(cube_mesh, checkerboard_id);
let sphere = Model::new(sphere_mesh, checkerboard_id);  // Shared!
```

---

## Phase 4: Code Quality

> **Goal:** Fix minor issues and improve maintainability
> **Estimated Effort:** 1-2 days
> **Risk:** Low

### 4.1 Consistent Component Naming ✅ (COMPLETED)

**Status:** COMPLETED (2025-02-05)

**Changes:**
- Renamed `Perspective` → `PerspectiveComponent`
- Renamed `FlyCameraController` → `FlyCameraControllerComponent`
- Renamed `FlyCameraLook` → `FlyCameraLookComponent`
- Updated all imports and usages throughout codebase

---

### 4.2 Fix Hardcoded Shader Paths ✅ (COMPLETED)

**Status:** COMPLETED (2025-02-05)

**Solution Implemented:**
- Created `ShaderRegistry` in `rendering/shader_registry.rs`
- Centralized shader loading with `include_bytes!()`
- Updated `MeshBuilder` to use `ShaderRegistry`
- All shaders now loaded through registry with named lookup

---

### 4.3 Remove Unused Parameters ✅ (COMPLETED)

**Status:** COMPLETED (2025-02-05)

**Changes:**
- Removed `_dt` parameter from `render_with_render_graph()`
- All unused parameters have been cleaned up

---

### 4.4 Simplify GLTF Parsing (OPTIONAL)

This is lower priority as the code works, but consider:

1. Use `gltf` crate's accessor iterators more
2. Extract buffer parsing into separate module
3. Add comprehensive unit tests for parsing

---

### 4.5 Code Quality Improvements ✅ (COMPLETED)

**Status:** COMPLETED (2025-02-05)

**Clippy/Warning Fixes:**
- Fixed redundant closures → use tuple variants directly
- Removed useless `.into()` conversions
- Fixed manual slice size calculations → use `size_of_val()`
- Simplified match expressions → use `matches!()` macro
- Changed `&Vec<T>` → `&[T]` for better API
- Fixed manual flatten → use `.flatten()`
- Fixed `expect` with `format!()` → use `unwrap_or_else(|| panic!())`
- Removed unused imports and variables
- Centralized checkerboard texture generation in `material_helpers.rs` (removed duplication)

// AFTER
fn update(&mut self, view: &Mat4, proj: &Mat4) {
    // Removed _dt
}
```

---

### 4.4 Simplify GLTF Parsing (OPTIONAL)

This is lower priority as the code works, but consider:

1. Use `gltf` crate's accessor iterators more
2. Extract buffer parsing into separate module
3. Add comprehensive unit tests for parsing

---

## Testing Strategy

### Unit Tests

```rust
// rendering/material_manager.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_material_sharing() {
        let mut manager = MaterialManager::new();
        let id1 = manager.create_material("test", ...);
        let id2 = manager.get_by_name("test").unwrap();

        assert_eq!(id1.0, id2.0);
        assert!(std::ptr::eq(manager.get(id1).unwrap(), manager.get(id2).unwrap()));
    }
}

// components/transform.rs
#[test]
fn test_transform_component() {
    let transform = Transform::new_from_position(Vec3::new(1.0, 2.0, 3.0));
    let component = TransformComponent::new(transform);

    assert_eq!(component.transform.position, Vec3::new(1.0, 2.0, 3.0));
}
```

### Integration Tests

```rust
#[test]
fn test_model_entity_rendering() {
    let mut world = World::new();
    let entity = create_cube_entity(&mut world, ...);

    // Verify components exist
    assert!(world.get_component::<TransformComponent>(entity).is_some());
    assert!(world.get_component::<DrawableComponent>(entity).is_some());
}
```

### Regression Tests

Before each phase:
1. Run demo application
2. Verify fox model renders
3. Verify all primitive shapes render
4. Test camera controls
5. Test window resize

After changes:
1. Run same tests
2. Compare output/screenshots
3. Performance benchmarks

---

## Risk Assessment

| Phase | Risk | Mitigation |
|-------|------|------------|
| Phase 1 | Low | Isolated change, easy rollback |
| Phase 2 | Medium | Core data structure changes, affects all entities |
| Phase 3 | High | API changes across katla_vulkan boundary |
| Phase 4 | Low | Cosmetic changes, low impact |

### Rollback Strategy

1. **Git branches:** One branch per phase
2. **Feature flags:** Conditional compilation for gradual rollout
3. **Testing:** Comprehensive test suite before merging
4. **Documentation:** Keep this document updated with actual changes

---

## Execution Order

### Week 1: Phase 1
- Day 1: Implement Option A for raw pointers
- Day 2: Update RenderCallback trait
- Day 3: Testing and validation

### Week 2-3: Phase 2
- Day 4-5: Fix dual transform storage
- Day 6-7: Refactor MeshBuilder
- Day 8-9: Refactor Camera pattern
- Day 10: Testing and integration

### Week 4-5: Phase 3
- Day 11-14: Implement Drawable abstraction
- Day 15-17: Material sharing system
- Day 18-20: Testing and performance validation

### Week 6: Phase 4 + Buffer
- Day 21-22: Code quality improvements
- Day 23-24: Final testing and documentation
- Day 25: Buffer for unexpected issues

---

## Success Criteria

Phase complete when:
- [ ] All tests pass
- [ ] Demo application runs without regressions
- [ ] No compiler warnings (except allowed ones)
- [ ] Code review approved
- [ ] Documentation updated
- [ ] Performance within 10% of baseline

---

## Appendix: File Changes Summary

### High Priority Files

| File | Changes | Phase |
|------|---------|-------|
| `application/mod.rs` | Remove raw pointers, update callback | 1 |
| `application/model.rs` | Remove transform field | 2 |
| `rendering/drawable.rs` | Update trait signature | 2, 3 |
| `rendering/mesh/builder.rs` | Fix builder pattern | 2 |
| `entities/camera.rs` | Remove wrapper or simplify | 2 |
| `components/perspective.rs` | Rename to PerspectiveComponent | 4 |
| `components/fly_camera.rs` | Add Component suffix | 4 |

### New Files to Create

- `rendering/material_manager.rs`
- `rendering/shader_registry.rs`
- `rendering/draw_context.rs`

---

## Questions & Decisions Needed

1. **RenderCallback trait change:** Requires coordination with `katla_vulkan`. Get approval first.
2. **RenderPass clone:** `MeshBuilder` needs `RenderPass` to be cloneable or `Rc`. Current status?
3. **Material cache location:** Should it be in World or a separate manager?
4. **Shader loading:** Runtime loading vs compile-time include_bytes?
5. **GLTF parsing:** Keep as-is or invest in refactoring?

---

**Last Updated:** 2025-02-05
**Next Review:** After Phase 1 completion
