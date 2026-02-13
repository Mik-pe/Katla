# Multi-Stage Render Graph Plan: Sky Shader & PBR Materials

**Date:** 2026-02-13
**Status:** 📋 Planned
**Goal:** Implement a sky shader pass and PBR material pipeline for realistic lighting

---

## Executive Summary

This plan outlines the implementation of a multi-stage render graph to replace the current single-pass architecture. The key deliverables are:

1. **Sky Shader Pass** - Procedural sky rendering before geometry
2. **PBR Materials** - Physically-based rendering with metallic/roughness workflow
3. **Lighting Integration** - Sun direction shared between sky and materials

---

## Current State

### Render Graph Architecture

The current render graph uses a single "geometry_pass":

```
┌─────────────────┐
│  geometry_pass  │ ──> swapchain (color) + depth
│  clear: green   │
└─────────────────┘
```

**Location:** `katla_vulkan/src/lib.rs:552-741`

### Current Shader

`model_pbr_storage.wgsl` implements basic diffuse + ambient lighting:
- Hardcoded light direction: `vec3f(-0.3, -1.0, -0.2)`
- Simple NdotL diffuse calculation
- Basic ambient term

### Clear Color

Hardcoded green background: `[0.3, 0.5, 0.3, 1.0]`

---

## Target Architecture

### Render Graph

```
┌─────────────┐     ┌─────────────────┐
│  sky_pass   │ ──> │  geometry_pass  │ ──> swapchain + depth
│ clear color │     │  load (no clear)│
│ clear depth │     │  PBR lighting   │
└─────────────┘     └─────────────────┘
```

### Key Differences

| Aspect | Current | Target |
|--------|---------|--------|
| Clear | Geometry pass clears | Sky pass clears |
| Depth write | Geometry writes | Sky doesn't write |
| Background | Solid green | Procedural sky |
| Lighting | Basic diffuse | PBR (metallic/roughness) |
| Light source | Hardcoded | Shared with sky |

---

## Phase 1: Sky Pass

### 1.1 Sky Shader (`resources/shaders/sky.wgsl`)

**Approach:** Procedural sky using ray marching on a fullscreen quad

**Features:**
- Sky gradient (horizon to zenith colors)
- Sun disk with glow
- Atmospheric scattering (simplified)
- Time uniform for future day/night cycle

**Vertex Shader:**
```wgsl
// Fullscreen triangle - no vertex buffer needed
// Outputs: clip_position, view_direction
```

**Fragment Shader:**
```wgsl
// Ray march through atmosphere
// Calculate sky color based on view direction
// Add sun disk if view direction is near sun
```

### 1.2 Sky Pipeline

**Depth Settings:**
- `depth_test_enable: true`
- `depth_write_enable: false`
- `depth_compare_op: ALWAYS` (sky always passes)

**Rasterization:**
- `cull_mode: FRONT` (or no culling)
- No vertex input (generate verts in shader)

### 1.3 Render Graph Integration

**File:** `katla_vulkan/src/lib.rs` - `setup_render_graph()`

**Changes:**
1. Add sky pass before geometry pass
2. Move clear values to sky pass
3. Change geometry pass to use `load` instead of `clear`

```rust
// Sky pass - clears buffers
graph_builder.add_pass("sky_pass", |pass| {
    pass.write(Attachment::Color(swapchain_res))
        .write(Attachment::DepthStencil(depth_res))
        .clear_color(swapchain_res, [0.0, 0.0, 0.0, 1.0])  // Will be overwritten
        .clear_depth_stencil(depth_res, 1.0, 0)
        .execute("sky_pass", |ctx| {
            // Render fullscreen sky quad
        });
});

// Geometry pass - loads existing content
graph_builder.add_pass("geometry_pass", |pass| {
    pass.write(Attachment::Color(swapchain_res))
        .write(Attachment::DepthStencil(depth_res))
        // No clear - load existing content from sky pass
        .execute("geometry_pass", |ctx| {
            // Render scene geometry
        });
});
```

---

## Phase 2: PBR Materials

### 2.1 PBR Shader Improvements

**Add to `model_pbr_storage.wgsl`:**

