# Render Graph API - Unified Design

> **Note**: This document describes the unified render graph API for `katla_gfx`. It combines the graphics engineering excellence (internal implementation) with developer experience (public API surface). All types live within `katla_gfx` - there is no separate "app layer" crate.

## Design Philosophy

This render graph API unifies both **graphics engineering excellence** and **developer experience**:

1. **Internal implementation is handle-based and explicit** (graphics engineering perspective)
2. **Public API uses strings and templates** (developer experience perspective)
3. **Compiler maps strings → handles at build time** (zero runtime cost)
4. **Sensible defaults with escape hatches** (easy path is fast path)

## All Within katla_gfx

**Important**: All types described here live within `katla_gfx`. The distinction is:
- **Public API** (`pub` types) - Exposed to `katla_app` and external users
- **Internal Implementation** (`pub(crate)` types) - Implementation details within `katla_gfx`

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│              PUBLIC API (katla_gfx exports)                 │
│  - String-based resource names                              │
│  - Pass templates (GeometryPass, FullscreenPass, etc.)      │
│  - Builder pattern                                          │
│  - Autocomplete-friendly                                    │
│  - Used by: katla_app, external users                       │
└────────────────────────┬────────────────────────────────────┘
                         │ build/compile
                         ▼
┌─────────────────────────────────────────────────────────────┐
│         INTERNAL IMPLEMENTATION (katla_gfx only)            │
│  - Handle-based resource tracking                           │
│  - Explicit dependency analysis                             │
│  - Barrier insertion                                        │
│  - Transient memory allocation                              │
│  - Visibility: pub(crate)                                   │
└─────────────────────────────────────────────────────────────┘
```

**Key insight**: Strings are resolved to handles at **graph build time** (once), not at execution time (every frame). Zero runtime overhead.

---

## Core API Types

### 1. GraphResourceHandle - Internal Resource Handle

```rust
/// Opaque handle for graph resources (internal use only).
///
/// Generated from string names at build time.
/// Prevents mixing graph resources with external handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GraphResourceHandle {
    index: u32,
    _marker: PhantomData<*const ()>, // !Send + !Sync
}

impl GraphResourceHandle {
    pub(crate) fn new(index: u32) -> Self {
        Self { index, _marker: PhantomData }
    }

    pub fn index(self) -> u32 {
        self.index
    }
}
```

### 2. ResourceState - Vulkan-Native State Tracking

```rust
/// Resource state for barrier tracking.
///
/// Maps directly to Vulkan pipeline stages and access flags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceState {
    /// Undefined (don't care about contents).
    Undefined,

    /// Color attachment output (render target).
    ColorAttachment,

    /// Depth-stencil read/write.
    DepthStencilAttachment,

    /// Shader read (sampled image or uniform buffer).
    ShaderRead,

    /// Shader write (storage image or storage buffer).
    ShaderWrite,

    /// Transfer source (copy from).
    TransferSrc,

    /// Transfer destination (copy to).
    TransferDst,

    /// Present source (swapchain image).
    PresentSrc,
}

impl ResourceState {
    /// Convert to Vulkan pipeline stage flags.
    pub fn to_vk_stage_flags(self) -> vk::PipelineStageFlags {
        match self {
            Self::Undefined => vk::PipelineStageFlags::TOP_OF_PIPE,
            Self::ColorAttachment => vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            Self::DepthStencilAttachment => vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS |
                                            vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
            Self::ShaderRead | Self::ShaderWrite =>
                vk::PipelineStageFlags::VERTEX_SHADER |
                vk::PipelineStageFlags::FRAGMENT_SHADER |
                vk::PipelineStageFlags::COMPUTE_SHADER,
            Self::TransferSrc | Self::TransferDst => vk::PipelineStageFlags::TRANSFER,
            Self::PresentSrc => vk::PipelineStageFlags::BOTTOM_OF_PIPE,
        }
    }

