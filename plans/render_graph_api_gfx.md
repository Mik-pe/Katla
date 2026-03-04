# Render Graph API Design for Katla Graphics Engine

> **Note**: All types and APIs described in this document live within `katla_gfx`. This document focuses on the graphics engineering perspective - internal implementation, Vulkan-native design, and zero-cost abstractions.

## Design Philosophy

This render graph API is designed with the following principles:

### 1. Minimal Public API Surface
Every public type and function is a maintenance burden. The API exposes only what's necessary:
- **GraphResourceHandle** - Opaque handles for graph resources (internal)
- **Pass templates** - Single way to declare render passes (public)
- **FrameGraph** - Frame container and execution (public)
- **GraphCompiler** - Internal dependency analysis (pub(crate) implementation detail)

### 2. Vulkan-Native Thinking
The API embraces Vulkan's explicit state model:
- Resources are explicitly declared with format, dimensions, and usage
- Passes explicitly declare reads/writes
- Barriers are inferred but explicit in the API (BarrierKind enum exists)
- No hidden allocations or state mutations

### 3. Zero-Cost Abstractions
- Type-safe handles prevent mixing resource types at compile time
- No dynamic dispatch in hot paths
- Allocation happens at graph build time, execution is pre-calculated
- Transient resources reuse memory via arena allocator

### 4. Single Way to Do Things
- No hybrid implementations (old/new render pass APIs)
- No multiple resource types confusing the API
- Clear separation: external resources (imported) vs transient resources (created by graph)

### 5. Explicit Over Implicit
- Resource creation is explicit (format, size, usage)
- Dependencies are explicit via reads/writes
- Error states are returned as Results, not silently ignored

---

## Core Trait Definitions

### ResourceHandle - Opaque Handle for Graph Resources

```rust
/// Marker type for render graph resources.
///
/// This is distinct from external resources (TextureHandle, etc.) to prevent
/// accidental mixing of persistent and transient resources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GraphResourceMarker;

/// Opaque handle to a render graph resource.
///
/// Resources are either:
/// - **Imported**: External textures/buffers provided by the application
/// - **Transient**: Temporary resources created and destroyed within the graph
///
/// Type safety ensures graph resources can't be confused with external handles.
pub type GraphResourceHandle = Handle<GraphResourceMarker>;

/// Descriptor for a graph resource.
///
/// Fully specifies resource properties for allocation and barrier tracking.
#[derive(Clone, Debug)]
pub struct GraphResourceDesc {
    /// Resource type (texture or buffer).
    pub resource_type: GraphResourceType,

    /// Image format (for textures).
    pub format: ImageFormat,

    /// Width in pixels (for textures) or size in bytes (for buffers).
    pub width: u32,

    /// Height in pixels (for textures only).
    pub height: u32,

    /// Depth/array layers (for textures only).
    pub depth: u32,

    /// Initial state for barrier tracking.
    pub initial_state: ResourceState,
}

/// Type of graph resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphResourceType {
    /// 2D texture attachment.
    Texture2D,
    /// 2D texture array.
    Texture2DArray,
    /// Cubemap texture.
    Cubemap,
    /// Storage buffer.
    Buffer,
}

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
```

### Pass Trait - Render Pass Declaration

