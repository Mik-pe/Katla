# SoA Vertex Buffers Plan

**Date:** 2026-02-13
**Goal:** Refactor vertex data from Array of Structures (AoS) to Structure of Arrays (SoA) layout for flexible, efficient rendering

> **Note:** This is a living document. Update it as implementation progresses.

---

## Executive Summary

Current vertex data uses interleaved Array of Structures (AoS) layout where each vertex's attributes are stored contiguously. This plan proposes refactoring to Structure of Arrays (SoA) where each attribute type has its own buffer.

**Why:** Enables depth-only passes, shadow mapping, better cache locality, and easier attribute management.

---

## Current vs Proposed Layout

### Current (AoS): Interleaved

```
Vertex 0: [px py pz] [nx ny nz] [tx ty tz tw] [u v]
Vertex 1: [px py pz] [nx ny nz] [tx ty tz tw] [u v]
Vertex 2: [px py pz] [nx ny nz] [tx ty tz tw] [u v]
...
```

Single vertex buffer, 48 bytes per vertex for `VertexPBR`.

### Proposed (SoA): Separate Buffers

```
positions:  [px py pz] [px py pz] [px py pz] ...
normals:    [nx ny nz] [nx ny nz] [nx ny nz] ...
tangents:   [tx ty tz tw] [tx ty tz tw] [tx ty tz tw] ...
uvs:        [u v] [u v] [u v] ...
```

Separate buffers per attribute, bound to different Vulkan binding slots.

---

## Benefits

| Benefit | Impact |
|---------|--------|
| **Depth-only passes** | Only bind position buffer - massive bandwidth savings for Z-prepass, shadow maps |
| **Shadow mapping** | Position-only, no need for normals/uvs/tangents |
| **Deferred rendering** | G-buffer fills can bind only needed attributes per target |
| **Better cache locality** | Position traversal doesn't pull unused attribute data into cache |
| **Flexible pipelines** | Different shaders use different attribute subsets without data duplication |
| **Easier extensibility** | Add new attribute = add new buffer, no struct layout changes |
| **Animation-friendly** | Skin only position/normal/tangent, leave UVs untouched |
| **Compression options** | Different compression per attribute type (e.g., quantized normals) |

---

## Implementation Plan

### Phase 1: Attribute Type Enum and Binding

**File:** `katla_vulkan/src/vulkan/vertexbinding.rs`

```rust
/// Semantic attribute types for vertex data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AttributeType {
    Position,
    Normal,
    Tangent,
    TexCoord0,
    TexCoord1,
    Color0,
    JointIndices,   // u16x4 for skeletal animation
    JointWeights,   // f32x4 for skeletal animation
}

impl AttributeType {
    /// Get the default Vulkan binding location for this attribute
    pub fn default_location(&self) -> u32 {
        match self {
            AttributeType::Position => 0,
            AttributeType::Normal => 1,
            AttributeType::Tangent => 2,
            AttributeType::TexCoord0 => 3,
            AttributeType::TexCoord1 => 4,
            AttributeType::Color0 => 5,
            AttributeType::JointIndices => 6,
            AttributeType::JointWeights => 7,
        }
    }
}

/// Single attribute buffer with format and binding info
pub struct AttributeBinding {
    pub attr_type: AttributeType,
    pub format: VertexFormat,
    pub buffer: VertexBuffer,
}

impl AttributeBinding {
    pub fn get_attribute_desc(&self, binding: u32) -> vk::VertexInputAttributeDescription {
        vk::VertexInputAttributeDescription::default()
            .binding(binding)
            .location(self.attr_type.default_location())
            .format(self.format.get_vk_format())
            .offset(0)  // Always 0 for SoA - each buffer starts at beginning
    }

    pub fn get_binding_desc(&self, binding: u32) -> vk::VertexInputBindingDescription {
        vk::VertexInputBindingDescription::default()
            .binding(binding)
            .stride(self.format.get_offset())  // Single element stride
            .input_rate(vk::VertexInputRate::VERTEX)
    }
}
```

### Phase 2: VertexAttributeSet

**File:** `katla_vulkan/src/vulkan/vertexbuffer.rs`

