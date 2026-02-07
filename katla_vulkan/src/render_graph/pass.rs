use ash::vk;

use crate::resource::CompiledResource;
use crate::sync::{VkFramebuffer, VkRenderPass};
use crate::types::{ClearValue, Extent2D, PipelineBindPoint};
use crate::{CommandBuffer, ResourceId, ResourceUsage};
use std::collections::HashMap;
use std::rc::Rc;

/// Describes the type of attachment when writing to a resource.
/// This enum allows the render graph to correctly configure the attachment
/// for either color or depth-stencil output.
pub enum Attachment {
    /// Color attachment output (e.g., swapchain image, offscreen render target)
    Color(ResourceId),
    /// Depth-stencil attachment output (e.g., depth buffer for depth testing)
    DepthStencil(ResourceId),
}

/// Type alias for execute closures to avoid repeating complex trait object syntax.
/// This approach isolates the trait object definition to help Rust's type inference.
/// ExecutionRegistry stores pass execution closures by pass name.
/// This approach avoids trait object lifetime issues by storing closures
/// separately from pass metadata in a HashMap.
pub struct ExecutionRegistry<'a> {
    closures: HashMap<String, Box<dyn FnMut(Rc<PassExecutionContext>) + 'a>>,
}

impl<'a> ExecutionRegistry<'a> {
    pub fn new() -> Self {
        Self {
            closures: HashMap::new(),
        }
    }

    /// Register a closure for a pass with the given name.
    pub fn register<F>(&mut self, name: String, f: F)
    where
        F: FnMut(Rc<PassExecutionContext>) + 'static,
    {
        self.closures.insert(name, Box::new(f));
    }

    /// Execute the closure for a pass with the given name.
    pub fn execute(&mut self, name: &str, ctx: Rc<PassExecutionContext>) {
        if let Some(closure) = self.closures.get_mut(name) {
            closure(ctx);
        }
    }

    /// Get the number of registered closures.
    pub fn len(&self) -> usize {
        self.closures.len()
    }

    /// Check if the registry has any registered closures.
    pub fn is_empty(&self) -> bool {
        self.closures.is_empty()
    }
}

impl<'a> Default for ExecutionRegistry<'a> {
    fn default() -> Self {
        Self::new()
    }
}

/// PassExecute wraps a closure that executes a pass.
/// To avoid trait object lifetime issues with generic types, we use a name-based lookup.
/// The closure is stored in an ExecutionRegistry and looked up by pass name at runtime.
pub struct PassExecute {
    pass_name: String,
}

impl PassExecute {
    pub fn new(pass_name: String) -> Self {
        Self { pass_name }
    }

    pub fn pass_name(&self) -> &str {
        &self.pass_name
    }
}

