use std::ops::AddAssign;

use ash::vk;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResourceId(pub(crate) u32);

#[derive(Debug)]
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
    },
    Image {
        width: u32,
        height: u32,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
        layout: vk::ImageLayout,
    },
}

impl ResourceId {
    pub fn new(id: u32) -> Self {
        ResourceId(id)
    }
}

impl Default for ResourceId {
    fn default() -> Self {
        ResourceId(0)
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
    fn test_resource() {
        // Test the resource
    }
}