    /// Convert to Vulkan access flags.
    pub fn to_vk_access_flags(self) -> vk::AccessFlags {
        match self {
            Self::Undefined => vk::AccessFlags::empty(),
            Self::ColorAttachment => vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            Self::DepthStencilAttachment => vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ |
                                             vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
            Self::ShaderRead => vk::AccessFlags::SHADER_READ,
            Self::ShaderWrite => vk::AccessFlags::SHADER_WRITE,
            Self::TransferSrc => vk::AccessFlags::TRANSFER_READ,
            Self::TransferDst => vk::AccessFlags::TRANSFER_WRITE,
            Self::PresentSrc => vk::AccessFlags::NONE,
        }
    }
}
```

### 3. PassDesc - Internal Pass Representation

```rust
/// Internal pass descriptor (pub(crate)).
///
/// Created from public pass templates at build time.
pub(crate) struct PassDesc {
    /// Human-readable name for debugging.
    pub name: String,

    /// Resources this pass reads from (by handle).
    pub reads: Vec<GraphResourceHandle>,

    /// Resources this pass writes to (by handle).
    pub writes: Vec<GraphResourceHandle>,

    /// Pass type (graphics, compute, transfer).
    pub pass_type: PassType,

    /// Execution callback.
    pub execute: PassExecFn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PassType {
    Graphics,
    Compute,
    Transfer,
}

/// Pass execution callback.
pub type PassExecFn = Box<dyn FnOnce(&mut PassContext) -> Result<(), RenderGraphError> + 'static>;
```

---

## Public API - Builder Layer

### 4. FrameGraphBuilder - Graph Construction

```rust
/// Builder for constructing a frame graph.
///
/// Resources are referenced by string names for convenience.
/// Names are resolved to handles at build time.
pub struct FrameGraphBuilder {
    /// Pass declarations (string-based).
    passes: Vec<PassBuilder>,

    /// Resource declarations (string-based).
    resources: HashMap<String, ResourceDecl>,

    /// External resource imports.
    imports: Vec<(String, ExternalResourceHandle, GraphResourceDesc)>,
}

impl FrameGraphBuilder {
    /// Create a new graph builder.
    pub fn new() -> Self {
        Self {
            passes: Vec::new(),
            resources: HashMap::new(),
            imports: Vec::new(),
        }
    }

    /// Add a pass to the graph.
    ///
    /// # Example
    /// ```ignore
    /// let builder = FrameGraphBuilder::new()
    ///     .add_pass(GeometryPass::new("geometry")
    ///         .write_color("color", ImageFormat::R16G16B16A16Sfloat)
    ///         .write_depth("depth", ImageFormat::D32Sfloat));
    /// ```
    pub fn add_pass(mut self, pass: impl PassBuilder) -> Self {
        self.passes.push(pass.as_builder());
        self
    }

    /// Import an external resource into the graph.
    ///
    /// # Arguments
    /// * `name` - Resource name for graph reference
    /// * `handle` - External resource handle
    /// * `desc` - Resource descriptor
    pub fn import_resource(
        mut self,
        name: impl Into<String>,
        handle: ExternalResourceHandle,
        desc: GraphResourceDesc,
    ) -> Self {
        let name = name.into();
        self.resources.insert(name.clone(), ResourceDecl::Imported { desc: desc.clone() });
        self.imports.push((name, handle, desc));
        self
    }

    /// Build the graph into an executable form.
    ///
    /// Resolves string names to handles, analyzes dependencies,
    /// and pre-computes the execution plan.
    ///
    /// # Arguments
    /// * `renderer` - VulkanRenderer for resource allocation
    pub fn build(
        self,
        renderer: &VulkanRenderer,
    ) -> Result<FrameGraph, RenderGraphError> {
        // 1. Resolve string names to handles
        let resource_map = self.resolve_resource_names()?;

        // 2. Convert passes to PassDesc (with handles)
        let passes = self.convert_passes(&resource_map)?;

        // 3. Build internal RenderGraph
        let mut graph = RenderGraph::new();

        for (name, handle, desc) in self.imports {
            graph.import_resource(handle, desc)?;
        }

        for pass in passes {
            graph.add_pass(pass);
        }

        // 4. Compile the graph
        graph.compile()?;

        Ok(graph)
    }

