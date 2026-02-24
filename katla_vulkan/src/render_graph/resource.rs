use std::collections::HashMap;
use std::fmt;
use std::ops::AddAssign;

/// Unique identifier for a render graph resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ResourceId(pub(crate) u32);

impl ResourceId {
    /// Get the raw u32 value of this resource ID.
    pub fn value(&self) -> u32 {
        self.0
    }
}

impl fmt::Display for ResourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ResourceId({})", self.0)
    }
}

/// Map from ResourceId to resource name for debugging purposes.
/// This allows looking up human-readable names from ResourceIds.
#[derive(Debug, Clone, Default)]
pub struct ResourceNameMap {
    names: HashMap<ResourceId, String>,
}

impl ResourceNameMap {
    /// Create a new empty name map.
    pub fn new() -> Self {
        Self {
            names: HashMap::new(),
        }
    }

    /// Insert a name for a resource ID.
    pub fn insert(&mut self, id: ResourceId, name: impl Into<String>) {
        self.names.insert(id, name.into());
    }

    /// Get the name for a resource ID, if it exists.
    pub fn get(&self, id: ResourceId) -> Option<&str> {
        self.names.get(&id).map(|s| s.as_str())
    }

    /// Get the name for a resource ID, or return a fallback string.
    pub fn get_or_fallback(&self, id: ResourceId) -> &str {
        self.names.get(&id).map(|s| s.as_str()).unwrap_or("unknown")
    }
}

/// A resource in the render graph (image, buffer, or external resource).
#[derive(Debug)]
pub struct Resource {
    pub(crate) id: ResourceId,
    pub(crate) name: String,
    pub(crate) kind: ResourceKind,
}

impl Resource {
    /// Get the resource ID.
    pub fn id(&self) -> ResourceId {
        self.id
    }

    /// Get the resource name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the resource kind.
    pub fn kind(&self) -> &ResourceKind {
        &self.kind
    }
}

#[derive(Debug)]
pub enum ResourceKind {
    Buffer {
        size: u64,
        usage: Vec<super::types::BufferUsage>,
        memory_properties: Vec<super::types::MemoryProperty>,
    },
    Image {
        extent: super::types::Extent3D,
        format: super::types::ImageFormat,
        usage: Vec<super::types::ImageUsage>,
        samples: super::types::SampleCount,
        tiling: super::types::ImageTiling,
        initial_layout: super::types::ImageLayout,
        final_layout: super::types::ImageLayout,
    },
    ExternalBuffer {
        buffer: super::types::VkBuffer,
    },
    ExternalImage {
        image: super::types::VkImage,
        image_view: super::types::VkImageView,
        format: super::types::ImageFormat,
        extent: super::types::Extent2D,
    },
}

/// Tracks how a resource is used within a specific pass.
/// This information is used for synchronization and determining
/// the necessary Vulkan barriers between passes.
#[derive(Clone)]
pub struct ResourceUsage {
    pub(crate) resource_id: ResourceId,
    pub(crate) access: Vec<super::types::Access>,
    pub(crate) stage: Vec<super::types::PipelineStage>,
    pub(crate) layout: super::types::ImageLayout,
    pub(crate) load_op: super::types::AttachmentLoadOp,
    pub(crate) store_op: super::types::AttachmentStoreOp,
    pub clear_value: Option<super::types::ClearValue>,
}

impl ResourceUsage {
    pub fn new(resource_id: ResourceId) -> Self {
        Self {
            resource_id,
            access: Vec::new(),
            stage: Vec::new(),
            layout: super::types::ImageLayout::Undefined,
            load_op: super::types::AttachmentLoadOp::DontCare,
            store_op: super::types::AttachmentStoreOp::DontCare,
            clear_value: None,
        }
    }

    pub fn with_read(
        mut self,
        access: super::types::Access,
        stage: super::types::PipelineStage,
    ) -> Self {
        if !self.access.contains(&access) {
            self.access.push(access);
        }
        if !self.stage.contains(&stage) {
            self.stage.push(stage);
        }
        self
    }

    pub fn with_write(
        mut self,
        access: super::types::Access,
        stage: super::types::PipelineStage,
    ) -> Self {
        if !self.access.contains(&access) {
            self.access.push(access);
        }
        if !self.stage.contains(&stage) {
            self.stage.push(stage);
        }
        self
    }

    pub fn with_layout(mut self, layout: super::types::ImageLayout) -> Self {
        self.layout = layout;
        self
    }

    pub fn with_load_op(mut self, load_op: super::types::AttachmentLoadOp) -> Self {
        self.load_op = load_op;
        self
    }

    pub fn with_store_op(mut self, store_op: super::types::AttachmentStoreOp) -> Self {
        self.store_op = store_op;
        self
    }

    pub fn with_clear_value(mut self, clear_value: super::types::ClearValue) -> Self {
        self.clear_value = Some(clear_value);
        self
    }

    /// Get the resource ID.
    pub fn resource_id(&self) -> ResourceId {
        self.resource_id
    }

    /// Get the access flags.
    pub fn access(&self) -> &[super::types::Access] {
        &self.access
    }

    /// Get the pipeline stage flags.
    pub fn stage(&self) -> &[super::types::PipelineStage] {
        &self.stage
    }