```wgsl
// PBR Functions
fn fresnel_schlick(cos_theta: f32, F0: vec3f) -> vec3f {
    return F0 + (1.0 - F0) * pow(1.0 - cos_theta, 5.0);
}

fn distribution_ggx(N: vec3f, H: vec3f, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let NdotH = max(dot(N, H), 0.0);
    let NdotH2 = NdotH * NdotH;
    let num = a2;
    var denom = (NdotH2 * (a2 - 1.0) + 1.0);
    denom = PI * denom * denom;
    return num / denom;
}

fn geometry_smith(N: vec3f, V: vec3f, L: vec3f, roughness: f32) -> f32 {
    let NdotV = max(dot(N, V), 0.0);
    let NdotL = max(dot(N, L), 0.0);
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;
    let ggx1 = geometry_schlick_ggx(NdotV, k);
    let ggx2 = geometry_schlick_ggx(NdotL, k);
    return ggx1 * ggx2;
}
```

### 2.2 Material Parameters

**New uniform structure:**

```wgsl
struct ObjectUniforms {
    model: mat4x4f,
    base_color: vec4f,
    metallic: f32,
    roughness: f32,
    ao: f32,
    _padding: f32,
}
```

### 2.3 Lighting Uniforms

**Shared light data:**

```wgsl
struct LightUniforms {
    sun_direction: vec3f,   // From sky shader
    sun_color: vec3f,
    sun_intensity: f32,
    ambient_color: vec3f,
    ambient_intensity: f32,
    _padding: vec2f,
}
```

---

## Phase 3: Integration

### 3.1 Sky Material

**New file:** `katla_app/src/rendering/sky_material.rs`

```rust
pub struct SkyMaterial {
    pub pipeline: Rc<RefCell<MaterialPipeline>>,
    pub sun_direction: Vec3,
    pub sun_color: Color,
}

impl SkyMaterial {
    pub fn new(context: Rc<VulkanContext>) -> Self { ... }
    pub fn render(&self, cmd: &CommandBuffer) { ... }
}
```

### 3.2 Render Graph Update

**Key insight:** Both passes write to the same attachments but with different load ops

| Pass | Color | Depth | Load Op | Store Op |
|------|-------|-------|---------|----------|
| Sky | Yes | Yes | Clear | Store |
| Geometry | Yes | Yes | Load | Store |

### 3.3 Synchronization

No additional barriers needed - both passes execute sequentially in the same command buffer.

---

## Files to Create

| File | Description |
|------|-------------|
| `resources/shaders/sky.wgsl` | Procedural sky shader |
| `katla_app/src/rendering/sky_material.rs` | Sky pipeline and rendering |

## Files to Modify

| File | Changes |
|------|---------|
| `katla_vulkan/src/lib.rs` | Add sky pass to render graph |
| `resources/shaders/model_pbr_storage.wgsl` | Add PBR BRDF functions |
| `katla_app/src/rendering/mod.rs` | Export sky_material module |
| `katla_app/src/rendering/material.rs` | Add metallic/roughness parameters |

---

## Implementation Checklist

### Phase 1: Sky
- [ ] Create `sky.wgsl` with procedural sky
- [ ] Create fullscreen quad pipeline (no vertex buffer)
- [ ] Add sky pass to render graph
- [ ] Configure depth settings (test=true, write=false)
- [ ] Test: Sky visible behind geometry

### Phase 2: PBR
- [ ] Add GGX distribution function
- [ ] Add geometry/shadowing function
- [ ] Add Fresnel-Schlick function
- [ ] Add metallic/roughness uniforms
- [ ] Test: Materials show PBR behavior

### Phase 3: Integration
- [ ] Share sun direction between sky and PBR
- [ ] Update material system for PBR params
- [ ] Test: Lighting matches sky appearance

---

## Verification

1. **Visual:** Sky gradient visible, sun disk rendered
2. **Depth:** Geometry correctly occludes (sky behind objects)
3. **PBR:** Metallic surfaces reflect, rough surfaces diffuse
4. **Performance:** Maintain 60fps target

---

## Future Work (Out of Scope)

- Image-Based Lighting (IBL) with cubemaps
- Point/spot lights
- Shadow mapping
- Atmospheric fog
- Day/night cycle
- Volumetric clouds