/// PassBuilder provides an ergonomic fluent API for constructing render passes.
/// It allows you to define inputs, outputs, clear values, and execution logic
/// for a pass in a declarative way.
pub struct PassBuilder {
    name: String,
    inputs: Vec<ResourceId>,
    outputs: Vec<ResourceId>,
    usages: Vec<ResourceUsage>,
    bind_point: PipelineBindPoint,
    extent: Option<Extent2D>,
    execute: Option<PassExecute>,
    pending_execute: Option<(String, Box<dyn FnMut(Rc<PassExecutionContext>) + 'static>)>,
    #[allow(dead_code)]
    pipeline_barriers_before: Vec<vk::MemoryBarrier<'static>>,
}

impl PassBuilder {
    /// Create a new pass builder with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            usages: Vec::new(),
            bind_point: PipelineBindPoint::Graphics,
            extent: None,
            execute: None,
            pending_execute: None,
            pipeline_barriers_before: Vec::new(),
        }
    }

    /// Mark a resource as a read input for this pass.
    /// This adds the resource to the inputs list and creates appropriate usage information.
    pub fn read(&mut self, resource_id: ResourceId) -> &mut Self {
        self.inputs.push(resource_id);

        let usage = ResourceUsage::new(resource_id)
            .with_read(
                crate::types::Access::ShaderRead,
                crate::types::PipelineStage::FragmentShader,
            )
            .with_layout(crate::types::ImageLayout::ShaderReadOnlyOptimal);
        self.usages.push(usage);
        self
    }

    /// Mark a resource as a write output for this pass.
    /// This adds the resource to the outputs list and creates appropriate usage information
    /// based on the attachment type (color or depth-stencil).
    pub fn write(&mut self, attachment: Attachment) -> &mut Self {
        match attachment {
            Attachment::Color(resource_id) => {
                self.outputs.push(resource_id);

                let usage = ResourceUsage::new(resource_id)
                    .with_write(
                        crate::types::Access::ColorAttachmentWrite,
                        crate::types::PipelineStage::ColorAttachmentOutput,
                    )
                    .with_load_op(crate::types::AttachmentLoadOp::Load)
                    .with_store_op(crate::types::AttachmentStoreOp::Store)
                    .with_layout(crate::types::ImageLayout::ColorAttachmentOptimal);
                self.usages.push(usage);
            }
            Attachment::DepthStencil(resource_id) => {
                self.outputs.push(resource_id);

                let usage = ResourceUsage::new(resource_id)
                    .with_write(
                        crate::types::Access::DepthStencilAttachmentWrite,
                        crate::types::PipelineStage::EarlyFragmentTests,
                    )
                    .with_load_op(crate::types::AttachmentLoadOp::Load)
                    .with_store_op(crate::types::AttachmentStoreOp::Store)
                    .with_layout(crate::types::ImageLayout::DepthStencilAttachmentOptimal);
                self.usages.push(usage);
            }
        }
        self
    }

    /// Specify a clear color value for a color attachment.
    /// This should be called after write() for the resource you want to clear.
    pub fn clear_color(&mut self, resource_id: ResourceId, color: [f32; 4]) -> &mut Self {
        if let Some(usage) = self
            .usages
            .iter_mut()
            .find(|u| u.resource_id == resource_id)
        {
            usage.clear_value =
                Some(ClearValue::color(color[0], color[1], color[2], color[3]));
            usage.load_op = crate::types::AttachmentLoadOp::Clear.into();
        }
        self
    }

    /// Specify clear values for depth and stencil attachments.
    /// This should be called after write() for the depth/stencil resource you want to clear.
    pub fn clear_depth_stencil(
        &mut self,
        resource_id: ResourceId,
        depth: f32,
        stencil: u32,
    ) -> &mut Self {
        if let Some(usage) = self
            .usages
            .iter_mut()
            .find(|u| u.resource_id == resource_id)
        {
            usage.clear_value = Some(ClearValue::depth(depth, stencil));
            usage.load_op = crate::types::AttachmentLoadOp::Clear.into();
            // Depth attachments typically don't need to store, so use DONT_CARE
            usage.store_op = crate::types::AttachmentStoreOp::DontCare.into();
        }
        self
    }

    /// Set the extent (dimensions) for this pass.
    /// If not set, it will be derived from the resources used in the pass.
    pub fn extent(&mut self, width: u32, height: u32) -> &mut Self {
        self.extent = Some(Extent2D::new(width, height));
        self
    }

    /// Set the pipeline bind point (graphics or compute) for this pass.
    pub fn bind_point(&mut self, bind_point: PipelineBindPoint) -> &mut Self {
        self.bind_point = bind_point;
        self
    }

    /// Specify the execution logic for this pass.
    /// The provided closure will be stored temporarily and registered
    /// with the ExecutionRegistry during graph building.
    /// Pass name is used to look up the closure during execution.
    pub fn execute<F>(&mut self, name: impl Into<String>, f: F) -> &mut Self
    where
        F: FnMut(Rc<PassExecutionContext>) + 'static,
    {
        let name = name.into();
        self.pending_execute = Some((name.clone(), Box::new(f)));
        self.execute = Some(PassExecute::new(name));
        self
    }

    /// Build the pass from the builder configuration.
    pub fn build(self) -> Pass {
        Pass::from_builder(self)
    }
}

