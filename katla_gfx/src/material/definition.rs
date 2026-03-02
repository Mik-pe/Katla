//! Material definition trait and key types.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::ImageFormat;
use crate::vulkan::descriptor::{DescriptorSetLayoutBuilder, LayoutBinding};
use crate::vulkan::pipeline_state::{DescriptorType, ShaderStages};
use crate::vulkan::vertexbinding::VertexBinding;

pub use crate::vulkan::material::descriptor::{RenderState, ShaderSource};

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

/// Trait that describes a material's pipeline requirements.
///
/// This trait provides all the information needed to create a Vulkan pipeline
/// without dictating how pipelines are cached or managed.
///
/// # Thread Safety
///
/// Note: The MaterialDefinition trait does not require `Send + Sync` because material
/// implementations often contain `Rc<RefCell<MaterialPipeline>>` for pipeline
/// storage. Materials are typically used on the main render thread only.
pub trait MaterialDefinition: 'static {
    // === Required: Shaders ===

    /// Returns the vertex shader source.
    fn vertex_shader(&self) -> ShaderSource;

    /// Returns the fragment shader source.
    fn fragment_shader(&self) -> ShaderSource;

    // === Required: Pipeline Config ===

    /// Returns the vertex binding description for this material's vertex format.
    ///
    /// For fullscreen/post-process materials, return an empty `VertexBinding`.
    fn vertex_binding(&self) -> VertexBinding;

    /// Returns the render state (depth, blending, culling configuration).
    fn render_state(&self) -> RenderState;

    // === Required: Descriptor Layouts ===

    /// Returns descriptor set layout builders for each set needed by this material.
    ///
    /// Most materials return a single builder for set 0 (frame/object uniforms).
    /// Materials with textures or additional resources may need multiple sets.
    fn descriptor_layouts(&self) -> Vec<DescriptorSetLayoutBuilder>;

    // === Optional: Pipeline Config ===

    /// Returns the color attachment format for this material.
    ///
    /// Default is HDR format (R16G16B16A16Sfloat) for proper tonemapping.
    fn color_format(&self) -> ImageFormat {
        ImageFormat::R16G16B16A16Sfloat
    }

    /// Returns the depth attachment format for this material.
    ///
    /// Default is D32SfloatS8Uint (depth + stencil).
    fn depth_format(&self) -> ImageFormat {
        ImageFormat::D32SfloatS8Uint
    }

    /// Returns the material domain for render pass organization.
    ///
    /// Default is Surface for standard 3D materials.
    fn domain(&self) -> MaterialDomain {
        MaterialDomain::Surface
    }

    // === Optional: Descriptor Layout ===

    /// Returns true if this material uses PBR textures (5 textures).
    ///
    /// Default is false (single texture).
    fn uses_pbr_textures(&self) -> bool {
        false
    }

    /// Returns true if this material needs a skeleton descriptor set.
    ///
    /// Default is false (no skeletal animation).
    fn uses_skeleton(&self) -> bool {
        false
    }

    /// Returns true if this material uses bindless textures.
    ///
    /// Default is false (individual texture descriptors).
    fn uses_bindless(&self) -> bool {
        false
    }

    // === Derived ===

    /// Returns true if this material uses alpha blending.
    ///
    /// Derived from render_state().alpha_blending by default.
    fn is_transparent(&self) -> bool {
        self.render_state().alpha_blending
    }
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
    /// Create a key from a MaterialDefinition implementation.
    pub fn from_material<M: MaterialDefinition + ?Sized>(material: &M) -> Self {
        Self {
            vertex_shader_hash: hash_shader(&material.vertex_shader()),
            fragment_shader_hash: hash_shader(&material.fragment_shader()),
            vertex_binding_hash: hash_vertex_binding(&material.vertex_binding()),
            render_state_hash: hash_render_state(&material.render_state()),
            layout_hash: hash_layouts(&material.descriptor_layouts()),
            domain: material.domain(),
            uses_pbr: material.uses_pbr_textures(),
            uses_skeleton: material.uses_skeleton(),
            uses_bindless: material.uses_bindless(),
        }
    }
}

//=============================================================================
// Hashing Helpers
//=============================================================================

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
