use std::collections::HashMap;
use std::rc::Rc;

use crate::pass::ExecutionRegistry;
use crate::render_graph::resource::Resource;
use crate::VulkanContext;
use crate::{CompiledRenderGraph, Pass, PassBuilder, RenderGraphError, ResourceId, ResourceKind};

#[derive(Default)]
pub struct RenderGraph {
    pub(crate) passes: Vec<Pass>,
    pub(crate) resources: HashMap<ResourceId, Resource>,
    next_id: ResourceId,
}

impl RenderGraph {
    pub fn add_resource(
        &mut self,
        name: impl Into<String>,
        resource_kind: ResourceKind,
    ) -> ResourceId {
        let id = self.next_id;
        self.resources
            .insert(id, Resource::new(id, name, resource_kind));
        self.next_id += 1;
        id
    }

    pub fn add_pass(&mut self, pass: Pass) {
        self.passes.push(pass);
    }
}

/// RenderGraphBuilder provides a fluent API for constructing render graphs.
/// It wraps a RenderGraph and provides convenience methods for building it.
/// It also manages the ExecutionRegistry that stores all pass execution closures.
pub struct RenderGraphBuilder {
    graph: RenderGraph,
    registry: ExecutionRegistry<'static>,
}

impl RenderGraphBuilder {
    /// Create a new render graph builder.
    pub fn new() -> Self {
        Self {
            graph: RenderGraph::default(),
            registry: ExecutionRegistry::new(),
        }
    }

    /// Add a resource to the render graph and return its ResourceId.
    pub fn add_resource(&mut self, name: impl Into<String>, kind: ResourceKind) -> ResourceId {
        self.graph.add_resource(name, kind)
    }

    /// Add a pass to the render graph using a builder closure.
    /// The closure receives a PassBuilder that can be configured.
    /// Any execution closures registered via PassBuilder::execute will be
    /// automatically stored in the builder's ExecutionRegistry.
    pub fn add_pass<F>(&mut self, name: impl Into<String>, f: F) -> &mut Self
    where
        F: FnOnce(&mut PassBuilder),
    {
        let mut builder = PassBuilder::new(name);
        f(&mut builder);
        let mut pass = builder.build();

        // Register any execution closure from the pass
        if let Some((exec_name, closure)) = pass.take_pending_execute() {
            self.registry.register(exec_name, closure);
        }

        // Register any pre-execution closure (runs before begin_rendering)
        if let Some((pre_exec_name, closure)) = pass.take_pending_pre_execute() {
            self.registry.register(pre_exec_name, closure);
        }

        self.graph.add_pass(pass);
        self
    }

    /// Build and compile the render graph into Vulkan objects.
    /// The ExecutionRegistry is transferred to the CompiledRenderGraph.
    pub fn build(
        self,
        context: &Rc<VulkanContext>,
    ) -> Result<CompiledRenderGraph, RenderGraphError> {
        CompiledRenderGraph::compile(self.graph, self.registry, context)
    }

    /// Get the internal RenderGraph (for testing purposes).
    pub fn graph(&self) -> &RenderGraph {
        &self.graph
    }

    /// Get a reference to the ExecutionRegistry (for testing purposes).
    pub fn registry(&self) -> &ExecutionRegistry<'_> {
        &self.registry
    }

    /// Get a mutable reference to the ExecutionRegistry (for testing purposes).
    pub fn registry_mut(&mut self) -> &mut ExecutionRegistry<'static> {
        &mut self.registry
    }

    /// Get the internal RenderGraph mutably (for testing purposes).
    pub fn graph_mut(&mut self) -> &mut RenderGraph {
        &mut self.graph
    }
}