    /// Resolve string resource names to handles.
    fn resolve_resource_names(&self) -> Result<HashMap<String, GraphResourceHandle>, RenderGraphError> {
        todo!()
    }

    /// Convert string-based pass builders to handle-based PassDesc.
    fn convert_passes(
        &self,
        resource_map: &HashMap<String, GraphResourceHandle>,
    ) -> Result<Vec<PassDesc>, RenderGraphError> {
        todo!()
    }
}

impl Default for FrameGraphBuilder {
    fn default() -> Self {
        Self::new()
    }
}
```

### 5. FrameGraph - Executable Graph

```rust
/// Executable render graph.
///
/// Built once, executed many times.
///
/// # Example
/// ```ignore
/// // Build once
/// let graph = FrameGraph::builder()
///     .add_pass(GeometryPass::new("geometry")
///         .write_color("color", ImageFormat::R16G16B16A16Sfloat)
///         .write_depth("depth", ImageFormat::D32Sfloat))
///     .build(&renderer)?;
///
/// // Execute every frame
/// graph.execute(&renderer, |ctx| {
///     ctx.pass("geometry").draw_list(&draw_list);
/// })?;
/// ```
pub struct FrameGraph {
    /// Internal render graph (handle-based).
    inner: RenderGraph,

    /// String → handle mapping (for execution context).
    resource_names: HashMap<String, GraphResourceHandle>,

    /// Pass name → index mapping (for execution context).
    pass_names: HashMap<String, usize>,
}

impl FrameGraph {
    /// Create a builder for this graph.
    pub fn builder() -> FrameGraphBuilder {
        FrameGraphBuilder::new()
    }

    /// Execute the graph.
    ///
    /// # Arguments
    /// * `renderer` - VulkanRenderer for GPU access
    /// * `f` - Execution callback with PassContext
    ///
    /// # Example
    /// ```ignore
    /// graph.execute(&renderer, |ctx| {
    ///     ctx.pass("geometry").draw_list(&opaque);
    ///     ctx.pass("transparent").draw_list(&transparent);
    ///     ctx.pass("ui").draw_ui(&ui_commands);
    /// })?;
    /// ```
    pub function execute<F>(
        &mut self,
        renderer: &VulkanRenderer,
        f: F,
    ) -> Result<(), RenderGraphError>
    where
        F: FnOnce(&mut ExecutionContext),
    {
        // Create execution context
        let mut ctx = ExecutionContext::new(&self.inner, &self.pass_names);

        // User callback
        f(&mut ctx);

        // Execute the graph
        self.inner.execute(renderer, ctx.take_commands())?;

        Ok(())
    }

    /// Get a resource handle by name (internal use).
    pub(crate) fn resource_handle(&self, name: &str) -> Option<GraphResourceHandle> {
        self.resource_names.get(name).copied()
    }
}
```

---

## Public API - Execution Layer

### 6. ExecutionContext - Pass Execution

```rust
/// Execution context for graph passes.
///
/// Provides autocomplete-friendly access to passes by name.
///
/// # Example
/// ```ignore
/// graph.execute(&renderer, |ctx| {
///     ctx.pass("geometry")
///         .set_frame_uniforms(&uniforms)
///         .draw_list(&opaque_draw_list);
///
///     ctx.pass("lighting")
///         .push_uniform(&light_data)
///         .dispatch();
/// })?;
/// ```
pub struct ExecutionContext<'a> {
    /// Inner graph reference.
    graph: &'a RenderGraph,

    /// Pass name mapping.
    pass_names: &'a HashMap<String, usize>,

    /// Command buffer for recording.
    cmd: Option<CommandBuffer>,

    /// Pending pass executions (name → draw data).
    pending: HashMap<String, PassData>,
}

