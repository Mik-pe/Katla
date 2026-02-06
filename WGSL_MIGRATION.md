# Plan: Migrate GLSL Shaders to WGSL and Cleanup

## Context

The codebase currently uses two GLSL shaders (compiled to SPIR-V) that are embedded at compile time:
- `model_pbr.vert` - PBR vertex shader with world/view/proj transforms
- `model.frag` - Fragment shader that samples albedo texture

A newer material asset system already exists that loads WGSL shaders at runtime with hot-reload support via TOML files. To standardize on WGSL and simplify the build process, we need to translate the in-use GLSL shaders to WGSL and remove all compiled `.spv` files.

**Note:** The `model_pbr.vert` shader has a bug on line 28 (`vs_pos = vs_pos;` is a no-op) and line 29 remaps normals to 0.5-1.0 range which may be intentional or a bug. The WGSL translation will preserve the current behavior.

## In-Use Shaders

**Currently in use:**
1. `model_pbr.vert.spv` - loaded in `shader_registry.rs:12` and `material.rs:60`
2. `model.frag.spv` - loaded in `shader_registry.rs:16` and `material.rs:63`

**Not in use (can be deleted):**
- `gui.vert`, `gui.frag` (and .spv versions)
- `model_pos.vert`, `model_norm.vert` (and .spv versions)
- `model_no_tex.frag` (.spv)
- `model.wgsl` (exists but unused)

## Implementation Steps

### 1. Create WGSL Shader for Model PBR

**File:** `resources/shaders/model_pbr.wgsl`

Translate the combined GLSL vertex and fragment shaders to WGSL:
- Preserve the current behavior (including the normal remapping on line 29)
- Use proper WGSL syntax following the pattern from `colored_mesh.wgsl`
- Structure with `@vertex` and `@fragment` entry points

**Key attributes to translate:**
```glsl
// GLSL inputs
layout(location=0) in vec3 position;
layout(location=1) in vec3 normal;
layout(location=2) in vec4 vert_tangent;
layout(location=3) in vec2 vert_texcoord0;

// Uniforms
layout(set = 0, binding = 0) uniform Data {
    mat4 world;
    mat4 view;
    mat4 proj;
} uniforms;

// Sampler
layout(binding=1) uniform sampler2D albedo_sampler;
```

### 2. Create Material Definition

**File:** `resources/materials/model_pbr.toml`

Define the material using the TOML format (similar to `colored_mesh.toml`):
- Reference the new WGSL shader
- Define uniform bindings (3 mat4s: world, view, proj)
- Define texture binding (albedo_sampler at binding 1)
- Set render state (depth_test=true, depth_write=true, cull_backfaces=true)

### 3. Update Material Loading Code

**Files:** `katla_app/src/rendering/material.rs`, `katla_app/src/rendering/material_helpers.rs`, `katla_app/src/rendering/mesh/builder.rs`

The `MaterialBuilder` already supports WGSL loading via `with_wgsl_shader(path)` (materialbuilder.rs:170-188). The `create_colored_checkerboard_material` function already demonstrates this pattern (material_helpers.rs:112-115).

**3a. Update `material.rs:58-66`:**
```rust
// Replace this:
let mut builder = MaterialBuilder::new(context.clone())
    .with_vertex_binding(vertex_binding.clone())
    .with_vertex_shader(include_bytes!("../../../resources/shaders/model_pbr.vert.spv"))
    .with_fragment_shader(include_bytes!("../../../resources/shaders/model.frag.spv"))
    .with_backface_culling(true)
    .with_depth_test(true)
    .with_depth_write(true);

// With this:
use std::path::Path;
let mut builder = MaterialBuilder::new(context.clone())
    .with_vertex_binding(vertex_binding.clone())
    .with_wgsl_shader(Path::new("resources/shaders/model_pbr.wgsl"))
    .with_backface_culling(true)
    .with_depth_test(true)
    .with_depth_write(true);
```

**3b. Update `material_helpers.rs:12-51`:**
```rust
// Remove the shader_registry parameter:
pub fn create_checkerboard_material(
    context: Rc<VulkanContext>,
    render_pass: &RenderPass,
    // shader_registry: &ShaderRegistry,  // REMOVE
) -> Material {
    // ... texture creation code unchanged ...

    // Replace shader loading:
    let vertex_binding = VertexPBR::get_vertex_binding();
    let wgsl_path = std::path::Path::new("resources/shaders/model_pbr.wgsl");
    let material_pipeline = MaterialBuilder::new(context.clone())
        .with_vertex_binding(vertex_binding.clone())
        .with_wgsl_shader(wgsl_path)  // Changed from get_vertex_shader/get_fragment_shader
        .with_texture(texture.clone())
        .with_depth_test(true)
        .with_depth_write(true)
        .with_backface_culling(true)
        .build(render_pass)
        .expect("Failed to create material pipeline");
    // ... rest unchanged ...
}
```