```rust
/// Render pass execution callback.
///
/// Called during graph execution with access to resolved resources.
/// Receives the command buffer for recording drawing commands.
pub type PassExecFn = Box<dyn FnOnce(&mut PassContext) -> Result<(), RenderGraphError> + 'static>;

/// Graph pass descriptor.
///
/// Passes are declared with their inputs, outputs, and execution callback.
/// The graph compiler analyzes dependencies and inserts barriers.
pub struct PassDesc {
    /// Human-readable name for debugging.
    pub name: String,

    /// Resources this pass reads from.
    pub reads: Vec<GraphResourceHandle>,

    /// Resources this pass writes to.
    pub writes: Vec<GraphResourceHandle>,

    /// Execution callback.
    pub execute: PassExecFn,
}

impl PassDesc {
    /// Create a new pass descriptor.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            reads: Vec::new(),
            writes: Vec::new(),
            execute: Box::new(|_| Ok(())),
        }
    }

    /// Add a resource read dependency.
    pub fn read(mut self, resource: GraphResourceHandle) -> Self {
        self.reads.push(resource);
        self
    }

    /// Add a resource write dependency.
    pub fn write(mut self, resource: GraphResourceHandle) -> Self {
        self.writes.push(resource);
        self
    }

    /// Set the execution callback.
    pub fn execute(mut self, f: PassExecFn) -> Self {
        self.execute = f;
        self
    }

    /// Builder helper: add multiple reads.
    pub fn reads(mut self, resources: &[GraphResourceHandle]) -> Self {
        self.reads.extend_from_slice(resources);
        self
    }

    /// Builder helper: add multiple writes.
    pub fn writes(mut self, resources: &[GraphResourceHandle]) -> Self {
        self.writes.extend_from_slice(resources);
        self
    }
}

/// Pass execution context.
///
/// Provides access to resolved resources during graph execution.
pub struct PassContext<'a> {
    /// Command buffer for recording drawing commands.
    pub cmd: &'a mut CommandBuffer,

    /// Resolved resource views (handle -> actual Vulkan resource).
    pub resources: &'a ResourceMap,

    /// Frame uniform descriptor set (set 0).
    pub frame_uniforms_set: DescriptorSet,

    /// Asset registry for accessing pipelines, meshes, etc.
    pub assets: &'a AssetRegistry,
}

impl<'a> PassContext<'a> {
    /// Get a texture view for a graph resource.
    ///
    /// Returns None if the resource is not a texture.
    pub fn texture_view(&self, resource: GraphResourceHandle) -> Option<ImageView> {
        self.resources.get_texture_view(resource)
    }

    /// Get a buffer view for a graph resource.
    ///
    /// Returns None if the resource is not a buffer.
    pub fn buffer_view(&self, resource: GraphResourceHandle) -> Option<BufferView> {
        self.resources.get_buffer_view(resource)
    }

    /// Begin a render pass with the specified attachments.
    pub fn begin_render_pass(
        &mut self,
        colors: &[GraphResourceHandle],
        depth: Option<GraphResourceHandle>,
    ) -> Result<RenderPassGuard<'_>, RenderGraphError> {
        // Implementation creates VkRenderPass and begins it
        todo!()
    }

    /// Bind a graphics pipeline.
    pub fn bind_pipeline(&mut self, pipeline: PipelineHandle) -> Result<(), RenderGraphError> {
        // Implementation binds pipeline
        todo!()
    }

    /// Draw indexed geometry.
    pub fn draw_indexed(
        &mut self,
        mesh: MeshHandle,
        instance_count: u32,
    ) -> Result<(), RenderGraphError> {
        // Implementation draws mesh
        todo!()
    }
}

/// RAII guard for active render pass.
///
/// Automatically ends the render pass when dropped.
pub struct RenderPassGuard<'a> {
    cmd: &'a mut CommandBuffer,
    // Internal state tracking
}

impl<'a> Drop for RenderPassGuard<'a> {
    fn drop(&mut self) {
        // End render pass if not already ended
        todo!()
    }
}
```

### RenderGraph - Frame Container and Execution

