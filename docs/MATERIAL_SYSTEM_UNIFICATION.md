# Material System Unification Plan

## Executive Summary

This document outlines a plan to unify Katla's material system into a single, cohesive API. The goal is to have **one way to create materials** throughout the workspace, removing all legacy codepaths.

---

## Part 1: Research Summary

### How Other Engines Approach Materials

#### Unreal Engine
- **Abstraction**: High-level node-based materials
- **Pattern**: Material Editor → HLSL compilation → shader permutations
- **Key Features**:
  - Material domains (Surface, UI, Post-Process, Particle)
  - Automatic shader permutation system
  - FShaderParameter binding system
  - Derivative Data Cache for compiled shaders

#### Unity SRP
- **Abstraction**: Mid-level with ShaderLab DSL
- **Pattern**: ShaderLab declarations + HLSL code
- **Key Features**:
  - SRP Batcher (batches by shader variant)
  - `UnityPerDraw` and `UnityPerMaterial` CBUFFER convention
  - Material Property Blocks for per-object overrides

#### Bevy Engine (Rust)
- **Abstraction**: Mid-level with trait-based materials
- **Pattern**: `Material` trait + `AsBindGroup` derive macro
- **Key Features**:
  ```rust
  #[derive(AsBindGroup)]
  struct CustomMaterial {
      #[uniform(0)]
      color: Vec4,
      #[texture(1)]
      texture: Handle<Image>,
  }

  impl Material for CustomMaterial {
      fn fragment_shader() -> ShaderRef { ... }
      fn specialize(&self, descriptor: &mut RenderPipelineDescriptor) { ... }
  }
  ```
  - Automatic bind group layout generation
  - Pipeline specialization callbacks
  - Bindless support with derive macro

#### wgpu
- **Abstraction**: Low-level GPU API
- **Pattern**: Explicit bind group layouts and pipeline creation
- **Key Features**:
  - Manual descriptor management
  - Hierarchical bind groups (per-frame, per-material, per-object)
  - Layout reuse and caching

#### Godot 4
- **Abstraction**: Server-based with shader abstraction
- **Pattern**: Shader types (`shader_type spatial`, `canvas_item`, `particles`)
- **Key Features**:
  - RenderingServer with RID handles
  - Shader type system for different use cases
  - Multiple renderer backends (Forward+, Mobile, Compatibility)

### Key Patterns to Adopt

| Pattern | Source | Benefit |
|---------|--------|---------|
| Trait-based materials | Bevy | Type-safe, idiomatic Rust |
| Derive macro for bindings | Bevy | Automatic layout generation |
| Shader specialization | Bevy/Unreal | Dynamic pipeline variants |
| Bind group hierarchy | wgpu/Unity | Efficient descriptor updates |
| Pipeline caching | Unreal/Bevy | Avoid recompilation |
| Material templates | Unity/Unreal | Data-driven materials |

---

## Part 2: Current State Analysis

### Current Material Creation Paths

| Path | Location | Method | Status |
|------|----------|--------|--------|
| PBR Materials | `material.rs` | `Material::new()` | **LEGACY** |
| Bindless PBR | `material_helpers.rs` | `MaterialBuilder::build_bindless()` | Modern |
| Template-based | `material.rs` | `Material::from_template_*()` | Modern |
| UI Materials | `ui_material.rs` | `MaterialBuilder` + UI layout | Modern |
| Sky Material | `sky_material.rs` | `build_with_storage()` | Modern |
| Grid Material | `grid_material.rs` | `build_with_storage()` | Modern |
| Gizmo Material | `gizmo_material.rs` | `build_with_storage()` | Modern |
| Particle Materials | `particle_emitter.rs` | Direct `PipelineBuilder` | **SPECIAL CASE** |

### Current Descriptor Set Patterns

| Pattern | Sets | Description |
|---------|------|-------------|
| Legacy | 1 | uniform + image + sampler |
| Storage | 2 | frame_data/objects + textures |
| Storage Skinned | 3 | + joint matrices |
| Bindless | 2 | frame_data/objects + bindless array |
| Bindless Skinned | 3 | + joint matrices |
| UI | 2 | static + push descriptor |
| Particle | 2 | frame_data/objects + particle buffer |

