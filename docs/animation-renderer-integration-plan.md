# Animation Renderer Integration Plan

**Date:** 2026-02-13
**Goal:** Integrate the existing CPU-side animation system with the Vulkan renderer for GPU-accelerated skeletal animation

> **Note:** This is a living document. Update it as implementation progresses, marking completed phases and adjusting estimates based on learnings.

---

## Executive Summary

The Katla engine has a complete CPU-side animation system in `katla_app/src/animation/` that handles:
- Animation clip loading from GLTF
- Skeletal animation with skinning data
- Animation playback (play/pause/loop/speed/crossfade)
- Morph target animation

However, **the renderer is not yet integrated** with this system. The GPU has no access to:
- Joint indices and weights (vertex skinning data)
- Current joint matrices (skeleton pose)
- Morph target weights

This plan outlines how to bridge the CPU animation system with the GPU renderer.

---

## Current State Analysis

### Animation System (CPU-side)

**Location:** `katla_app/src/animation/`

| Component | File | Description |
|-----------|------|-------------|
| `AnimationPlayer` | `components.rs` | Playback control (play/pause/loop/crossfade) |
| `AnimatedModel` | `components.rs` | Stores animation clips from GLTF |
| `Skin` | `skin.rs` | Joint indices and inverse bind matrices |
| `Skeleton` | `skin.rs` | Current joint transforms (Mat4 array) |
| `JointWeights` | `skin.rs` | Per-vertex joint indices + weights |
| `AnimationClip` | `clips.rs` | Animation data with channels |
| `AnimationSampler` | `clips.rs` | Keyframe interpolation (Linear/Step/CubicSpline) |
| `AnimationUpdateSystem` | `systems.rs` | Updates playback time |
| `SkeletalAnimationSystem` | `systems.rs` | Samples animation, updates joint transforms |
| `MorphTargetSystem` | `systems.rs` | Updates morph target weights |

**Animation Loading:** `gltf_loader.rs` parses GLTF animations and skins.

### Renderer (GPU-side)

**Current Vertex Format** (`VertexPBR`):
```rust
pub struct VertexPBR {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub tangent: [f32; 4],
    pub tex_coord0: [f32; 2],
}
// Total: 48 bytes, no skinning data!
```

**Current Shader** (`model_pbr_storage.wgsl`):
- Uses storage buffers for frame/object uniforms
- No skinning vertex attributes
- No joint matrix lookup

**Current Vertex Binding** (`vertexbinding.rs`):
```rust
pub fn get_pbr_vertex_binding() -> VertexBinding {
    VertexBinding {
        formats: vec![
            VertexFormat::RGB32f,  // position
            VertexFormat::RGB32f,  // normal
            VertexFormat::RGBA32f, // tangent
            VertexFormat::RG32f,   // uv
        ],
    }
}
```

### Missing Pieces

| Piece | Status | Location Needed |
|-------|--------|-----------------|
| Skinning vertex attributes | ❌ Missing | `VertexPBR`, `vertexbinding.rs` |
| GPU skinning shader | ❌ Missing | `model_pbr_storage.wgsl` |
| Joint matrix uniform buffer | ❌ Missing | `ObjectUniforms` or new buffer |
| Skinned mesh pipeline | ❌ Missing | Material system |
| GLTF joint/weight loading | ❌ Missing | `gltf_parser.rs` |
| Skeleton upload to GPU | ❌ Missing | New system or integration |

---

## Architecture Decision: AoS for Now

We'll implement animation integration using the current **Array of Structures (AoS)** vertex layout. This allows us to ship animation support faster without blocking on the larger SoA refactor.

**Future Work:** See [`docs/soa-vertex-buffers-plan.md`](./soa-vertex-buffers-plan.md) for the SoA refactor, which enables depth-only passes and more flexible attribute binding.

---

## Architecture Options

### Option A: Per-Mesh Joint Matrices (Recommended)

Each animated mesh has its own joint matrix storage buffer.

**Pros:**
- Different meshes can have different skeletons
- Works well with instancing (same pose per instance)
- Standard approach in game engines

**Cons:**
- More descriptor management
- Slightly more complex

**Implementation:**
```wgsl
// Set 0: Frame uniforms
// Set 1: Object uniforms + textures
// Set 2: Skeleton uniforms (per-mesh)

@group(2) @binding(0)
var<storage, read> joint_matrices: array<mat4x4f>;
```

### Option B: Global Joint Matrix Buffer

All joint matrices stored in one large buffer with offset indexing.

**Pros:**
- Single descriptor set for all skeletons
- Simpler descriptor management

**Cons:**
- Complex offset calculation
- Buffer size limits
- Harder to manage dynamic skeletons

### Option C: Push Constants

Use push constants for joint matrix offsets or small skeletons.