/// PassExecutionContext provides all the information needed during pass execution.
/// This is passed to the execute closure defined in the PassBuilder.
pub struct PassExecutionContext {
    /// Command buffer to record commands into
    pub command_buffer: std::rc::Rc<CommandBuffer>,
    /// Compiled resources available for this pass
    pub resources: std::rc::Rc<std::collections::HashMap<ResourceId, CompiledResource>>,
    /// The framebuffer for this pass
    pub framebuffer: VkFramebuffer,
    /// The Vulkan render pass for this pass
    pub render_pass: VkRenderPass,
    /// The current subpass index (0 for simple passes)
    pub subpass: u32,
    /// The render extent (width and height)
    pub extent: Extent2D,
}

impl PassExecutionContext {
    pub fn new(
        command_buffer: CommandBuffer,
        resources: std::rc::Rc<std::collections::HashMap<ResourceId, CompiledResource>>,
        framebuffer: vk::Framebuffer,
        render_pass: vk::RenderPass,
        extent: Extent2D,
    ) -> Self {
        Self {
            command_buffer: std::rc::Rc::new(command_buffer),
            resources,
            framebuffer: VkFramebuffer::new(framebuffer),
            render_pass: VkRenderPass::new(render_pass),
            subpass: 0,
            extent,
        }
    }

    /// Get a compiled resource by ID, if it exists
    pub fn get_resource(&self, resource_id: ResourceId) -> Option<&CompiledResource> {
        self.resources.get(&resource_id)
    }

    /// Get a compiled image resource by ID
    pub fn get_image(&self, resource_id: ResourceId) -> Option<(vk::Image, vk::ImageView)> {
        if let Some(CompiledResource::Image {
            image, image_view, ..
        }) = self.resources.get(&resource_id)
        {
            Some((*image, *image_view))
        } else {
            None
        }
    }

    /// Get a compiled buffer resource by ID
    pub fn get_buffer(&self, resource_id: ResourceId) -> Option<vk::Buffer> {
        if let Some(CompiledResource::Buffer { buffer, .. }) = self.resources.get(&resource_id) {
            Some(*buffer)
        } else {
            None
        }
    }
}

/// Represents a render pass in the render graph.
/// Passes are constructed using PassBuilder and then added to the RenderGraph.
pub struct Pass {
    name: String,
    inputs: Vec<ResourceId>,
    outputs: Vec<ResourceId>,
    usages: Vec<ResourceUsage>,
    bind_point: PipelineBindPoint,
    extent: Option<Extent2D>,
    execute: Option<PassExecute>,
    pending_execute: Option<(String, Box<dyn FnMut(Rc<PassExecutionContext>) + 'static>)>,
}

impl Pass {
    /// Create a pass from a PassBuilder.
    /// This is called internally by PassBuilder::build().
    pub fn from_builder(builder: PassBuilder) -> Self {
        Self {
            name: builder.name,
            inputs: builder.inputs,
            outputs: builder.outputs,
            usages: builder.usages,
            bind_point: builder.bind_point,
            extent: builder.extent,
            execute: builder.execute,
            pending_execute: builder.pending_execute,
        }
    }

    /// Get the name of this pass
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get all input resource IDs for this pass
    pub fn inputs(&self) -> &[ResourceId] {
        &self.inputs
    }

    /// Get all output resource IDs for this pass
    pub fn outputs(&self) -> &[ResourceId] {
        &self.outputs
    }

    /// Get the usage information for all resources in this pass
    pub fn usages(&self) -> &[ResourceUsage] {
        &self.usages
    }

    /// Get the pipeline bind point for this pass
    pub fn bind_point(&self) -> PipelineBindPoint {
        self.bind_point
    }

    /// Get the extent for this pass, if explicitly set
    pub fn extent(&self) -> Option<Extent2D> {
        self.extent
    }

