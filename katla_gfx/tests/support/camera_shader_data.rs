//! Shader data for geometry, mesh and pipeline-cache contract fixtures.

use katla_gfx::{ConstantBinding, PassBindings, ShaderStages};

#[derive(Debug, Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub(crate) struct CameraShaderData {
    pub view_matrix: [f32; 16],
    pub proj_matrix: [f32; 16],
    pub inv_view_proj_matrix: [f32; 16],
    pub camera_position: [f32; 4],
}

impl CameraShaderData {
    pub(crate) fn bindings(&self) -> PassBindings {
        PassBindings {
            constants: vec![ConstantBinding {
                group: 0,
                binding: 0,
                stages: ShaderStages::VERTEX,
                bytes: bytemuck::bytes_of(self).to_vec(),
            }],
            ..Default::default()
        }
    }
}