```rust
/// Render graph for frame rendering.
///
/// Manages transient resource allocation and pass execution.
/// Reusable across frames.
pub struct RenderGraph {
    /// Graph resources (imported and transient).
    resources: ResourceStorage<GraphResource>,

    /// Pass descriptors.
    passes: Vec<PassDesc>,

    /// Transient allocator for memory reuse.
    transient_allocator: TransientAllocator,

    /// Compiled execution plan (cached across frames).
    execution_plan: Option<ExecutionPlan>,

    /// Whether the graph needs recompilation.
    dirty: bool,
}

impl RenderGraph {
    /// Create a new render graph.
    pub fn new() -> Self {
        Self {
            resources: ResourceStorage::new(),
            passes: Vec::new(),
            transient_allocator: TransientAllocator::new(),
            execution_plan: None,
            dirty: true,
        }
    }

    /// Import an external resource into the graph.
    ///
    /// Importing a resource allows passes to read/write it.
    /// The graph does not own imported resources.
    ///
    /// # Arguments
    /// * `external_handle` - External handle (e.g., swapchain image)
    /// * `desc` - Resource descriptor for barrier tracking
    pub fn import_resource(
        &mut self,
        external_handle: ExternalResourceHandle,
        desc: GraphResourceDesc,
    ) -> Result<GraphResourceHandle, RenderGraphError> {
        let index = self.resources.insert(GraphResource::Imported {
            external: external_handle,
            desc,
        });
        self.dirty = true;
        Ok(GraphResourceHandle::new(index))
    }

    /// Create a transient resource within the graph.
    ///
    /// Transient resources are allocated from a transient memory pool
    /// and automatically reclaimed at the end of the frame.
    ///
    /// # Arguments
    /// * `desc` - Resource descriptor for allocation
    pub fn create_transient(
        &mut self,
        desc: GraphResourceDesc,
    ) -> Result<GraphResourceHandle, RenderGraphError> {
        let index = self.resources.insert(GraphResource::Transient { desc });
        self.dirty = true;
        Ok(GraphResourceHandle::new(index))
    }

    /// Add a pass to the graph.
    ///
    /// Passes are executed in dependency order.
    ///
    /// # Arguments
    /// * `pass` - Pass descriptor with reads/writes/execute callback
    pub fn add_pass(&mut self, pass: PassDesc) {
        self.passes.push(pass);
        self.dirty = true;
    }

    /// Set the output resource (final target).
    ///
    /// This is typically the swapchain image.
    /// The graph compiler ensures all passes complete before presenting.
    pub fn set_output(&mut self, resource: GraphResourceHandle) {
        // Mark resource as final output
        todo!()
    }

    /// Compile the graph into an execution plan.
    ///
    /// Analyzes pass dependencies and:
    /// - Topologically sorts passes
    /// - Inserts memory barriers
    /// - Allocates transient resources
    /// - Builds render pass descriptions
    ///
    /// Cached across frames unless the graph is modified.
    pub fn compile(&mut self) -> Result<(), RenderGraphError> {
        if !self.dirty {
            return Ok(());
        }

        let compiler = GraphCompiler::new(&self.resources, &self.passes);
        self.execution_plan = Somecompiler.compile()?;
        self.dirty = false;
        Ok(())
    }

    /// Execute the graph.
    ///
    /// Records all pass commands into the provided command buffer.
    /// Transient resources are allocated from the transient pool.
    ///
    /// # Arguments
    /// * `cmd` - Command buffer for recording
    /// * `frame_uniforms` - Frame-level shader uniforms
    pub fn execute(
        &mut self,
        cmd: &mut CommandBuffer,
        frame_uniforms: &FrameUniforms,
    ) -> Result<(), RenderGraphError> {
        self.compile()?;

        let plan = self.execution_plan.as_ref().unwrap();
        self.transient_allocator.reset();

        // Allocate transient resources
        for resource in plan.transient_resources() {
            self.transient_allocator.allocate(resource)?;
        }

        // Execute passes in dependency order
        for pass in &plan.passes {
            self.execute_pass(pass, cmd, frame_uniforms)?;
        }

        Ok(())
    }

    /// Clear all passes and transient resources.
    ///
    /// Retains imported resources.
    pub fn reset(&mut self) {
        self.passes.clear();
        self.resources.retain(|r| matches!(r, GraphResource::Imported { .. }));
        self.dirty = true;
    }

    /// Execute a single pass.
    fn execute_pass(
        &mut self,
        pass: &CompiledPass,
        cmd: &mut CommandBuffer,
        frame_uniforms: &FrameUniforms,
    ) -> Result<(), RenderGraphError> {
        // Insert barriers before pass
        for barrier in &pass.barriers {
            barrier.cmd_barrier(cmd)?;
        }

        // Create pass context
        let mut ctx = PassContext {
            cmd,
            resources: &pass.resource_map,
            frame_uniforms_set: frame_uniforms.descriptor_set(),
            assets: &self.assets,
        };

        // Execute pass callback
        (pass.callback)(&mut ctx)?;

        Ok(())
    }
}

impl Default for RenderGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Internal graph resource representation.
enum GraphResource {
    /// External resource (swapchain, persistent texture, etc.).
    Imported {
        external: ExternalResourceHandle,
        desc: GraphResourceDesc,
    },

    /// Transient resource (allocated and freed within frame).
    Transient {
        desc: GraphResourceDesc,
    },
}
```

### GraphCompiler - Dependency Analysis