impl Default for RenderGraphBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pass::Attachment;
    use crate::render_graph::types::{
        BufferUsage, Extent3D, ImageFormat, ImageLayout, ImageTiling, ImageUsage, MemoryProperty,
        SampleCount,
    };

    #[test]
    fn test_render_graph_builder_creation() {
        let builder = RenderGraphBuilder::new();
        assert_eq!(builder.graph.passes.len(), 0);
        assert_eq!(builder.graph.resources.len(), 0);
        assert_eq!(builder.registry.len(), 0);
    }

    #[test]
    fn test_render_graph_builder_add_resource() {
        let mut builder = RenderGraphBuilder::new();

        let resource_id = builder.add_resource(
            "buffer",
            ResourceKind::Buffer {
                size: 1024,
                usage: vec![BufferUsage::VertexBuffer],
                memory_properties: vec![MemoryProperty::DeviceLocal],
            },
        );

        assert_eq!(builder.graph.resources.len(), 1);
        assert_eq!(resource_id.0, 0);
        assert_eq!(builder.graph.next_id.0, 1);
    }

    #[test]
    fn test_render_graph_builder_add_pass() {
        let mut builder = RenderGraphBuilder::new();

        let resource_id = builder.add_resource(
            "color_target",
            ResourceKind::Image {
                extent: Extent3D::new(1920, 1080, 1),
                format: ImageFormat::R8G8B8A8Srgb,
                usage: vec![ImageUsage::ColorAttachment],
                samples: SampleCount::Sample1,
                tiling: ImageTiling::Optimal,
                initial_layout: ImageLayout::Undefined,
                final_layout: ImageLayout::ColorAttachmentOptimal,
            },
        );

        builder.add_pass("test_pass", |pass| {
            pass.write(Attachment::Color(resource_id))
                .clear_color(resource_id, [0.1, 0.2, 0.3, 1.0]);
        });

        assert_eq!(builder.graph.passes.len(), 1);
        assert_eq!(builder.graph.passes[0].name(), "test_pass");
        assert_eq!(builder.graph.passes[0].outputs().len(), 1);
    }

    #[test]
    fn test_render_graph_builder_multiple_passes() {
        let mut builder = RenderGraphBuilder::new();

        let resource_id = builder.add_resource(
            "color_target",
            ResourceKind::Image {
                extent: Extent3D::new(1920, 1080, 1),
                format: ImageFormat::R8G8B8A8Srgb,
                usage: vec![ImageUsage::ColorAttachment, ImageUsage::Sampled],
                samples: SampleCount::Sample1,
                tiling: ImageTiling::Optimal,
                initial_layout: ImageLayout::Undefined,
                final_layout: ImageLayout::ShaderReadOnlyOptimal,
            },
        );

        builder.add_pass("geometry_pass", |pass| {
            pass.write(Attachment::Color(resource_id))
                .clear_color(resource_id, [0.0, 0.0, 0.0, 1.0]);
        });

        builder.add_pass("post_process_pass", |pass| {
            pass.read(resource_id).extent(1920, 1080);
        });

        assert_eq!(builder.graph.passes.len(), 2);
        assert_eq!(builder.graph.passes[0].name(), "geometry_pass");
        assert_eq!(builder.graph.passes[1].name(), "post_process_pass");
    }

    #[test]
    fn test_render_graph_builder_build() {
        let mut builder = RenderGraphBuilder::new();

        let resource_id = builder.add_resource(
            "buffer",
            ResourceKind::Buffer {
                size: 1024,
                usage: vec![BufferUsage::VertexBuffer],
                memory_properties: vec![MemoryProperty::DeviceLocal],
            },
        );

        builder.add_pass("test_pass", |pass| {
            pass.read(resource_id);
        });

        let graph = builder.graph();

        assert_eq!(graph.resources.len(), 1);
        assert_eq!(graph.passes.len(), 1);
    }

    #[test]
    fn test_render_graph_builder_default() {
        let builder = RenderGraphBuilder::default();
        assert_eq!(builder.graph.passes.len(), 0);
    }

    #[test]
    fn test_render_graph_add_resource() {
        let mut graph = RenderGraph::default();
        let resource_id = graph.add_resource(
            "buffer",
            ResourceKind::Buffer {
                size: 1024,
                usage: vec![BufferUsage::VertexBuffer],
                memory_properties: vec![MemoryProperty::DeviceLocal],
            },
        );
        assert_eq!(graph.resources.len(), 1);
        assert_eq!(resource_id, ResourceId(0));
        assert_eq!(graph.next_id, ResourceId(1));
    }
}