**3c. Update `mesh/builder.rs`:**
```rust
// Remove the ShaderRegistry field and import:
use crate::{
    application::Model,
    entities::ModelEntity,
    rendering::{create_checkerboard_material, create_colored_checkerboard_material, Material, MaterialManager},  // Remove ShaderRegistry
};

pub struct MeshBuilder {
    options: MeshOptions,
    context: Rc<VulkanContext>,
    // shader_registry: ShaderRegistry,  // REMOVE
    material_manager: Option<MaterialManager>,
}

impl MeshBuilder {
    pub fn new(context: Rc<VulkanContext>) -> Self {
        Self {
            options: MeshOptions::default(),
            context,
            // shader_registry: ShaderRegistry::new(),  // REMOVE
            material_manager: None,
        }
    }
```

Also remove the `&self.shader_registry` parameter passed to `create_checkerboard_material` in any methods within `mesh/builder.rs`.

**Entry point naming:** The WGSL shader must use `vs_main` and `fs_main` entry points (not `main`), as this is what `ShaderModule::from_wgsl()` expects (materialbuilder.rs:175, 183).

### 4. Update Imports and Module Exports

**Files:** `katla_app/src/application/mod.rs`, `katla_app/src/rendering/mod.rs`

Remove `ShaderRegistry` from imports and exports:

```rust
// In application/mod.rs:26, remove ShaderRegistry:
use crate::rendering::{
    create_checkerboard_material, create_colored_checkerboard_material, MaterialManager, MeshBuilder
    // ShaderRegistry,  // REMOVE
};

// In rendering/mod.rs:12, remove the export:
// pub use shader_registry::ShaderRegistry;  // REMOVE or comment out
```

### 5. Clean Up Unused Files

**Delete these files:**
- `resources/shaders/*.spv` (all 9 SPIR-V files: gui.vert.spv, gui.frag.spv, model_pos.vert.spv, model_norm.vert.spv, model_pbr.vert.spv, model_no_tex.frag.spv, model.frag.spv)
- `resources/shaders/gui.vert`
- `resources/shaders/gui.frag`
- `resources/shaders/model_pos.vert`
- `resources/shaders/model_norm.vert`
- `resources/shaders/model_no_tex.frag`
- `resources/shaders/model.wgsl` (unused, different from model_pbr)
- `katla_app/src/rendering/shader_registry.rs` (no longer needed after migration)

### 6. Update Build System (if needed)

Check `Cargo.toml` and build scripts for any SPIR-V compilation steps (e.g., `build.rs` with `glslValidator` or similar) and remove them. (Verified: no `build.rs` files exist in the workspace, so this step may not be needed.)

## Critical Files to Modify

1. **Create:** `resources/shaders/model_pbr.wgsl` - new WGSL shader
2. **Create:** `resources/materials/model_pbr.toml` - material definition
3. **Modify:** `katla_app/src/rendering/material.rs:58-66` - change shader loading
4. **Modify:** `katla_app/src/rendering/material_helpers.rs` - remove shader_registry dependency
5. **Modify:** `katla_app/src/rendering/mesh/builder.rs` - remove shader_registry field
6. **Modify:** `katla_app/src/application/mod.rs` - remove shader_registry import
7. **Modify:** `katla_app/src/rendering/mod.rs` - remove shader_registry export
8. **Delete:** All `.spv` files and unused `.frag`/`.vert` files
9. **Delete:** `katla_app/src/rendering/shader_registry.rs`

## Verification

1. **Build:** `cargo build` - ensure no references to deleted .spv files
2. **Run:** Execute the application and verify models render correctly
3. **Test:** Run `cargo test` - ensure shader tests pass or are updated
4. **Verify cleanup:** Check that only WGSL shaders remain in `resources/shaders/`

## Existing Utilities to Reuse

- `katla_vulkan::vulkan::material::shadermodule.rs` - `ShaderModule::from_wgsl()` for runtime WGSL loading
- `katla_vulkan::vulkan::material::asset.rs` - Material asset loading from TOML
- Pattern from `colored_mesh.wgsl` and `colored_mesh.toml` - WGSL shader structure