**Pros:**
- Fast updates
- No descriptor needed

**Cons:**
- Push constant size limits (128 bytes typically = max 8 mat4)
- Not suitable for complex skeletons (Fox has ~53 bones)

**Recommendation:** Option A with storage buffers, similar to existing `ObjectUniforms` pattern.

---

## Implementation Plan

### Phase 1: Vertex Format Extension

**Goal:** Add skinning attributes to vertex data.

#### 1.1 Create Skinned Vertex Type

**File:** `katla_app/src/rendering/vertextypes.rs`

```rust
/// Vertex format with skeletal animation support
#[repr(C)]
#[derive(Default, Debug, Clone)]
pub struct VertexSkinned {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub tangent: [f32; 4],
    pub tex_coord0: [f32; 2],
    // Skinning data
    pub joint_indices: [u16; 4],  // Up to 4 joint influences
    pub joint_weights: [f32; 4],  // Weights must sum to 1.0
}
// Total: 48 + 8 + 16 = 72 bytes
```

#### 1.2 Add Vertex Format Support

**File:** `katla_vulkan/src/vulkan/vertexbinding.rs`

```rust
/// Skinned PBR vertex format with position, normal, tangent, UV, and skinning
pub fn get_skinned_vertex_binding() -> VertexBinding {
    VertexBinding {
        formats: vec![
            VertexFormat::RGB32f,    // position (location 0)
            VertexFormat::RGB32f,    // normal (location 1)
            VertexFormat::RGBA32f,   // tangent (location 2)
            VertexFormat::RG32f,     // uv (location 3)
            VertexFormat::RGBA16u,   // joint_indices (location 4) - NEW
            VertexFormat::RGBA32f,   // joint_weights (location 5) - NEW
        ],
    }
}
```

**Note:** Need to add `RGBA16u` to `VertexFormat` enum.

#### 1.3 Parse Joint Data from GLTF

**File:** `katla_app/src/util/gltf_parser.rs`

Add methods to parse JOINTS_0 and WEIGHTS_0 attributes:

```rust
impl<'a> AttributeParser<'a> {
    /// Parse joint indices (u8 or u16) from accessor
    pub fn parse_joint_indices(&self, accessor: gltf::Accessor<'a>) -> Vec<[u16; 4]> {
        // GLTF stores as VEC4 of u8 (JOINTS_0) or u16
        // Convert to u16 for shader compatibility
    }

    /// Parse joint weights from accessor
    pub fn parse_joint_weights(&self, accessor: gltf::Accessor<'a>) -> Vec<[f32; 4]> {
        // GLTF stores as VEC4 of f32 (WEIGHTS_0)
    }
}
```

**Update `build_vertex_data()`:**
```rust
pub fn build_skinned_vertex_data(
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    tex_coords: Vec<[f32; 2]>,
    joint_indices: Vec<[u16; 4]>,
    joint_weights: Vec<[f32; 4]>,
) -> (Vec<VertexSkinned>, Sphere) {
    // ...
}
```

---

### Phase 2: GPU Skinning Shader

**Goal:** Implement vertex skinning in WGSL.

#### 2.1 Create Skinned PBR Shader

**File:** `resources/shaders/model_pbr_skinned.wgsl`

```wgsl
// Skinned PBR shader with GPU skeletal animation

// Frame uniforms (Set 0) - same as before
struct FrameUniforms { ... }

// Object uniforms (Set 0, binding 1)
struct ObjectUniforms {
    model: mat4x4f,
    base_color: vec4f,
    material_params: vec4f,
}

// Skeleton uniforms (Set 2, binding 0)
@group(2) @binding(0)
var<storage, read> joint_matrices: array<mat4x4f>;

// Maximum joints per skeleton
const MAX_JOINTS: u32 = 256u;

struct VertexInput {
    @location(0) position: vec3f,
    @location(1) normal: vec3f,
    @location(2) vert_tangent: vec4f,
    @location(3) vert_texcoord0: vec2f,
    // Skinning attributes
    @location(4) joint_indices: vec4u,  // u16 packed as u32
    @location(5) joint_weights: vec4f,
}

@vertex
fn vs_main(
    in: VertexInput,
    @builtin(instance_index) instance_idx: u32,
) -> VertexOutput {
    var out: VertexOutput;

    // Compute skinned position
    let skin_matrix =
        joint_matrices[in.joint_indices[0]] * in.joint_weights[0] +
        joint_matrices[in.joint_indices[1]] * in.joint_weights[1] +
        joint_matrices[in.joint_indices[2]] * in.joint_weights[2] +
        joint_matrices[in.joint_indices[3]] * in.joint_weights[3];

    // Apply skinning to position
    let skinned_pos = skin_matrix * vec4f(in.position, 1.0);

    // Apply model matrix and view-projection
    let obj = objects[instance_idx];
    let world_pos = obj.model * skinned_pos;
    out.world_pos = world_pos.xyz;
    out.clip_position = frame_data.proj * frame_data.view * world_pos;

    // Apply skinning to normal
    let skin_matrix_3x3 = mat3x3f(
        skin_matrix[0].xyz,
        skin_matrix[1].xyz,
        skin_matrix[2].xyz,
    );
    let normal_matrix = mat3x3f(
        obj.model[0].xyz,
        obj.model[1].xyz,
        obj.model[2].xyz,
    );
    out.world_normal = normalize(normal_matrix * skin_matrix_3x3 * in.normal);

    out.tex_coords = in.vert_texcoord0;
    out.instance_idx = instance_idx;

    return out;
}

// Fragment shader unchanged from model_pbr_storage.wgsl
```