```rust
use std::collections::HashMap;

/// Collection of attribute buffers for a mesh (SoA layout)
pub struct VertexAttributeSet {
    attributes: HashMap<AttributeType, AttributeBinding>,
    vertex_count: u32,
}

impl VertexAttributeSet {
    pub fn new(vertex_count: u32) -> Self {
        Self {
            attributes: HashMap::new(),
            vertex_count,
        }
    }

    pub fn add_attribute(&mut self, binding: AttributeBinding) {
        self.attributes.insert(binding.attr_type, binding);
    }

    pub fn get(&self, attr_type: AttributeType) -> Option<&AttributeBinding> {
        self.attributes.get(&attr_type)
    }

    pub fn vertex_count(&self) -> u32 {
        self.vertex_count
    }

    /// Get all attribute descriptions for pipeline creation
    pub fn get_attribute_descriptions(&self) -> Vec<vk::VertexInputAttributeDescription> {
        self.attributes
            .values()
            .enumerate()
            .map(|(binding, attr)| attr.get_attribute_desc(binding as u32))
            .collect()
    }

    /// Get all binding descriptions for pipeline creation
    pub fn get_binding_descriptions(&self) -> Vec<vk::VertexInputBindingDescription> {
        self.attributes
            .values()
            .enumerate()
            .map(|(binding, attr)| attr.get_binding_desc(binding as u32))
            .collect()
    }

    /// Check if this set has the required attributes for a pipeline
    pub fn has_attributes(&self, required: &[AttributeType]) -> bool {
        required.iter().all(|attr| self.attributes.contains_key(attr))
    }
}
```

### Phase 3: CommandBuffer Binding

**File:** `katla_vulkan/src/vulkan/commandbuffer.rs`

```rust
impl CommandBuffer {
    /// Bind all vertex attributes from an SoA attribute set
    pub fn bind_vertex_attributes(&self, attributes: &VertexAttributeSet) {
        let bindings: Vec<_> = attributes.attributes
            .values()
            .enumerate()
            .sorted_by_key(|(_, attr)| attr.attr_type.default_location())
            .collect();

        let buffers: Vec<vk::Buffer> = bindings
            .iter()
            .map(|(_, attr)| attr.buffer.object())
            .collect();

        let offsets: Vec<vk::DeviceSize> = vec![0; buffers.len()];

        if !buffers.is_empty() {
            unsafe {
                self.device.cmd_bind_vertex_buffers(
                    self.buffer,
                    0,
                    &buffers,
                    &offsets,
                );
            }
        }
    }

    /// Bind only specific attributes (for depth-only, shadow passes, etc.)
    pub fn bind_attributes_subset(
        &self,
        attributes: &VertexAttributeSet,
        attr_types: &[AttributeType],
    ) {
        let mut buffers = Vec::new();
        let mut offsets = Vec::new();

        for attr_type in attr_types {
            if let Some(binding) = attributes.get(*attr_type) {
                buffers.push(binding.buffer.object());
                offsets.push(0);
            }
        }

        if !buffers.is_empty() {
            unsafe {
                self.device.cmd_bind_vertex_buffers(
                    self.buffer,
                    0,
                    &buffers,
                    &offsets,
                );
            }
        }
    }
}
```

### Phase 4: Mesh Storage Update

**File:** `katla_vulkan/src/vulkan/mesh.rs` (or wherever Mesh is defined)

```rust
/// Mesh with SoA vertex attributes
pub struct Mesh {
    pub attributes: VertexAttributeSet,
    pub index_buffer: IndexBuffer,
    pub bounding_sphere: Sphere,
}

impl Mesh {
    /// Create mesh from separate attribute arrays
    pub fn new_soa(
        context: Rc<VulkanContext>,
        positions: &[[f32; 3]],
        normals: &[[f32; 3]],
        tangents: &[[f32; 4]],
        uvs: &[[f32; 2]],
        indices: &[u8],
        index_stride: u8,
    ) -> Self {
        let vertex_count = positions.len() as u32;
        let mut attributes = VertexAttributeSet::new(vertex_count);

        // Create separate buffers for each attribute
        if !positions.is_empty() {
            let buffer = Self::create_attribute_buffer(
                context.clone(),
                positions,
                AttributeType::Position,
                VertexFormat::RGB32f,
            );
            attributes.add_attribute(buffer);
        }

        if !normals.is_empty() {
            let buffer = Self::create_attribute_buffer(
                context.clone(),
                normals,
                AttributeType::Normal,
                VertexFormat::RGB32f,
            );
            attributes.add_attribute(buffer);
        }

        // ... similar for tangents, uvs, etc.

        Self { attributes, index_buffer, bounding_sphere }
    }
}
```

### Phase 5: GLTF Parser Update

