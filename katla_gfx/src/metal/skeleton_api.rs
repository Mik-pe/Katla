use crate::backend::resource::GpuBuffer;
use crate::error::RendererError;
use crate::handle::SkeletonHandle;

use super::metal_renderer::MetalRenderer;

impl MetalRenderer {
    pub(crate) fn create_skeleton_impl(
        &mut self,
        joint_count: usize,
    ) -> Result<SkeletonHandle, RendererError> {
        let mut handle = SkeletonHandle::NONE;
        for storage in &mut self.skeletons {
            let buffer = self
                .context
                .create_buffer((joint_count * 64) as u64, true)?;
            let identity: [f32; 16] = [
                1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
            ];
            unsafe {
                let dst = buffer.map().cast::<[f32; 16]>();
                for joint in 0..joint_count {
                    dst.add(joint).write(identity);
                }
            }
            handle = storage.insert(buffer);
        }
        Ok(handle)
    }

    pub(crate) fn destroy_skeleton_impl(&mut self, handle: SkeletonHandle) {
        for storage in &mut self.skeletons {
            storage.remove(handle);
        }
    }
}