impl<'a> ExecutionContext<'a> {
    pub(crate) fn new(
        graph: &'a RenderGraph,
        pass_names: &'a HashMap<String, usize>,
    ) -> Self {
        Self {
            graph,
            pass_names,
            cmd: None,
            pending: HashMap::new(),
        }
    }

    /// Access a pass by name.
    ///
    /// Returns a PassHandle for configuring pass execution.
    ///
    /// # Panics
    /// Panics if pass name doesn't exist (use try_pass for non-panic).
    pub fn pass(&mut self, name: &str) -> PassHandle {
        let index = *self.pass_names
            .get(name)
            .unwrap_or_else(|| panic!("Pass '{}' not found in graph", name));

        PassHandle {
            index,
            data: &mut self.pending,
        }
    }

    /// Try to access a pass by name (non-panic).
    pub fn try_pass(&mut self, name: &str) -> Option<PassHandle> {
        self.pass_names.get(name).map(|&index| PassHandle {
            index,
            data: &mut self.pending,
        })
    }

    /// Take the command buffer for execution.
    pub(crate) fn take_commands(&mut self) -> Option<CommandBuffer> {
        self.cmd.take()
    }
}

/// Handle for configuring pass execution.
///
/// Returned by ExecutionContext::pass().
pub struct PassHandle<'a> {
    index: usize,
    data: &'a mut HashMap<String, PassData>,
}

impl<'a> PassHandle<'a> {
    /// Set frame uniforms for this pass.
    pub fn set_frame_uniforms(&mut self, uniforms: &FrameUniforms) -> &mut Self {
        self.data
            .entry(self.index.to_string())
            .or_insert_with(PassData::new)
            .frame_uniforms = Some(uniforms.clone());
        self
    }

    /// Submit a DrawList for rendering.
    pub fn draw_list(&mut self, draw_list: &DrawList) -> &mut Self {
        self.data
            .entry(self.index.to_string())
            .or_insert_with(PassData::new)
            .draw_lists.push(draw_list.clone());
        self
    }

    /// Submit UI draw commands.
    pub fn draw_ui(&mut self, commands: &[UiDrawCommand]) -> &mut Self {
        self.data
            .entry(self.index.to_string())
            .or_insert_with(PassData::new)
            .ui_commands.extend_from_slice(commands);
        self
    }

    /// Push uniform data for compute/fullscreen passes.
    pub fn push_uniform(&mut self, data: &[u8]) -> &mut Self {
        self.data
            .entry(self.index.to_string())
            .or_insert_with(PassData::new)
            .uniform_data.extend_from_slice(data);
        self
    }

    /// Dispatch compute workgroups.
    pub fn dispatch(&mut self, x: u32, y: u32, z: u32) -> &mut Self {
        self.data
            .entry(self.index.to_string())
            .or_insert_with(PassData::new)
            .dispatch = Some((x, y, z));
        self
    }
}

/// Data for a single pass execution.
#[derive(Default, Clone)]
struct PassData {
    frame_uniforms: Option<FrameUniforms>,
    draw_lists: Vec<DrawList>,
    ui_commands: Vec<UiDrawCommand>,
    uniform_data: Vec<u8>,
    dispatch: Option<(u32, u32, u32)>,
}

impl PassData {
    fn new() -> Self {
        Self::default()
    }
}
```

---

## Public API - Pass Templates

### 7. GeometryPass

```rust
/// Geometry render pass template.
///
/// Renders 3D geometry with optional depth pre-pass.
///
/// # Example
/// ```ignore
/// let geometry = GeometryPass::new("geometry")
///     .write_color("color", ImageFormat::R16G16B16A16Sfloat)
///     .write_depth("depth", ImageFormat::D32Sfloat)
///     .clear_color([0.1, 0.1, 0.15, 1.0])
///     .clear_depth(1.0);
///
/// let graph = FrameGraph::builder()
///     .add_pass(geometry)
///     .build(&renderer)?;
///
/// graph.execute(&renderer, |ctx| {
///     ctx.pass("geometry").draw_list(&draw_list);
/// })?;
/// ```
pub struct GeometryPass {
    name: String,
    color_outputs: Vec<(String, ImageFormat, LoadOp, StoreOp, ClearValue)>,
    depth_output: Option<(String, ImageFormat, LoadOp, StoreOp, ClearValue)>,
    reads: Vec<String>,
}