```rust
/// Internal graph compiler (pub(crate) - implementation detail within katla_gfx).
///
/// NOT exposed to public API.
pub(crate) struct GraphCompiler<'a> {
    resources: &'a ResourceStorage<GraphResource>,
    passes: &'a [PassDesc],
}

impl<'a> GraphCompiler<'a> {
    pub(crate) fn new(
        resources: &'a ResourceStorage<GraphResource>,
        passes: &'a [PassDesc],
    ) -> Self {
        Self { resources, passes }
    }

    /// Compile the graph into an execution plan.
    pub(crate) fn compile(&self) -> Result<ExecutionPlan, RenderGraphError> {
        // 1. Build dependency graph
        let deps = self.build_dependency_graph()?;

        // 2. Topologically sort passes
        let sorted_passes = self.topological_sort(&deps)?;

        // 3. Insert barriers between passes
        let barriers = self.compute_barriers(&sorted_passes)?;

        // 4. Allocate transient resources
        let transient = self.allocate_transient_resources(&sorted_passes)?;

        Ok(ExecutionPlan {
            passes: sorted_passes,
            barriers,
            transient_resources: transient,
        })
    }

    /// Build dependency graph from pass reads/writes.
    fn build_dependency_graph(&self) -> Result<DependencyGraph, RenderGraphError> {
        todo!()
    }

    /// Topologically sort passes by dependency.
    fn topological_sort(&self, deps: &DependencyGraph) -> Result<Vec<CompiledPass>, RenderGraphError> {
        todo!()
    }

    /// Compute memory barriers between passes.
    fn compute_barriers(&self, passes: &[CompiledPass]) -> Result<Vec<MemoryBarrier>, RenderGraphError> {
        todo!()
    }

    /// Allocate transient resources.
    fn allocate_transient_resources(&self, passes: &[CompiledPass]) -> Result<Vec<TransientAllocation>, RenderGraphError> {
        todo!()
    }
}
```

### TransientAllocator - Memory Reuse

```rust
/// Transient resource allocator.
///
/// Manages memory for temporary attachments that are created and destroyed
/// within a single frame. Reuses memory across frames to reduce allocation overhead.
pub struct TransientAllocator {
    /// Memory pools for different allocation sizes.
    pools: [MemoryPool; 8],

    /// Current frame allocations.
    allocations: Vec<Allocation>,

    /// Total bytes allocated this frame.
    allocated_bytes: usize,
}

impl TransientAllocator {
    /// Create a new transient allocator.
    pub fn new() -> Self {
        todo!()
    }

    /// Allocate memory for a transient resource.
    pub fn allocate(&mut self, desc: &GraphResourceDesc) -> Result<Allocation, RenderGraphError> {
        // Round-trip through pools to find best fit
        todo!()
    }

    /// Reset allocations for the next frame.
    ///
    /// Memory is retained but marked as free for reuse.
    pub fn reset(&mut self) {
        self.allocations.clear();
        self.allocated_bytes = 0;
    }

    /// Get total bytes allocated this frame.
    pub fn allocated_bytes(&self) -> usize {
        self.allocated_bytes
    }
}

/// Memory pool for a specific size range.
struct MemoryPool {
    /// Pool block size (power of 2).
    block_size: usize,

    /// Free blocks.
    free_blocks: Vec<MemoryBlock>,

    /// Total capacity.
    capacity: usize,
}

/// Memory allocation.
pub struct Allocation {
    /// GPU memory offset.
    pub offset: u64,

    /// Allocation size in bytes.
    pub size: u64,

    /// Memory pool index.
    pub pool_index: usize,
}

/// Memory block.
struct MemoryBlock {
    /// Offset within pool.
    offset: u64,

    /// Block size.
    size: u64,
}
```

### Execution Plan - Compiled Graph

```rust
/// Compiled execution plan.
///
/// Pre-computed pass order, barriers, and allocations.
pub(crate) struct ExecutionPlan {
    /// Compiled passes in execution order.
    pub passes: Vec<CompiledPass>,

    /// Memory barriers between passes.
    pub barriers: Vec<MemoryBarrier>,

    /// Transient resource allocations.
    pub transient_resources: Vec<TransientAllocation>,
}

/// Compiled pass with resolved resources.
pub(crate) struct CompiledPass {
    /// Pass name.
    pub name: String,

    /// Execution callback.
    pub callback: PassExecFn,

    /// Resolved resource map (handle -> actual resource).
    pub resource_map: ResourceMap,

    /// Barriers to execute before this pass.
    pub barriers: Vec<MemoryBarrier>,

    /// Read dependencies.
    pub reads: Vec<GraphResourceHandle>,

    /// Write dependencies.
    pub writes: Vec<GraphResourceHandle>,
}

/// Memory barrier between passes.
pub(crate) struct MemoryBarrier {
    /// Resource to barrier.
    pub resource: GraphResourceHandle,

    /// Source state (before barrier).
    pub src_state: ResourceState,

    /// Destination state (after barrier).
    pub dst_state: ResourceState,

    /// Source pipeline stage.
    pub src_stage: PipelineStageFlags,

    /// Destination pipeline stage.
    pub dst_stage: PipelineStageFlags,
}

impl MemoryBarrier {
    /// Record the barrier to a command buffer.
    fn cmd_barrier(&self, cmd: &mut CommandBuffer) -> Result<(), RenderGraphError> {
        todo!()
    }
}
```

