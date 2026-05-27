# Material Format Inference - CLEANER APPROACH

## Problem Statement

Currently, materials must be compiled with a specific `color_format`:
```rust
// For HDR rendering
let hdr_material = renderer.create_pbr_material(
    "shaders/pbr.wgsl",
    Some(ImageFormat::R16G16B16A16Sfloat)
)?;

// For LDR rendering
let ldr_material = renderer.create_pbr_material(
    "shaders/pbr.wgsl",
    Some(ImageFormat::B8G8R8A8Srgb)
)?;
```

**Issues:**
- App developers must understand render target formats
- Same material needs multiple handles for different passes
- Error-prone: easy to use wrong format for a pass
- Unnecessary duplication of materials

## Cleaner Solution: Single MaterialHandle with Deferred Compilation

Instead of creating a new handle type, we make `MaterialHandle` work in two modes:

1. **Fully compiled** (current behavior): Format specified, pipeline created immediately
2. **Deferred compilation** (NEW): No format specified, pipeline compiled on-demand

This approach:
- ✅ Reuses existing `MaterialHandle` - no new handle types
- ✅ Backward compatible - existing code works unchanged
- ✅ Single concept for developers - just "create a material"
- ✅ Format resolution is internal detail

## Architecture

### MaterialAsset Enhancement

```rust
pub struct MaterialAsset {
    // When fully_compiled = true:
    pub pipeline: PipelineHandle,

    // When fully_compiled = false (deferred):
    // Pipeline is compiled on-demand based on format
    pub pipeline: Option<PipelineHandle>,
    pub fully_compiled: bool,

    // For deferred materials: base options without color_format
    pub base_options: Option<MaterialOptions>,

    // The rest remains the same
    pub vertex_binding: VertexBinding,
    pub material_data: MaterialData,
    pub material_descriptor_set: Option<vk::DescriptorSet>,
    pub material_descriptor_layout: Option<vk::DescriptorSetLayout>,
}
```

### API Design

#### 1. Create Deferred Material (No Format Specified)

```rust
impl VulkanRenderer {
    /// Create a material without specifying format (compiles on-demand)
    pub fn create_material(
        &mut self,
        shader_path: impl AsRef<Path>,
    ) -> Result<MaterialHandle, RendererError>
    {
        self.create_material_with_options(
            shader_path,
            MaterialOptions {
                vertex_type: VertexType::Pbr,
                alpha_blended: false,
                double_sided: false,
                wireframe: false,
                color_format: ImageFormat::Auto,  // NEW: Auto = compile on-demand
            },
        )
    }

    /// Create a material with explicit format (immediate compilation)
    pub fn create_material_with_format(
        &mut self,
        shader_path: impl AsRef<Path>,
        format: ImageFormat,
    ) -> Result<MaterialHandle, RendererError>
    {
        self.create_material_with_options(
            shader_path,
            MaterialOptions {
                vertex_type: VertexType::Pbr,
                alpha_blended: false,
                double_sided: false,
                wireframe: false,
                color_format: format,  // Explicit format
            },
        )
    }
}
```

#### 2. Add ImageFormat::Auto Variant

```rust
// In ImageFormat enum
pub enum ImageFormat {
    // ... existing formats ...

    /// Auto-detect: compile material on-demand for each format used
    Auto,
}
```

#### 3. Frame Graph Declares Format Requirements

```rust
let graph = FrameGraph::builder()
    .create_resource(GraphResourceDesc {
        name: "hdr_color".to_string(),
        resource_type: GraphResourceType::ColorAttachment {
            clear_value: Some([0.1, 0.1, 0.15, 1.0]),
        },
        format: ImageFormat::R16G16B16A16Sfloat,  // Pass declares its format
    })
    .add_pass(GeometryPass::new("geometry")
        .write_color("hdr_color", ImageFormat::R16G16B16A16Sfloat)
        .material(material))  // Same material can be used in LDR pass too!
    .build(&renderer)?;
```

#### 4. Material Resolution Before Execution

```rust
// In FrameGraph::execute() - before the user's closure
fn execute(
    &mut self,
    renderer: &mut VulkanRenderer,
    image_index: u32,
    f: impl FnOnce(&mut Frame),
) -> Result<(), RenderGraphError>
{
    // NEW: Resolve deferred materials to format-specific variants
    self.resolve_materials(renderer)?;

    // Now all materials are compiled for the correct formats
    let mut frame = Frame::new(self, renderer, image_index);
    f(&mut frame);
    frame.execute_passes()?;

    Ok(())
}

// New method
fn resolve_materials(&mut self, renderer: &mut VulkanRenderer) -> Result<(), RenderGraphError> {
    for pass in &self.passes {
        if let Some(material_handle) = pass.material {
            // Check if material needs compilation for this pass's format
            renderer.ensure_material_compiled(material_handle, pass.output_format())?;
        }
    }
    Ok(())
}
```

### Internal Implementation

#### Changes to MaterialOptions

```rust
pub struct MaterialOptions {
    pub alpha_blended: bool,
    pub double_sided: bool,
    pub wireframe: bool,
    pub vertex_type: VertexType,
    /// Color attachment format for this material.
    /// - `Auto`: Compile on-demand for each format used (NEW)
    /// - Specific format: Compile immediately for that format
    pub color_format: ImageFormat,
}
```

#### AssetRegistry Enhancements