**File:** `katla_app/src/util/gltf_parser.rs`

```rust
/// Parsed attribute data in SoA format
pub struct ParsedAttributes {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub tangents: Vec<[f32; 4]>,
    pub tex_coords0: Vec<[f32; 2]>,
    pub joint_indices: Vec<[u16; 4]>,  // Optional, for animated meshes
    pub joint_weights: Vec<[f32; 4]>,  // Optional, for animated meshes
}

impl ParsedAttributes {
    pub fn from_gltf(primitive: &gltf::Primitive, buffers: &[BufferData]) -> Self {
        let parser = AttributeParser::new(buffers);

        let positions = primitive.attributes()
            .find(|(attr, _)| *attr == Semantic::Positions)
            .map(|(_, accessor)| parser.parse_positions(accessor))
            .unwrap_or_default();

        let normals = primitive.attributes()
            .find(|(attr, _)| *attr == Semantic::Normals)
            .map(|(_, accessor)| parser.parse_normals(accessor))
            .unwrap_or_default();

        // ... etc

        Self { positions, normals, /* ... */ }
    }

    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    pub fn has_skinning(&self) -> bool {
        !self.joint_indices.is_empty()
    }
}
```

### Phase 6: Shader Compatibility

Good news: **No shader changes needed!**

Shaders already use `@location(N)` semantics:
```wgsl
struct VertexInput {
    @location(0) position: vec3f,
    @location(1) normal: vec3f,
    @location(2) tangent: vec4f,
    @location(3) texcoord: vec2f,
}
```

SoA just binds each attribute to a separate buffer, but the shader sees the same input layout.

---

## Use Cases After Refactor

### Depth-Only Prepass

```rust
// Only position needed
ctx.command_buffer.bind_attributes_subset(
    &mesh.attributes,
    &[AttributeType::Position],
);
ctx.command_buffer.draw_indexed(mesh.index_count, 1, 0, 0, 0);
```

### Shadow Mapping

```rust
// Position only for shadow depth
ctx.command_buffer.bind_attributes_subset(
    &mesh.attributes,
    &[AttributeType::Position],
);
```

### Deferred G-Buffer Fill

```rust
// Position + Normal for G-buffer
ctx.command_buffer.bind_attributes_subset(
    &mesh.attributes,
    &[AttributeType::Position, AttributeType::Normal],
);
```

### Animated Mesh Skinning

```rust
// GPU compute skinning: read position/normal/tangent, write to separate buffers
// UVs stay untouched
```

---

## Migration Strategy

1. **Phase 1-2:** Add new types alongside existing `VertexBinding`
2. **Phase 3-4:** Add `Mesh::new_soa()` alongside existing `Mesh::new()`
3. **Phase 5:** Update GLTF loader to produce `ParsedAttributes`
4. **Phase 6:** Switch default mesh creation to SoA
5. **Phase 7:** Deprecate/remove old AoS code

---

## Files to Create

| File | Purpose |
|------|---------|
| `katla_vulkan/src/vulkan/vertex_attribute.rs` | `AttributeType`, `AttributeBinding`, `VertexAttributeSet` |

## Files to Modify

| File | Changes |
|------|---------|
| `katla_vulkan/src/vulkan/vertexbinding.rs` | Add `AttributeType` enum or import from new module |
| `katla_vulkan/src/vulkan/vertexbuffer.rs` | Add `VertexAttributeSet` |
| `katla_vulkan/src/vulkan/commandbuffer.rs` | Add `bind_vertex_attributes()`, `bind_attributes_subset()` |
| `katla_vulkan/src/vulkan/mesh.rs` | Add `new_soa()` constructor |
| `katla_app/src/util/gltf_parser.rs` | Return `ParsedAttributes` struct |
| `katla_app/src/rendering/vertextypes.rs` | Mark `VertexPBR` as deprecated |

---

## Testing

1. **Unit tests:** Attribute binding generation, buffer creation
2. **Integration tests:** Load GLTF, verify attribute counts match
3. **Visual tests:** Render with SoA, compare to AoS reference
4. **Performance tests:** Measure bandwidth reduction in depth-only pass

---

## References

- **Vulkan Vertex Input:** https://renderdoc.org/vkspec_chunked/chap21.html
- **SoA vs AoS:** https://en.wikipedia.org/wiki/AoS_and_SoA
- **GPU Cache Optimization:** https://developer.nvidia.com/gpugems/GPUGems3/gpugems3_ch35.html