### MaterialBuilder Build Methods (7 total!)

1. `build()` - Legacy uniform buffer **[DEPRECATED]**
2. `build_with_storage()` - Standard storage buffers
3. `build_with_storage_pbr()` - Full PBR (5 textures)
4. `build_with_storage_skinned()` - + skeletal animation
5. `build_bindless()` - Bindless textures
6. `build_bindless_skinned()` - Bindless + skeletal
7. `build_with_desc_layout()` - Hot reload internal

### Legacy Code to Remove

1. **`MaterialBuilder::build()`** - Uses UNIFORM_BUFFER, no instance indexing
2. **`Material::new()` from GLTF** - Uses legacy `build()`
3. **`MaterialRegistry::load_directory()`** - Non-storage mode
4. **Single-set descriptor layouts** - Old pattern

---

## Part 3: Proposed Unified API

### Design Goals

1. **Single material creation path** for all material types
2. **Type-safe** descriptor layouts via traits
3. **Automatic** bind group management
4. **Hot reload** support built-in
5. **Extensible** for special cases (particles, compute)

### Proposed Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Material Trait                           │
│  - fragment_shader() / vertex_shader()                      │
│  - bind_group_layout() -> MaterialLayout                    │
│  - specialize(&mut PipelineDescriptor)                      │
│  - create_bind_group(resources) -> BindGroup                │
└─────────────────────────────────────────────────────────────┘
                              │
        ┌─────────────────────┼─────────────────────┐
        │                     │                     │
        ▼                     ▼                     ▼
┌───────────────┐    ┌───────────────┐    ┌───────────────┐
│ PbrMaterial   │    │ UIMaterial    │    │ ParticleMat   │
│ - base_color  │    │ - font_atlas  │    │ - particle_buf│
│ - metallic    │    │ - sampler     │    │ - blend_mode  │
│ - roughness   │    │ - uniforms    │    │               │
│ - textures[]  │    │               │    │               │
└───────────────┘    └───────────────┘    └───────────────┘
        │                     │                     │
        └─────────────────────┼─────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    MaterialPipeline                         │
│  - Shared pipeline cache                                     │
│  - Automatic layout generation                               │
│  - Hot reload support                                        │
└─────────────────────────────────────────────────────────────┘
```

### Proposed API

```rust
/// Core material abstraction
pub trait Material: Send + Sync + 'static {
    /// Shader reference (path or precompiled)
    type ShaderSource: Into<ShaderRef>;

    /// Fragment shader
    fn fragment_shader(&self) -> ShaderRef;

    /// Vertex shader (optional, defaults to standard)
    fn vertex_shader(&self) -> Option<ShaderRef> { None }

    /// Define the bind group layout for this material
    fn layout() -> MaterialLayout where Self: Sized;

    /// Specialize the pipeline based on material configuration
    fn specialize(
        &self,
        descriptor: &mut RenderPipelineDescriptor,
        key: &MaterialKey,
    ) -> Result<(), SpecializationError> {
        Ok(())
    }

    /// Create bind groups for rendering
    fn bind_groups(
        &self,
        device: &Device,
        layout: &MaterialLayout,
        frame_resources: &FrameResources,
    ) -> Vec<BindGroup>;
}

/// Automatic bind group layout generation via derive macro
#[derive(Material, AsBindGroup)]
#[material(domain = "spatial")]  // spatial, ui, particle, post_process
pub struct PbrMaterial {
    // Set 0: Frame data (automatic)

    // Set 1: Material parameters
    #[uniform(1, binding = 0)]
    pub base_color: Vec4,

    #[uniform(1, binding = 0, offset = 16)]
    pub metallic: f32,

    #[uniform(1, binding = 0, offset = 20)]
    pub roughness: f32,

    // Bindless textures (indices into global array)
    #[bindless_texture(1, binding = 1)]
    pub base_color_texture: Option<TextureIndex>,

    #[bindless_texture(1, binding = 2)]
    pub normal_texture: Option<TextureIndex>,

    #[bindless_texture(1, binding = 3)]
    pub metallic_roughness_texture: Option<TextureIndex>,

    // Render state
    #[blend_mode]
    pub blend_mode: BlendMode,

    #[cull_mode]
    pub cull_mode: CullMode,
}