impl GeometryPass {
    /// Create a new geometry pass.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            color_outputs: Vec::new(),
            depth_output: None,
            reads: Vec::new(),
        }
    }

    /// Add a color attachment output.
    pub fn write_color(
        mut self,
        name: impl Into<String>,
        format: ImageFormat,
    ) -> Self {
        self.color_outputs.push((
            name.into(),
            format,
            LoadOp::Clear,
            StoreOp::Store,
            ClearValue::OPAQUE_BLACK,
        ));
        self
    }

    /// Set the depth attachment output.
    pub fn write_depth(
        mut self,
        name: impl Into<String>,
        format: ImageFormat,
    ) -> Self {
        self.depth_output = Some((
            name.into(),
            format,
            LoadOp::Clear,
            StoreOp::Store,
            ClearValue::DEFAULT_DEPTH,
        ));
        self
    }

    /// Read from a resource (e.g., shadow map).
    pub fn read(mut self, name: impl Into<String>) -> Self {
        self.reads.push(name.into());
        self
    }

    /// Set color clear value.
    pub fn clear_color(mut self, color: [f32; 4]) -> Self {
        if let Some(output) = self.color_outputs.last_mut() {
            output.4 = ClearValue::Color(color);
        }
        self
    }

    /// Set depth clear value.
    pub fn clear_depth(mut self, depth: f32) -> Self {
        if let Some(output) = self.depth_output.as_mut() {
            output.4 = ClearValue::DepthStencil { depth, stencil: 0 };
        }
        self
    }
}

impl PassBuilder for GeometryPass {
    fn as_builder(self) -> PassBuilder {
        PassBuilder {
            name: self.name,
            pass_type: PassType::Graphics,
            reads: self.reads,
            writes: self
                .color_outputs
                .into_iter()
                .map(|(n, _, _, _, _)| n)
                .chain(self.depth_output.into_iter().map(|(n, _, _, _, _)| n))
                .collect(),
            build_fn: Box::new(move |resource_map: &HashMap<String, GraphResourceHandle>| {
                // Convert string names to handles
                let colors = self.color_outputs.iter()
                    .map(|(n, _, load, store, clear)| {
                        let handle = *resource_map.get(n).unwrap();
                        (handle, *load, *store, *clear)
                    })
                    .collect();

                let depth = self.depth_output.as_ref()
                    .map(|(n, _, load, store, clear)| {
                        let handle = *resource_map.get(n).unwrap();
                        (handle, *load, *store, *clear)
                    });

                let reads = self.reads.iter()
                    .map(|n| *resource_map.get(n).unwrap())
                    .collect();

                Ok(GeometryPassData { colors, depth, reads })
            }),
        }
    }
}
```

### 8. FullscreenPass

```rust
/// Fullscreen/compute pass template.
///
/// Post-processing, lighting, and compute-like work.
///
/// # Example
/// ```ignore
/// let tone_map = FullscreenPass::new("tone_map")
///     .read("hdr_color")
///     .write("ldr_output", ImageFormat::R8G8B8A8Srgb)
///     .pipeline(tone_map_pipeline);
///
/// let graph = FrameGraph::builder()
///     .add_pass(tone_map)
///     .build(&renderer)?;
///
/// graph.execute(&renderer, |ctx| {
///     ctx.pass("tone_map").dispatch();
/// })?;
/// ```
pub struct FullscreenPass {
    name: String,
    reads: Vec<String>,
    writes: Vec<(String, ImageFormat)>,
    pipeline: Option<PipelineHandle>,
}