### Error Types

```rust
/// Render graph errors.
#[derive(Debug, thiserror::Error)]
pub enum RenderGraphError {
    /// Resource not found.
    #[error("Resource {0:?} not found")]
    ResourceNotFound(GraphResourceHandle),

    /// Cycle detected in dependency graph.
    #[error("Cycle detected in dependency graph involving pass: {0}")]
    DependencyCycle(String),

    /// Invalid resource state transition.
    #[error("Invalid state transition: {0:?} -> {1:?}")]
    InvalidStateTransition(ResourceState, ResourceState),

    /// Allocation failed (out of memory).
    #[error("Failed to allocate {0} bytes from transient allocator")]
    AllocationFailed(usize),

    /// Pass execution failed.
    #[error("Pass {0} failed: {1}")]
    PassExecutionFailed(String, Box<dyn std::error::Error>),

    /// Vulkan error.
    #[error("Vulkan error: {0}")]
    VulkanError(#[from] ash::vk::Result),
}
```

---

## Usage Examples

### Basic Forward Rendering

```rust
fn build_forward_graph(
    renderer: &mut VulkanRenderer,
    swapchain_view: GraphResourceHandle,
    depth_view: GraphResourceHandle,
) -> Result<RenderGraph, RenderGraphError> {
    let mut graph = RenderGraph::new();

    // Import external resources
    let color_target = graph.import_resource(
        ExternalResourceHandle::SwapchainImage(swapchain_view),
        GraphResourceDesc {
            resource_type: GraphResourceType::Texture2D,
            format: ImageFormat::B8G8R8A8Srgb,
            width: 1920,
            height: 1080,
            depth: 1,
            initial_state: ResourceState::Undefined,
        },
    )?;

    let depth_target = graph.import_resource(
        ExternalResourceHandle::DepthImage(depth_view),
        GraphResourceDesc {
            resource_type: GraphResourceType::Texture2D,
            format: ImageFormat::D32Sfloat,
            width: 1920,
            height: 1080,
            depth: 1,
            initial_state: ResourceState::Undefined,
        },
    )?;

    // Add forward rendering pass
    graph.add_pass(
        PassDesc::new("forward")
            .write(color_target)
            .write(depth_target)
            .execute(Box::new(|ctx| {
                // Begin render pass
                let _pass = ctx.begin_render_pass(&[color_target], Some(depth_target))?;

                // Bind pipeline and draw
                ctx.bind_pipeline(pipeline)?;
                for mesh in meshes {
                    ctx.draw_indexed(mesh, 1)?;
                }

                Ok(())
            })),
    );

    Ok(graph)
}
```

### Deferred Rendering with Multiple Passes

```rust
fn build_deferred_graph(
    renderer: &mut VulkanRenderer,
    swapchain_view: GraphResourceHandle,
) -> Result<RenderGraph, RenderGraphError> {
    let mut graph = RenderGraph::new();

    // Import swapchain
    let output = graph.import_resource(
        ExternalResourceHandle::SwapchainImage(swapchain_view),
        output_desc(),
    )?;

    // Create G-Buffer attachments
    let albedo = graph.create_transient(gbuffer_desc(ImageFormat::R8G8B8A8Srgb))?;
    let normal = graph.create_transient(gbuffer_desc(ImageFormat::A2B10G10R10UnormPack32))?;
    let position = graph.create_transient(gbuffer_desc(ImageFormat::R16G16B16A16Sfloat))?;
    let depth = graph.create_transient(depth_desc())?;

    // Geometry pass (write G-Buffer)
    graph.add_pass(
        PassDesc::new("geometry")
            .write(albedo)
            .write(normal)
            .write(position)
            .write(depth)
            .execute(Box::new(|ctx| {
                let _pass = ctx.begin_render_pass(
                    &[albedo, normal, position],
                    Some(depth),
                )?;
                ctx.bind_pipeline(geometry_pipeline)?;
                // Draw all geometry
                Ok(())
            })),
    );

    // Lighting pass (read G-Buffer, write output)
    graph.add_pass(
        PassDesc::new("lighting")
            .read(albedo)
            .read(normal)
            .read(position)
            .read(depth)
            .write(output)
            .execute(Box::new(|ctx| {
                let _pass = ctx.begin_render_pass(&[output], None)?;
                ctx.bind_pipeline(lighting_pipeline)?;
                // Draw fullscreen quad for lighting
                Ok(())
            })),
    );

    Ok(graph)
}
```

