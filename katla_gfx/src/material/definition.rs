//! Material key types for pipeline caching.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::vulkan::descriptor::{DescriptorSetLayoutBuilder, LayoutBinding};
use crate::vulkan::pipeline_state::{DescriptorType, ShaderStages};
use crate::vulkan::vertexbinding::VertexBinding;

pub use crate::vulkan::material::descriptor::{RenderState, ShaderSource};

use super::template::MaterialTemplateConfig;

/// Material domain for render pass organization.
///
/// Materials are grouped by domain to ensure proper render ordering
/// and pipeline compatibility. This is separate from descriptor layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MaterialDomain {
    /// Standard 3D surface rendering (opaque and transparent objects)
    Surface,
    /// 2D UI overlay (rendered after scene, no depth testing against scene)
    Ui,
    /// Fullscreen post-processing effects (no vertex data, single quad)
    PostProcess,
    /// GPU particle rendering (compute-generated geometry)
    Particle,
}

/// Key for pipeline caching and deduplication.
///
/// This key uniquely identifies a pipeline configuration by hashing all
/// material properties that affect pipeline creation. Two materials with
/// the same key can share the same pipeline.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MaterialKey {
    /// Hash of the vertex shader source
    pub vertex_shader_hash: u64,
    /// Hash of the fragment shader source
    pub fragment_shader_hash: u64,
    /// Hash of the vertex binding configuration
    pub vertex_binding_hash: u64,
    /// Hash of the render state configuration
    pub render_state_hash: u64,
    /// Hash of the descriptor layouts
    pub layout_hash: u64,
    /// Material domain
    pub domain: MaterialDomain,
    /// Uses PBR textures (5 textures)
    pub uses_pbr: bool,
    /// Uses skeleton descriptor set
    pub uses_skeleton: bool,
    /// Uses bindless textures
    pub uses_bindless: bool,
}

impl MaterialKey {
    /// Create a key from a MaterialTemplateConfig.
    pub fn from_template_config(config: &MaterialTemplateConfig) -> Self {
        static EMPTY_SHADER: ShaderSource = ShaderSource::WgslString(String::new());
        let vertex_shader = config.vertex_shader().unwrap_or(&EMPTY_SHADER);
        let fragment_shader = config.fragment_shader().unwrap_or(&EMPTY_SHADER);

        Self {
            vertex_shader_hash: hash_shader(vertex_shader),
            fragment_shader_hash: hash_shader(fragment_shader),
            vertex_binding_hash: config
                .vertex_binding()
                .map(hash_vertex_binding)
                .unwrap_or(0),
            render_state_hash: hash_render_state(config.render_state()),
            layout_hash: hash_template_layouts(config.descriptor_layouts()),
            domain: config.domain(),
            uses_pbr: false,
            uses_skeleton: config.uses_skeleton(),
            uses_bindless: config.uses_bindless(),
        }
    }
}

//=============================================================================
// Hashing Helpers
//=============================================================================

use super::template::DescriptorSetLayout;