impl FullscreenPass {
    /// Create a new fullscreen pass.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            reads: Vec::new(),
            writes: Vec::new(),
            pipeline: None,
        }
    }

    /// Read from a resource (can call multiple times).
    pub fn read(mut self, name: impl Into<String>) -> Self {
        self.reads.push(name.into());
        self
    }

    /// Write to an output resource.
    pub fn write(mut self, name: impl Into<String>, format: ImageFormat) -> Self {
        self.writes.push((name.into(), format));
        self
    }

    /// Set the graphics pipeline.
    pub fn pipeline(mut self, pipeline: PipelineHandle) -> Self {
        self.pipeline = Some(pipeline);
        self
    }
}

impl PassBuilder for FullscreenPass {
    fn as_builder(self) -> PassBuilder {
        PassBuilder {
            name: self.name,
            pass_type: PassType::Graphics,
            reads: self.reads,
            writes: self.writes.iter().map(|(n, _)| n.clone()).collect(),
            build_fn: Box::new(move |resource_map: &HashMap<String, GraphResourceHandle>| {
                // Convert to handles
                let reads = self.reads.iter()
                    .map(|n| *resource_map.get(n).unwrap())
                    .collect();

                let writes = self.writes.iter()
                    .map(|(n, _)| *resource_map.get(n).unwrap())
                    .collect();

                Ok(FullscreenPassData {
                    reads,
                    writes,
                    pipeline: self.pipeline,
                })
            }),
        }
    }
}
```

### 9. ShadowPass

```rust
/// Shadow mapping pass template.
///
/// Directional and point light shadow mapping.
///
/// # Example
/// ```ignore
/// let shadows = ShadowPass::new("shadows")
///     .write_depth("shadow_map", ImageFormat::D32Sfloat)
///     .resolution(2048, 2048)
///     .light_type(LightType::Directional);
///
/// let graph = FrameGraph::builder()
///     .add_pass(shadows)
///     .add_pass(GeometryPass::new("geometry")
///         .read("shadow_map")
///         .write_color("color", ImageFormat::R16G16B16A16Sfloat)
///         .write_depth("depth", ImageFormat::D32Sfloat))
///     .build(&renderer)?;
///
/// graph.execute(&renderer, |ctx| {
///     ctx.pass("shadows")
///         .light_direction([0.3, 1.0, 0.2])
///         .draw_list(&shadow_casters);
///
///     ctx.pass("geometry").draw_list(&main_geometry);
/// })?;
/// ```
pub struct ShadowPass {
    name: String,
    depth_output: Option<(String, ImageFormat)>,
    resolution: (u32, u32),
    light_type: LightType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightType {
    Directional,
    Point,
    Spot,
}

impl ShadowPass {
    /// Create a new shadow pass.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            depth_output: None,
            resolution: (1024, 1024),
            light_type: LightType::Directional,
        }
    }

    /// Set the depth output (shadow map).
    pub fn write_depth(mut self, name: impl Into<String>, format: ImageFormat) -> Self {
        self.depth_output = Some((name.into(), format));
        self
    }

    /// Set shadow map resolution.
    pub fn resolution(mut self, width: u32, height: u32) -> Self {
        self.resolution = (width, height);
        self
    }

    /// Set the light type.
    pub fn light_type(mut self, ty: LightType) -> Self {
        self.light_type = ty;
        self
    }
}

