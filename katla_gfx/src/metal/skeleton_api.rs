use crate::backend::resource::GpuBuffer;
use crate::error::RendererError;
use crate::handle::SkeletonHandle;

use super::metal_renderer::MetalRenderer;

impl MetalRenderer {
    pub(crate) fn create_skeleton_impl(
        &mut self,
        joint_count: usize,
    ) -> Result<SkeletonHandle, RendererError> {
        let buffer_size = (joint_count * 64) as u64;
        let buffer = self.context.create_buffer(buffer_size, true)?;

        let identity: [f32; 16] = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let ptr = buffer.map();
        unsafe {
            let dst = ptr as *mut [f32; 16];
            for i in 0..joint_count {
                dst.add(i).write(identity);
            }
        }
        buffer.unmap();

        let id = self.skeletons.insert(buffer);
        Ok(SkeletonHandle::new(id))
    }

    pub(crate) fn update_skeleton_impl(&mut self, handle: SkeletonHandle, matrices: &[[f32; 16]]) {
        let Some(buffer) = self.skeletons.get_mut(handle.index()) else {
            return;
        };
        let ptr = buffer.map();
        let matrices_bytes = unsafe {
            std::slice::from_raw_parts(matrices.as_ptr() as *const u8, matrices.len() * 64)
        };
        unsafe {
            std::ptr::copy_nonoverlapping(matrices_bytes.as_ptr(), ptr, matrices_bytes.len());
        }
        buffer.unmap();
    }

    pub(crate) fn destroy_skeleton_impl(&mut self, handle: SkeletonHandle) {
        self.skeletons.remove(handle.index());
    }
}