/// Hash a shader source.
pub(crate) fn hash_shader(shader: &ShaderSource) -> u64 {
    let mut hasher = DefaultHasher::new();
    match shader {
        ShaderSource::WgslFile(path) => {
            "file".hash(&mut hasher);
            path.hash(&mut hasher);
        }
        ShaderSource::WgslString(s) => {
            "string".hash(&mut hasher);
            s.hash(&mut hasher);
        }
        ShaderSource::PreCompiled(bytes) => {
            "spirv".hash(&mut hasher);
            bytes.len().hash(&mut hasher);
            // Hash first 64 bytes of SPIR-V as a fingerprint
            let fingerprint_len = bytes.len().min(64);
            bytes[..fingerprint_len].hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// Hash a vertex binding configuration.
pub(crate) fn hash_vertex_binding(binding: &VertexBinding) -> u64 {
    let mut hasher = DefaultHasher::new();
    binding.formats.len().hash(&mut hasher);
    for format in &binding.formats {
        format.hash(&mut hasher);
    }
    hasher.finish()
}

/// Hash a render state configuration.
pub(crate) fn hash_render_state(state: &RenderState) -> u64 {
    let mut hasher = DefaultHasher::new();
    state.depth_test.hash(&mut hasher);
    state.depth_write.hash(&mut hasher);
    state.cull_backfaces.hash(&mut hasher);
    state.alpha_blending.hash(&mut hasher);
    hasher.finish()
}

/// Hash descriptor layouts.
pub(crate) fn hash_layouts(layouts: &[DescriptorSetLayoutBuilder]) -> u64 {
    let mut hasher = DefaultHasher::new();
    layouts.len().hash(&mut hasher);
    for layout in layouts {
        hash_layout(layout, &mut hasher);
    }
    hasher.finish()
}

/// Hash template descriptor layouts by set indices.
pub(crate) fn hash_template_layouts(layouts: &[DescriptorSetLayout]) -> u64 {
    let mut hasher = DefaultHasher::new();
    layouts.len().hash(&mut hasher);
    for layout in layouts {
        layout.set_index().hash(&mut hasher);
    }
    hasher.finish()
}

/// Hash a single descriptor set layout.
pub(crate) fn hash_layout(layout: &DescriptorSetLayoutBuilder, hasher: &mut DefaultHasher) {
    let bindings = layout.bindings();
    bindings.len().hash(hasher);
    for binding in bindings {
        hash_descriptor_binding(binding, hasher);
    }
    // Hash push_descriptor flag
    layout.is_push_descriptor().hash(hasher);
}

/// Hash a single descriptor binding.
pub(crate) fn hash_descriptor_binding(binding: &LayoutBinding, hasher: &mut DefaultHasher) {
    binding.binding.hash(hasher);
    hash_descriptor_type(&binding.descriptor_type, hasher);
    binding.descriptor_count.hash(hasher);
    hash_shader_stages(&binding.shader_stages, hasher);
}

/// Hash a descriptor type.
pub(crate) fn hash_descriptor_type(ty: &DescriptorType, hasher: &mut DefaultHasher) {
    // Use discriminant for stable hashing across versions
    let discriminant = match ty {
        DescriptorType::StorageBuffer => 0u8,
        DescriptorType::SampledImage => 1,
        DescriptorType::Sampler => 2,
    };
    discriminant.hash(hasher);
}

/// Hash shader stages.
pub(crate) fn hash_shader_stages(stages: &ShaderStages, hasher: &mut DefaultHasher) {
    stages.vertex.hash(hasher);
    stages.fragment.hash(hasher);
    stages.compute.hash(hasher);
    stages.geometry.hash(hasher);
    stages.tessellation_control.hash(hasher);
    stages.tessellation_evaluation.hash(hasher);
}

//=============================================================================
// Tests
//=============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_material_key_deduplication() {
        // Same shader = same hash
        let shader1 = ShaderSource::WgslFile(PathBuf::from("shader.wgsl"));
        let shader2 = ShaderSource::WgslFile(PathBuf::from("shader.wgsl"));
        assert_eq!(hash_shader(&shader1), hash_shader(&shader2));

        // Different shader = different hash
        let shader3 = ShaderSource::WgslFile(PathBuf::from("other.wgsl"));
        assert_ne!(hash_shader(&shader1), hash_shader(&shader3));

        // String vs file = different hash (even if same content)
        let shader4 = ShaderSource::WgslString("content".to_string());
        let shader5 = ShaderSource::WgslFile(PathBuf::from("content"));
        assert_ne!(hash_shader(&shader4), hash_shader(&shader5));
    }

    #[test]
    fn test_render_state_hash() {
        let state1 = RenderState {
            depth_test: true,
            depth_write: true,
            cull_backfaces: false,
            alpha_blending: false,
        };
        let state2 = RenderState {
            depth_test: true,
            depth_write: true,
            cull_backfaces: false,
            alpha_blending: false,
        };
        let state3 = RenderState {
            depth_test: false,
            depth_write: true,
            cull_backfaces: false,
            alpha_blending: false,
        };

        assert_eq!(hash_render_state(&state1), hash_render_state(&state2));
        assert_ne!(hash_render_state(&state1), hash_render_state(&state3));
    }

    #[test]
    fn test_material_domain_hash() {
        let key1 = MaterialKey {
            vertex_shader_hash: 1,
            fragment_shader_hash: 2,
            vertex_binding_hash: 3,
            render_state_hash: 4,
            layout_hash: 5,
            domain: MaterialDomain::Surface,
            uses_pbr: false,
            uses_skeleton: false,
            uses_bindless: false,
        };
        let key2 = MaterialKey {
            vertex_shader_hash: 1,
            fragment_shader_hash: 2,
            vertex_binding_hash: 3,
            render_state_hash: 4,
            layout_hash: 5,
            domain: MaterialDomain::Ui,
            uses_pbr: false,
            uses_skeleton: false,
            uses_bindless: false,
        };

        assert_ne!(key1, key2);
    }

    #[test]
    fn test_descriptor_type_hash_stability() {
        let mut hasher1 = DefaultHasher::new();
        hash_descriptor_type(&DescriptorType::StorageBuffer, &mut hasher1);
        let hash1 = hasher1.finish();

        let mut hasher2 = DefaultHasher::new();
        hash_descriptor_type(&DescriptorType::StorageBuffer, &mut hasher2);
        let hash2 = hasher2.finish();

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, {
            let mut hasher = DefaultHasher::new();
            hash_descriptor_type(&DescriptorType::SampledImage, &mut hasher);
            hasher.finish()
        });
    }
}
