use std::collections::HashMap;

use crate::{Pass, Resource, ResourceId, ResourceKind};

#[derive(Default)]
pub struct RenderGraph {
    pub(crate) passes: Vec<Pass>,
    pub(crate) resources: HashMap<ResourceId, Resource>,
    next_id: ResourceId,
}

impl RenderGraph {
    pub fn add_resource(&mut self, name: impl Into<String>, resource_kind: ResourceKind) {
        let id = self.next_id;
        self.resources
            .insert(id, Resource::new(id, name, resource_kind));
        self.next_id += 1;
    }

    pub fn add_pass(&mut self, pass: Pass) {
        self.passes.push(pass);
    }
}

#[cfg(test)]
mod tests {
    use ash::vk;

    use crate::ResourceKind;

    use super::*;

    #[test]
    fn test_add_resource() {
        let mut graph = RenderGraph::default();
        graph.add_resource(
            "buffer",
            ResourceKind::Buffer {
                size: 1024,
                usage: vk::BufferUsageFlags::VERTEX_BUFFER,
            },
        );
        assert_eq!(graph.resources.len(), 1);
        assert_eq!(graph.next_id, ResourceId(1));
    }
}
