use super::*;

impl VulkanRenderer {
    /// Allocate independently mutable joint storage for every reusable frame slot.
    pub fn create_skeleton(&mut self, joint_count: usize) -> Result<SkeletonHandle, RendererError> {
        if joint_count == 0 {
            return Err(RendererError::InvalidOperation(
                "Skeleton requires at least one joint".into(),
            ));
        }
        let identity: [f32; 16] = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let matrices = vec![identity; joint_count];
        let desc = crate::render_graph::BufferDesc::new(
            (joint_count * 64) as u64,
            crate::render_graph::BufferUsages::STORAGE
                | crate::render_graph::BufferUsages::TRANSFER_DESTINATION,
            crate::render_graph::BufferMemoryPolicy::CpuVisible,
        );
        let mut handles = Vec::new();
        for _ in 0..FRAMES_IN_FLIGHT {
            match self.create_buffer_with_data(desc, bytemuck::cast_slice(&matrices)) {
                Ok(handle) => handles.push(handle),
                Err(error) => {
                    for handle in handles {
                        self.graph_buffers.remove(handle);
                    }
                    return Err(error);
                }
            }
        }
        Ok(self.skeleton_buffers.insert(handles))
    }
}