    /// Execute this pass with the given context and registry
    pub fn execute(&self, ctx: Rc<PassExecutionContext>, registry: &mut ExecutionRegistry<'_>) {
        if let Some(execute) = &self.execute {
            registry.execute(&execute.pass_name, ctx);
        }
    }

    /// Get the name of this pass for looking up the execution closure
    pub fn execute_name(&self) -> Option<&str> {
        self.execute.as_ref().map(|e| e.pass_name())
    }

    /// Get the execute name for this pass.
    /// This is used during compilation to transfer the name to the compiled pass.
    pub fn take_execute_name(&mut self) -> String {
        self.execute
            .take()
            .map(|e| e.pass_name().to_string())
            .unwrap_or_default()
    }

    /// Take the pending execution closure and name from this pass.
    /// This is used during graph building to register the closure with the ExecutionRegistry.
    /// Returns None if no execution closure was specified.
    pub fn take_pending_execute(
        &mut self,
    ) -> Option<(String, Box<dyn FnMut(Rc<PassExecutionContext>) + 'static>)> {
        self.pending_execute.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pass_builder_creation() {
        let builder = PassBuilder::new("test_pass");
        assert_eq!(builder.name, "test_pass");
        assert!(builder.inputs.is_empty());
        assert!(builder.outputs.is_empty());
    }

    #[test]
    fn test_pass_builder_add_input_output() {
        let mut builder = PassBuilder::new("test_pass");

        let resource_id1 = ResourceId(0);
        let resource_id2 = ResourceId(1);

        builder.read(resource_id1);
        builder.write(Attachment::Color(resource_id2));

        assert_eq!(builder.inputs.len(), 1);
        assert_eq!(builder.outputs.len(), 1);
        assert_eq!(builder.inputs[0], resource_id1);
        assert_eq!(builder.outputs[0], resource_id2);
    }

    #[test]
    fn test_pass_builder_clear_color() {
        let mut builder = PassBuilder::new("test_pass");

        let resource_id = ResourceId(0);

        builder
            .write(Attachment::Color(resource_id))
            .clear_color(resource_id, [1.0, 0.0, 0.0, 1.0]);

        assert_eq!(builder.usages.len(), 1);
        let usage = &builder.usages[0];
        assert_eq!(usage.load_op, crate::types::AttachmentLoadOp::Clear.into());
        assert!(usage.clear_value.is_some());
    }

    #[test]
    fn test_pass_builder_extent() {
        let mut builder = PassBuilder::new("test_pass");

        builder.extent(1920, 1080);

        assert_eq!(builder.extent, Some(Extent2D::new(1920, 1080)));
    }

    #[test]
    fn test_pass_builder_bind_point() {
        let mut builder = PassBuilder::new("test_pass");

        builder.bind_point(PipelineBindPoint::Compute);

        assert_eq!(builder.bind_point, PipelineBindPoint::Compute);
    }

    #[test]
    fn test_pass_build() {
        let mut builder = PassBuilder::new("test_pass");

        let resource_id = ResourceId(0);
        builder.write(Attachment::Color(resource_id));

        let pass = builder.build();

        assert_eq!(pass.name(), "test_pass");
        assert_eq!(pass.outputs().len(), 1);
    }

    #[test]
    fn test_pass_accessors() {
        let mut builder = PassBuilder::new("test_pass");

        let input_id = ResourceId(0);
        let output_id = ResourceId(1);

        builder.read(input_id).write(Attachment::Color(output_id)).extent(1920, 1080);

        let pass = builder.build();

        assert_eq!(pass.inputs(), &[input_id]);
        assert_eq!(pass.outputs(), &[output_id]);
        assert_eq!(pass.extent(), Some(Extent2D::new(1920, 1080)));
        assert_eq!(pass.bind_point(), PipelineBindPoint::Graphics);
        assert_eq!(pass.usages().len(), 2);
    }
}