/// UI Material with push descriptors
#[derive(Material, AsBindGroup)]
#[material(domain = "ui", push_descriptors = true)]
pub struct UIMaterial {
    #[sampled_image(0, binding = 0)]
    pub font_atlas: TextureHandle,

    #[sampler(0, binding = 1)]
    pub sampler: SamplerHandle,

    #[uniform(0, binding = 3)]
    pub screen_size: Vec4,
}

/// Particle material with custom storage buffer
#[derive(Material, AsBindGroup)]
#[material(domain = "particle")]
pub struct ParticleMaterial {
    // Set 0: Frame data (automatic)

    // Set 1: Particle data
    #[storage_buffer(1, binding = 0, read_only = true)]
    pub particle_buffer: BufferHandle,

    #[blend_mode(additive)]
    pub _blend: (),  // Compile-time blend mode
}
```

### Material Creation

```rust
// Create material from code
let material = PbrMaterial {
    base_color: Vec4::new(1.0, 0.0, 0.0, 1.0),
    metallic: 0.5,
    roughness: 0.5,
    base_color_texture: Some(texture_index),
    ..Default::default()
};

// Create material from TOML template
let material = MaterialRegistry::load::<PbrMaterial>("materials/metal.toml")?;

// Create specialized material
let pipeline = MaterialPipeline::new::<PbrMaterial>(context, &layout_cache)?;
```

### Hot Reload Integration

```rust
// Materials automatically reload when shaders change
impl MaterialRegistry {
    pub fn watch_directory(&mut self, path: &Path) {
        // File watcher integration
    }

    pub fn check_reload(&mut self) -> Vec<MaterialReloadEvent> {
        // Check for shader/material file changes
    }
}
```

---

## Part 4: Implementation Plan

### Phase 1: Core Trait Infrastructure

**Goal**: Define the core `Material` trait and `MaterialLayout` types.

**Files to Create/Modify**:
- `katla_vulkan/src/material/mod.rs` - Core trait definitions
- `katla_vulkan/src/material/layout.rs` - MaterialLayout types
- `katla_vulkan/src/material/cache.rs` - Pipeline cache

**Tasks**:
1. Define `Material` trait
2. Define `MaterialLayout` for describing bind groups
3. Define `MaterialKey` for pipeline specialization
4. Create `PipelineCache` for caching compiled pipelines

**Blockers**: None

---

### Phase 2: Derive Macro

**Goal**: Create `#[derive(Material)]` and `#[derive(AsBindGroup)]` macros.

**Files to Create/Modify**:
- `katla_derive/src/material.rs` - Material derive macro
- `katla_derive/src/bind_group.rs` - AsBindGroup derive macro
- `katla_derive/src/lib.rs` - Export macros

**Tasks**:
1. Parse struct attributes for bind group layout
2. Generate `Material::layout()` implementation
3. Generate `Material::bind_groups()` implementation
4. Handle optional fields and default values

**Blockers**:
- Requires proc-macro expertise
- Need to handle complex attribute parsing

---

### Phase 3: Migrate Existing Materials

**Goal**: Convert all existing materials to use the new trait system.

**Materials to Migrate**:
1. PbrMaterial (from template system)
2. SkyMaterial
3. GridMaterial
4. GizmoMaterial
5. UIMaterial
6. ParticleMaterial

**Tasks**:
1. Create new material structs implementing `Material` trait
2. Update material creation code in katla_app
3. Remove old MaterialBuilder paths
4. Test each material type

**Blockers**:
- Particle system uses compute + graphics pipelines (special case)
- UI uses push descriptors (needs special handling)

---

### Phase 4: Remove Legacy Code

**Goal**: Delete all deprecated material creation paths.

**Files to Delete/Modify**:
- `MaterialBuilder::build()` - Remove
- `MaterialBuilder::build_with_storage()` - Remove (replaced by trait)
- `MaterialBuilder::build_bindless()` - Remove (replaced by trait)
- All 7 build methods - Consolidate to single path
- `Material::new()` legacy path - Remove

**Tasks**:
1. Identify all legacy code paths
2. Migrate remaining usages
3. Delete deprecated methods
4. Update documentation

**Blockers**:
- Need to ensure all materials migrated first
- Asset loading pipeline may need updates