    /// Get the image layout.
    pub fn layout(&self) -> super::types::ImageLayout {
        self.layout
    }

    /// Get the attachment load operation.
    pub fn load_op(&self) -> super::types::AttachmentLoadOp {
        self.load_op
    }

    /// Get the attachment store operation.
    pub fn store_op(&self) -> super::types::AttachmentStoreOp {
        self.store_op
    }

    /// Get the clear value, if set.
    pub fn clear_value(&self) -> Option<super::types::ClearValue> {
        self.clear_value
    }
}

/// Describes the type of access a pass has to a resource.
/// This helps in determining synchronization requirements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceAccessType {
    Read,
    Write,
    ReadWrite,
}

/// Tracks the lifetime of a resource across the render graph.
/// This information is used for resource pooling and optimization.
#[derive(Debug, Clone)]
pub struct ResourceLifetime {
    pub first_use: usize,
    pub last_use: usize,
    pub is_transient: bool,
}

impl std::fmt::Debug for ResourceUsage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResourceUsage")
            .field("resource_id", &self.resource_id)
            .field("access", &self.access)
            .field("stage", &self.stage)
            .field("layout", &self.layout)
            .field("load_op", &self.load_op)
            .field("store_op", &self.store_op)
            .field("has_clear_value", &self.clear_value.is_some())
            .finish()
    }
}

impl ResourceLifetime {
    pub fn new(first_use: usize, last_use: usize, is_transient: bool) -> Self {
        Self {
            first_use,
            last_use,
            is_transient,
        }
    }

    /// Returns true if the resource is still in use at the given pass index
    pub fn is_in_use(&self, pass_index: usize) -> bool {
        pass_index >= self.first_use && pass_index <= self.last_use
    }

    /// Returns true if the resource can be freed after the given pass index
    pub fn can_free_after(&self, pass_index: usize) -> bool {
        pass_index >= self.last_use
    }
}

/// CompiledResource represents a fully allocated Vulkan resource.
/// These are created during graph compilation and used during execution.
pub enum CompiledResource {
    Buffer {
        buffer: super::types::VkBuffer,
        allocation: gpu_allocator::vulkan::Allocation,
        size: u64,
    },
    Image {
        image: super::types::VkImage,
        image_view: super::types::VkImageView,
        allocation: gpu_allocator::vulkan::Allocation,
        extent: super::types::Extent3D,
        format: super::types::ImageFormat,
        layout: super::types::ImageLayout,
    },
    ExternalBuffer {
        buffer: super::types::VkBuffer,
    },
    ExternalImage {
        image: super::types::VkImage,
        image_view: super::types::VkImageView,
        format: super::types::ImageFormat,
        extent: super::types::Extent2D,
    },
}

impl std::fmt::Debug for CompiledResource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompiledResource::Buffer { size, .. } => f
                .debug_struct("CompiledResource::Buffer")
                .field("size", size)
                .finish(),
            CompiledResource::Image { extent, format, .. } => f
                .debug_struct("CompiledResource::Image")
                .field("extent", extent)
                .field("format", format)
                .finish(),
            CompiledResource::ExternalBuffer { .. } => {
                f.debug_struct("CompiledResource::ExternalBuffer").finish()
            }
            CompiledResource::ExternalImage { format, extent, .. } => f
                .debug_struct("CompiledResource::ExternalImage")
                .field("format", format)
                .field("extent", extent)
                .finish(),
        }
    }
}

impl ResourceId {
    pub fn new(id: u32) -> Self {
        ResourceId(id)
    }
}

impl AddAssign<u32> for ResourceId {
    fn add_assign(&mut self, other: u32) {
        self.0 += other;
    }
}

impl Resource {
    pub fn new(id: ResourceId, name: impl Into<String>, kind: ResourceKind) -> Self {
        Resource {
            id,
            name: name.into(),
            kind,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_creation() {
        let kind = ResourceKind::Buffer {
            size: 1024,
            usage: vec![crate::types::BufferUsage::VertexBuffer],
            memory_properties: vec![crate::types::MemoryProperty::DeviceLocal],
        };
        let resource = Resource::new(ResourceId(0), "test_buffer", kind);
        assert_eq!(resource.id.0, 0);
        assert_eq!(resource.name, "test_buffer");
    }

    #[test]
    fn test_resource_usage_builder() {
        let usage = ResourceUsage::new(ResourceId(0))
            .with_read(
                crate::types::Access::VertexAttributeRead,
                crate::types::PipelineStage::VertexInput,
            )
            .with_layout(crate::types::ImageLayout::ShaderReadOnlyOptimal);

        assert!(usage
            .access()
            .contains(&crate::types::Access::VertexAttributeRead));
        assert!(usage
            .stage()
            .contains(&crate::types::PipelineStage::VertexInput));
        assert_eq!(
            usage.layout(),
            crate::types::ImageLayout::ShaderReadOnlyOptimal
        );
    }

    #[test]
    fn test_resource_lifetime() {
        let lifetime = ResourceLifetime::new(0, 5, true);
        assert!(lifetime.is_in_use(3));
        assert!(!lifetime.is_in_use(6));
        assert!(lifetime.can_free_after(5));
        assert!(!lifetime.can_free_after(4));
    }
}