impl PassBuilder for ShadowPass {
    fn as_builder(self) -> PassBuilder {
        PassBuilder {
            name: self.name,
            pass_type: PassType::Graphics,
            reads: Vec::new(),
            writes: self.depth_output.iter().map(|(n, _)| n.clone()).collect(),
            build_fn: Box::new(move |resource_map: &HashMap<String, GraphResourceHandle>| {
                let depth = self.depth_output.as_ref()
                    .map(|(n, _)| *resource_map.get(n).unwrap());

                Ok(ShadowPassData {
                    depth,
                    resolution: self.resolution,
                    light_type: self.light_type,
                })
            }),
        }
    }
}
```

---

## Pass Builder Trait

```rust
/// Pass builder trait.
///
/// Implemented by all pass templates (GeometryPass, FullscreenPass, etc.).
pub trait PassBuilder: Any {
    /// Convert this pass to a PassBuilder (internal representation).
    fn as_builder(self) -> PassBuilder;
}
```

---

## Internal Types

### PassBuilder (Internal)

```rust
/// Internal pass builder representation.
pub(crate) struct PassBuilder {
    pub name: String,
    pub pass_type: PassType,
    pub reads: Vec<String>,
    pub writes: Vec<String>,
    pub build_fn: Box<dyn FnOnce(&HashMap<String, GraphResourceHandle>) -> Result<Box<dyn Any>, RenderGraphError>>,
}
```

### RenderGraph (Internal)

```rust
/// Internal render graph (handle-based, explicit).
pub(crate) struct RenderGraph {
    resources: ResourceStorage<GraphResource>,
    passes: Vec<PassDesc>,
    transient_allocator: TransientAllocator,
    execution_plan: Option<ExecutionPlan>,
    dirty: bool,
}

impl RenderGraph {
    pub(crate) fn new() -> Self {
        todo!()
    }

    pub(crate) fn import_resource(
        &mut self,
        handle: ExternalResourceHandle,
        desc: GraphResourceDesc,
    ) -> Result<GraphResourceHandle, RenderGraphError> {
        todo!()
    }

    pub(crate) fn add_pass(&mut self, pass: PassDesc) {
        todo!()
    }

    pub(crate) fn compile(&mut self) -> Result<(), RenderGraphError> {
        todo!()
    }

    pub(crate) fn execute(
        &mut self,
        renderer: &VulkanRenderer,
        commands: Option<CommandBuffer>,
    ) -> Result<(), RenderGraphError> {
        todo!()
    }
}
```

---

## Error Types

```rust
/// Render graph errors.
#[derive(Debug, thiserror::Error)]
pub enum RenderGraphError {
    /// Resource not found.
    #[error("Resource '{0}' not found")]
    ResourceNotFound(String),

    /// Pass not found.
    #[error("Pass '{0}' not found")]
    PassNotFound(String),

    /// Cycle detected in dependency graph.
    #[error("Cycle detected in dependency graph: {0}")]
    DependencyCycle(String),

    /// Invalid resource state transition.
    #[error("Invalid state transition: {0:?} -> {1:?} for resource '{2}'")]
    InvalidStateTransition(ResourceState, ResourceState, String),

    /// Allocation failed.
    #[error("Failed to allocate {0} bytes from transient allocator")]
    AllocationFailed(usize),

    /// Pipeline not set.
    #[error("Pipeline not set for pass '{0}'")]
    PipelineNotSet(String),

    /// Vulkan error.
    #[error("Vulkan error: {0}")]
    VulkanError(#[from] ash::vk::Result),
}
```

---

## Summary

### Public API Types (pub, ~15 types)

These are exported from `katla_gfx` and used by `katla_app`:

- `FrameGraph` - Executable graph
- `FrameGraphBuilder` - Graph construction
- `ExecutionContext` - Execution context
- `PassHandle` - Pass execution handle
- `GeometryPass` - Geometry pass template
- `FullscreenPass` - Fullscreen pass template
- `ShadowPass` - Shadow pass template
- `LightType` - Shadow light type
- `RenderGraphError` - Error type

### Internal Types (pub(crate), ~10 types)

These are implementation details within `katla_gfx` only:

- `GraphResourceHandle` - Resource handle
- `ResourceState` - Resource state for barriers
- `PassDesc` - Pass descriptor
- `PassExecFn` - Execution callback
- `PassContext` - Pass execution context
- `RenderGraph` - Internal graph
- `GraphCompiler` - Dependency analysis
- `ExecutionPlan` - Compiled graph
- `TransientAllocator` - Memory reuse

### What This Design Provides

- **Developer velocity**: Strings, templates, autocomplete-friendly
- **Graphics excellence**: Handles internally, explicit states, zero-cost abstractions
- **No hybrid states**: One way to do things, no confusion
- **Vulkan-native**: Barriers mapped 1:1 to Vulkan, explicit state tracking