---

### Phase 5: Template System Integration

**Goal**: Integrate TOML-based templates with the new trait system.

**Tasks**:
1. Update `MaterialDescriptor` to work with derive macro
2. Create `MaterialTemplate::from_descriptor::<T>()`
3. Support hot reload with new system
4. Update `MaterialRegistry` API

**Blockers**:
- Need derive macro working first

---

## Part 5: Blockers and Dependencies

### Technical Blockers

| Blocker | Description | Mitigation |
|---------|-------------|------------|
| Derive Macro Complexity | `AsBindGroup` macro is complex | Start simple, iterate |
| Push Descriptors | UI uses push descriptors | Special case in trait |
| Compute Pipelines | Particle system needs compute | Separate `ComputeMaterial` trait |
| Bindless Integration | Bindless manager coupling | Make bindless a material feature |
| Hot Reload | Hot reload depends on layouts | Layout stability in trait |

### Dependencies

```
Phase 1 (Traits)
    │
    ├── Phase 2 (Derive Macro) ──┐
    │                            │
    ▼                            ▼
Phase 3 (Migrate) ◄──────── Phase 5 (Templates)
    │
    ▼
Phase 4 (Remove Legacy)
```

---

## Part 6: Success Criteria

### Must Have
- [ ] Single `Material` trait for all material types
- [ ] All existing materials migrated to new system
- [ ] Legacy `MaterialBuilder` build methods removed
- [ ] No deprecated codepaths in katla_app

### Should Have
- [ ] Derive macro for automatic layout generation
- [ ] Pipeline caching with specialization keys
- [ ] Hot reload support

### Nice to Have
- [ ] Material validation at compile time
- [ ] Automatic uniform buffer layout generation
- [ ] Visual material editor integration

---

## Part 7: Timeline Estimate

| Phase | Duration | Description |
|-------|----------|-------------|
| Phase 1 | 2-3 days | Core trait infrastructure |
| Phase 2 | 3-5 days | Derive macro development |
| Phase 3 | 3-4 days | Migrate existing materials |
| Phase 4 | 1-2 days | Remove legacy code |
| Phase 5 | 2-3 days | Template system integration |
| **Total** | **11-17 days** | |

---

## Appendix A: Current File Structure

```
katla_vulkan/src/vulkan/material/
├── mod.rs              # MaterialPipeline, exports
├── buffer_descriptor.rs # UniformBuffer, BufferDescriptorSource
├── materialbuilder.rs  # MaterialBuilder (7 build methods!)
├── template.rs         # MaterialTemplate
├── registry.rs         # MaterialRegistry
├── shadermodule.rs     # ShaderModule, WGSL compilation
├── reflection.rs       # naga-based reflection
├── parameters.rs       # MaterialParameters
├── uniform_layout.rs   # UniformLayout
├── storage_uniform.rs  # StorageDescriptorSet
├── skeleton_descriptor.rs # SkeletonDescriptorSet
└── compute_pipeline.rs # ComputePipeline

katla_app/src/rendering/
├── material.rs         # Material wrapper (legacy + template paths)
├── material_helpers.rs # Helper functions
├── sky_material.rs     # SkyMaterial
├── grid_material.rs    # GridMaterial
├── gizmo_material.rs   # GizmoMaterial
└── ui_material.rs      # UIMaterial
```

## Appendix B: Proposed File Structure

```
katla_vulkan/src/material/
├── mod.rs              # Material trait, exports
├── trait.rs            # Material trait definition
├── layout.rs           # MaterialLayout, BindingType
├── cache.rs            # PipelineCache
├── specialize.rs       # MaterialKey, specialization
├── bind_group.rs       # BindGroup helpers
├── template.rs         # MaterialTemplate (updated)
├── registry.rs         # MaterialRegistry (updated)
└── builtins/
    ├── mod.rs
    ├── pbr.rs          # PbrMaterial
    ├── ui.rs           # UIMaterial
    ├── sky.rs          # SkyMaterial
    ├── grid.rs         # GridMaterial
    └── particle.rs     # ParticleMaterial

katla_derive/src/
├── lib.rs
├── material.rs         # #[derive(Material)]
└── bind_group.rs       # #[derive(AsBindGroup)]
```