```rust
impl AssetRegistry {
    /// Ensure a material is compiled for the given format
    /// If material is deferred, compile the variant
    /// If variant exists, use it
    pub fn ensure_material_compiled(
        &mut self,
        material: MaterialHandle,
        format: ImageFormat,
        compiler: &mut MaterialCompiler,
    ) -> Result<(), MaterialError>
    {
        let asset = self.get_material(material)
            .ok_or_else(|| MaterialError::InvalidHandle(material))?;

        if asset.fully_compiled {
            // Check if format matches
            let pipeline = self.get_pipeline(asset.pipeline)?;
            if pipeline.format == format {
                return Ok(());  // Already compiled for this format
            }
            // Format mismatch - this is an error
            return Err(MaterialError::IncompatibleFormat {
                material,
                expected: format,
                actual: pipeline.format,
            });
        }

        // Deferred material - compile for this format
        let options = asset.base_options.as_ref()
            .ok_or_else(|| MaterialError::InvalidMaterialState)?;

        let compiled = compiler.compile(
            self,
            &asset.shader_path,
            options.with_color_format(format),
        )?;

        // Update the material with the compiled pipeline
        asset.pipeline = compiled.pipeline;
        asset.fully_compiled = true;

        Ok(())
    }
}
```

#### Add to Pass Desc

```rust
pub(crate) struct PassDesc {
    pub name: String,
    pub reads: Vec<String>,
    pub writes: Vec<String>,
    pub pass_type: PassType,
    pub execute: PassExecFn,
    pub pipeline: Option<crate::handle::PipelineHandle>,
    pub tonemap_params: Option<crate::render_graph::passes::TonemapParams>,

    // NEW: Material for this pass (if applicable)
    pub material: Option<crate::handle::MaterialHandle>,
}

impl PassDesc {
    // NEW: Helper to get output format
    pub(crate) fn output_format(&self) -> ImageFormat {
        // Extract format from writes
        self.writes.iter()
            .find_map(|name| {
                if name == "backbuffer" {
                    Some(ImageFormat::B8G8R8A8Srgb)
                } else {
                    None  // Would need to look up from resources
                }
            })
            .unwrap_or(ImageFormat::B8G8R8A8Srgb)
    }
}
```

### App Usage

```rust
// Phase 1: Create material (no format needed!)
let material = renderer.create_material("shaders/pbr.wgsl")?;

// Phase 2: Build frame graph (format declared by pass)
let graph = renderer
    .create_frame_graph()
    .create_resource(GraphResourceDesc {
        name: "hdr_color".to_string(),
        resource_type: GraphResourceType::ColorAttachment {
            clear_value: Some([0.1, 0.1, 0.15, 1.0]),
        },
        format: ImageFormat::R16G16B16A16Sfloat,
    })
    .add_pass(
        GeometryPass::new("geometry")
            .write_color("hdr_color", ImageFormat::R16G16B16A16Sfloat)
            .material(material),  // Auto-resolved to HDR variant
    )
    .build(&renderer)?;

// Phase 3: Execute (auto-resolves materials)
renderer.render(&graph, |frame| {
    frame.submit("geometry", &draw_list);
})?;
```

## Benefits

### For App Developers (APP Perspective)
- ✅ **Super simple**: `renderer.create_material("pbr.wgsl")?`
- ✅ **No format thinking**: Pass declares format, material adapts
- ✅ **Single material**: Use same material everywhere
- ✅ **Backward compatible**: Explicit format still works

### For GFX Maintainability (GFX Perspective)
- ✅ **No new handle type**: Reuses MaterialHandle
- ✅ **Explicit compilation**: `resolve_materials()` makes it clear
- ✅ **Format is first-class**: Pass declares what it needs
- ✅ **No hidden magic**: Compilation happens at known points

## How Other Engines Do It

### Bevy
- Materials are format-agnostic
- Pipelines are compiled when material is first used
- Material has a `shader` field, not a `pipeline` field

### Unity
- Materials reference Shaders
- Shaders have variants for different platforms/formats
- Material doesn't know or care about render target format

### The Forge
- Materials reference Shaders
- Render Backend manages PSO (Pipeline State Object) creation
- Material is independent of pipeline state

### Common Pattern
**Materials describe appearance, not implementation details.**
- Color, texture, roughness → material
- Pipeline format → render pass

Our solution follows this pattern cleanly!

## Implementation Steps

### Step 1: Add ImageFormat::Auto
- Add variant to ImageFormat enum
- Update Display, Default, etc.

### Step 2: Update MaterialOptions
- Make `color_format: ImageFormat` support `Auto` variant
- Document what Auto means

### Step 3: Update MaterialAsset
- Add `fully_compiled: bool` field
- Add `base_options: Option<MaterialOptions>` field
- Make `pipeline: Option<PipelineHandle>` (optional when deferred)

### Step 4: Update MaterialCompiler::compile()
- Check if `color_format == Auto`
- If Auto, store as deferred material
- Otherwise, compile immediately

### Step 5: Add VulkanRenderer::create_material()
- Simple method without format parameter
- Calls `create_material_with_options(Auto)`

### Step 6: Add .material() to pass builders
- GeometryPass, UIPass, etc. get `.material()` method

### Step 7: Add material field to PassDesc
- Store material handle
- Add `output_format()` helper

### Step 8: Add resolve_materials() to FrameGraph
- Iterate passes, compile materials as needed
- Call before `execute()`

### Step 9: Ensure VulkanRenderer::ensure_material_compiled()
- Logic to compile variants on-demand
- Cache compiled variants

### Step 10: Update app to use new API
- Switch to `create_material()` without format
- Test with both HDR and LDR passes

## Success Criteria

- [x] No new handle types (reuse MaterialHandle)
- [ ] `renderer.create_material("shader.wgsl")` works
- [ ] Same material can be used in HDR and LDR passes
- [ ] Material compiled only once per unique format
- [ ] Clear error when format is incompatible
- [ ] Backward compatible (explicit format still works)
- [ ] No performance regression