#### 2.2 Joint Index Packing

GLTF stores joint indices as u8, but Vulkan/WGSL doesn't have `vec4<u8>`. Options:

1. **Pack as u32:** `(j0 | j1 << 8 | j2 << 16 | j3 << 24)`
2. **Use RGBA16UScaled:** Store as normalized u16
3. **Use separate storage buffer:** Look up from buffer instead

**Recommendation:** Use `vec4<u32>` with unpacking in shader for simplicity.

---

### Phase 3: Skeleton Buffer Management

**Goal:** Upload joint matrices to GPU each frame.

#### 3.1 Skeleton Uniform Buffer

**File:** `katla_vulkan/src/vulkan/skeleton_buffer.rs` (new)

```rust
/// Storage buffer for skeleton joint matrices
pub struct SkeletonBuffer {
    buffer: vk::Buffer,
    allocation: Allocation,
    max_joints: usize,
}

impl SkeletonBuffer {
    pub fn new(context: &Rc<VulkanContext>, max_joints: usize) -> Self;

    /// Upload joint matrices to GPU
    pub fn upload(&mut self, joint_matrices: &[Mat4]);

    pub fn buffer(&self) -> vk::Buffer;
    pub fn descriptor_info(&self) -> vk::DescriptorBufferInfo;
}
```

#### 3.2 Per-Mesh Skeleton Tracking

**File:** `katla_app/src/rendering/animated_mesh.rs` (new)

```rust
/// Tracks an animated mesh with its skeleton buffer
pub struct AnimatedMeshData {
    pub mesh_handle: MeshHandle,
    pub skeleton_buffer: SkeletonBuffer,
    pub joint_count: usize,
}

impl AnimatedMeshData {
    /// Update skeleton from ECS Skeleton component
    pub fn update_from_skeleton(&mut self, skeleton: &Skeleton) {
        self.skeleton_buffer.upload(&skeleton.joint_transforms);
    }
}
```

#### 3.3 Skeleton Upload System

**File:** `katla_app/src/systems/skeleton_upload_system.rs` (new)

```rust
/// ECS system that uploads skeleton poses to GPU
pub struct SkeletonUploadSystem {
    animated_meshes: HashMap<EntityId, AnimatedMeshData>,
}

impl System for SkeletonUploadSystem {
    fn update(&mut self, world: &mut World, _delta_time: f32) {
        for (entity, skeleton, skin) in world.query::<(&Skeleton, &Skin)>() {
            if let Some(animated_mesh) = self.animated_meshes.get_mut(&entity) {
                animated_mesh.update_from_skeleton(skeleton);
            }
        }
    }
}
```

**Execution Order:** After `SkeletalAnimationSystem`, before rendering.

---

### Phase 4: Material Pipeline Integration

**Goal:** Create skinned material variant and integrate with MaterialRegistry.

#### 4.1 Skinned Material Template

**File:** `resources/materials/gltf_skinned.toml` (new)

```toml
name = "gltf_skinned"
vertex_shader = "resources/shaders/model_pbr_skinned.wgsl"
fragment_shader = "resources/shaders/model_pbr_skinned.wgsl"  # same FS
vertex_binding = "skinned_pbr"  # references get_skinned_vertex_binding()
```

#### 4.2 MaterialBuilder Support

**File:** `katla_vulkan/src/vulkan/material/builder.rs`

Add method:
```rust
impl MaterialBuilder<'_> {
    pub fn with_skeleton_buffer(mut self, max_joints: usize) -> Self {
        self.skeleton_buffer = Some(max_joints);
        self
    }
}
```

#### 4.3 Descriptor Set Layout Update

Need third descriptor set for skeleton buffer:

```rust
// Set 0: Frame + Object uniforms (existing)
// Set 1: Textures (existing)
// Set 2: Skeleton joint matrices (new)
```

---

### Phase 5: Render Graph Integration