### Post-Processing Pipeline

```rust
fn build_post_processing_graph(
    input: GraphResourceHandle,
    output: GraphResourceHandle,
) -> Result<RenderGraph, RenderGraphError> {
    let mut graph = RenderGraph::new();

    // Import resources
    let input_tex = graph.import_resource(input, texture_desc())?;
    let output_tex = graph.import_resource(output, texture_desc())?;

    // Create intermediate transient
    let bloom_temp = graph.create_transient(transient_desc())?;

    // Bloom extract
    graph.add_pass(
        PassDesc::new("bloom_extract")
            .read(input_tex)
            .write(bloom_temp)
            .execute(Box::new(|ctx| {
                let _pass = ctx.begin_render_pass(&[bloom_temp], None)?;
                ctx.bind_pipeline(bloom_extract_pipeline)?;
                ctx.draw_indexed(quad_mesh, 1)?;
                Ok(())
            })),
    );

    // Bloom blur
    graph.add_pass(
        PassDesc::new("bloom_blur")
            .read(bloom_temp)
            .write(output_tex)
            .execute(Box::new(|ctx| {
                let _pass = ctx.begin_render_pass(&[output_tex], None)?;
                ctx.bind_pipeline(bloom_blur_pipeline)?;
                ctx.draw_indexed(quad_mesh, 1)?;
                Ok(())
            })),
    );

    Ok(graph)
}
```

---

## Integration with Existing katla_gfx Types

### External Resources

```rust
/// External resource handle.
///
/// Wraps existing katla_gfx handles for import into the graph.
#[derive(Clone, Debug)]
pub enum ExternalResourceHandle {
    /// Swapchain image (imported from VulkanRenderer).
    SwapchainImage(SwapchainHandle),

    /// Persistent texture (TextureHandle from katla_gfx).
    Texture(TextureHandle),

    /// Persistent buffer (BufferHandle from katla_gfx).
    Buffer(BufferHandle),
}
```

### Resource Map

```rust
/// Map of graph resources to actual Vulkan resources.
///
/// Resolved during graph execution.
pub struct ResourceMap {
    textures: HashMap<GraphResourceHandle, ImageView>,
    buffers: HashMap<GraphResourceHandle, BufferView>,
}

impl ResourceMap {
    pub fn get_texture_view(&self, resource: GraphResourceHandle) -> Option<ImageView> {
        self.textures.get(&resource).copied()
    }

    pub fn get_buffer_view(&self, resource: GraphResourceHandle) -> Option<BufferView> {
        self.buffers.get(&resource).copied()
    }
}
```

---

## API Surface Summary

### Public Types (pub - exported from katla_gfx)
- `FrameGraph` - Main graph container
- `FrameGraphBuilder` - Graph construction
- `ExecutionContext` - Execution context
- `GeometryPass`, `FullscreenPass`, `ShadowPass` - Pass templates
- `RenderGraphError` - Error type
- `ExternalResourceHandle` - External resource wrapper

### Internal Types (pub(crate) - katla_gfx implementation only)
- `GraphResourceHandle` - Internal resource handle
- `GraphResourceDesc` - Resource descriptor
- `ResourceState` - Resource state for barriers
- `PassDesc` - Internal pass descriptor
- `PassExecFn` - Pass execution callback
- `PassContext` - Context during pass execution
- `GraphCompiler` - Dependency analysis
- `ExecutionPlan` - Compiled graph
- `CompiledPass` - Compiled pass
- `MemoryBarrier` - Barrier between passes
- `TransientAllocator` - Memory reuse
- `ResourceMap` - Resource resolution

### Notes on Minimal API
- No separate "RenderPassBuilder" - PassDesc is sufficient
- No "ResourceBuilder" - GraphResourceDesc is sufficient
- No "GraphBuilder" - RenderGraph methods are sufficient
- No separate barrier types - BarrierKind already exists
- No texture sampling helpers - Use existing katla_gfx types

This design maintains a clean, minimal public API surface while providing powerful render graph capabilities. The internal implementation handles complex dependency analysis and barrier insertion without exposing complexity to the application layer.
