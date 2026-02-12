use std::ops::AddAssign;

use ash::vk;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ResourceId(pub(crate) u32);

// TODO: Fields `id` and `name` are never read (clippy warning)
// Either use these fields or remove them
#[derive(Debug)]
#[allow(dead_code)]
pub struct Resource {
    pub(crate) id: ResourceId,
    pub(crate) name: String,
    pub(crate) kind: ResourceKind,
}

#[derive(Debug)]
pub enum ResourceKind {
    Buffer {
        size: u64,
        usage: vk::BufferUsageFlags,
        memory_properties: vk::MemoryPropertyFlags,
    },
    Image {
        extent: vk::Extent3D,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
        samples: vk::SampleCountFlags,
        tiling: vk::ImageTiling,
        initial_layout: vk::ImageLayout,
        final_layout: vk::ImageLayout,
    },
    ExternalBuffer {
        vk_buffer: vk::Buffer,
    },
    ExternalImage {
        vk_image: vk::Image,
        image_view: vk::ImageView,
        format: vk::Format,
        extent: vk::Extent2D,
    },
}

/// Tracks how a resource is used within a specific pass.
/// This information is used for synchronization and determining
/// the necessary Vulkan barriers between passes.
#[derive(Clone)]
pub struct ResourceUsage {
    pub(crate) resource_id: ResourceId,
    pub(crate) access: vk::AccessFlags,
    pub(crate) stage: vk::PipelineStageFlags,
    pub(crate) layout: vk::ImageLayout,
    pub(crate) load_op: vk::AttachmentLoadOp,
    pub(crate) store_op: vk::AttachmentStoreOp,
    pub clear_value: Option<super::types::ClearValue>,
}

impl ResourceUsage {
    pub fn new(resource_id: ResourceId) -> Self {
        Self {
            resource_id,
            access: vk::AccessFlags::empty(),
            stage: vk::PipelineStageFlags::empty(),
            layout: vk::ImageLayout::UNDEFINED,
            load_op: vk::AttachmentLoadOp::DONT_CARE,
            store_op: vk::AttachmentStoreOp::DONT_CARE,
            clear_value: None,
        }
    }

    pub fn with_read(
        mut self,
        access: super::types::Access,
        stage: super::types::PipelineStage,
    ) -> Self {
        self.access |= access.to_vk_flags();
        self.stage |= stage.to_vk_flags();
        self
    }

    pub fn with_write(
        mut self,
        access: super::types::Access,
        stage: super::types::PipelineStage,
    ) -> Self {
        self.access |= access.to_vk_flags();
        self.stage |= stage.to_vk_flags();
        self
    }

    pub fn with_layout(mut self, layout: super::types::ImageLayout) -> Self {
        self.layout = layout.into();
        self
    }

    pub fn with_load_op(mut self, load_op: super::types::AttachmentLoadOp) -> Self {
        self.load_op = load_op.into();
        self
    }

    pub fn with_store_op(mut self, store_op: super::types::AttachmentStoreOp) -> Self {
        self.store_op = store_op.into();
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
    pub fn access(&self) -> vk::AccessFlags {
        self.access
    }

    /// Get the pipeline stage flags.
    pub fn stage(&self) -> vk::PipelineStageFlags {
        self.stage
    }

    /// Get the image layout.
    pub fn layout(&self) -> vk::ImageLayout {
        self.layout
    }

    /// Get the attachment load operation.
    pub fn load_op(&self) -> vk::AttachmentLoadOp {
        self.load_op
    }

    /// Get the attachment store operation.
    pub fn store_op(&self) -> vk::AttachmentStoreOp {
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
            .field("clear_value", &self.clear_value.is_some())
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
#[derive(Debug)]
pub enum CompiledResource {
    Buffer {
        buffer: vk::Buffer,
        allocation: gpu_allocator::vulkan::Allocation,
        size: u64,
    },
    Image {
        image: vk::Image,
        image_view: vk::ImageView,
        allocation: gpu_allocator::vulkan::Allocation,
        extent: vk::Extent3D,
        format: vk::Format,
        layout: vk::ImageLayout,
    },
    ExternalBuffer {
        buffer: vk::Buffer,
    },
    ExternalImage {
        image: vk::Image,
        image_view: vk::ImageView,
        format: vk::Format,
        extent: vk::Extent2D,
    },
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
            usage: vk::BufferUsageFlags::VERTEX_BUFFER,
            memory_properties: vk::MemoryPropertyFlags::DEVICE_LOCAL,
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
            .access
            .contains(vk::AccessFlags::VERTEX_ATTRIBUTE_READ));
        assert!(usage.stage.contains(vk::PipelineStageFlags::VERTEX_INPUT));
        assert_eq!(usage.layout, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
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