**Goal:** Render animated meshes with correct skeleton data.

#### 5.1 Draw Call Extension

**File:** `katla_vulkan/src/lib.rs`

Extend `DrawCall` to include skeleton buffer:

```rust
pub struct DrawCall {
    mesh_handle: MeshHandle,
    material_handle: MaterialHandle,
    matrices: DrawCallMatrices,
    skeleton_buffer: Option<vk::Buffer>,  // NEW
    // ...
}
```

#### 5.2 Animated Mesh Registration

When loading animated GLTF models:

1. Check if mesh has JOINTS_0/WEIGHTS_0 attributes
2. Use `VertexSkinned` format instead of `VertexPBR`
3. Create `SkeletonBuffer` for the mesh
4. Store skeleton buffer handle with mesh registration

#### 5.3 Render Pass Update

In `geometry_pass` execute closure:

```rust
// For each draw call with skeleton:
if let Some(skeleton_buffer) = draw_call.skeleton_buffer {
    // Bind descriptor set 2 with skeleton buffer
    command_buffer.bind_descriptor_set(2, skeleton_descriptor);
}
```

---

## Files to Create

| File | Purpose |
|------|---------|
| `resources/shaders/model_pbr_skinned.wgsl` | GPU skinning shader |
| `resources/materials/gltf_skinned.toml` | Skinned material template |
| `katla_vulkan/src/vulkan/skeleton_buffer.rs` | Joint matrix storage buffer |
| `katla_app/src/systems/skeleton_upload_system.rs` | ECS system for GPU upload |
| `katla_app/src/rendering/animated_mesh.rs` | Animated mesh data tracking |

## Files to Modify

| File | Changes |
|------|---------|
| `katla_app/src/rendering/vertextypes.rs` | Add `VertexSkinned` struct |
| `katla_vulkan/src/vulkan/vertexbinding.rs` | Add `get_skinned_vertex_binding()`, `RGBA16u` format |
| `katla_app/src/util/gltf_parser.rs` | Parse JOINTS_0/WEIGHTS_0 attributes |
| `katla_vulkan/src/lib.rs` | Extend `DrawCall` with skeleton buffer |
| `katla_vulkan/src/vulkan/material/builder.rs` | Add skeleton buffer support |
| `katla_app/src/entities/model.rs` | Detect and create animated mesh variant |

---

## Implementation Order

1. **Phase 1** - Vertex format (foundation)
   - Add `VertexSkinned` struct
   - Add vertex binding with skinning attributes
   - Update GLTF parser for joint data

2. **Phase 2** - GPU shader (core)
   - Create skinned PBR shader
   - Implement vertex skinning math

3. **Phase 3** - Skeleton buffer (data path)
   - Create SkeletonBuffer type
   - Implement upload system

4. **Phase 4** - Material integration (pipeline)
   - Add skinned material template
   - Update MaterialBuilder

5. **Phase 5** - Render graph (final)
   - Extend DrawCall
   - Bind skeleton descriptors

---

## Testing Strategy

1. **Unit Tests:**
   - Vertex packing/unpacking
   - Joint matrix upload
   - Interpolation correctness

2. **Integration Tests:**
   - Load Fox.glb with animations
   - Verify joint indices/weights parsed correctly
   - Verify skeleton transforms uploaded

3. **Visual Tests:**
   - Fox animation plays correctly
   - No vertex glitching
   - Smooth interpolation between keyframes

---

## Future Enhancements (Out of Scope)

- **Dual Quaternion Skinning:** Better for twisting joints
- **GPU Animation Compute:** Compute shader for animation sampling
- **Animation Compression:** Quantized keyframes
- **Async Animation:** Background thread for CPU sampling
- **Animation Streaming:** Stream large animation clips

---

## Considerations for `katla_animation` Crate

The animation code in `katla_app/src/animation/` is ~1000 lines across 8 files. Considerations for extraction:

**Pros of separate crate:**
- Clean separation of concerns
- Reusable without Vulkan dependency
- Easier testing in isolation

**Cons of separate crate:**
- More crate management overhead
- May need `katla_math` dependency anyway
- Current code is well-isolated in `animation/` module

**Recommendation:** Keep in `katla_app` for now. Extract to `katla_animation` only if:
1. Multiple renderers need animation support (e.g., wgpu backend)
2. Animation tools need the logic without Vulkan
3. The module grows significantly larger

---

## References

- **GLTF 2.0 Skinning:** https://github.com/KhronosGroup/glTF-Tutorials/blob/master/gltfTutorial/gltfTutorial_020_Skins.md
- **Vulkan Skinning:** https://developer.nvidia.com/gpugems/GPUGems3/gpugems3_ch02.html
- **WGSL Matrix Operations:** https://www.w3.org/TR/WGSL/#matrix-types
