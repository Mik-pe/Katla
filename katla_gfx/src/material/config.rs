//! Built-in material configurations.

use std::path::PathBuf;

use crate::ShaderStages;
use crate::vulkan::descriptor::DescriptorSetLayoutBuilder;
use crate::vulkan::pipeline_state::DescriptorType;
use crate::vulkan::vertexbinding::VertexBinding;

use super::{MaterialDefinition, MaterialDomain, RenderState, ShaderSource};

/// Standard PBR material config with storage buffers and single texture.
///
/// This is the most common material type for 3D objects.
/// Uses two descriptor sets:
/// - Set 0: frame_data + objects storage buffers
/// - Set 1: albedo texture + sampler
#[derive(Clone, Debug)]
pub struct PbrMaterialConfig {
    pub vertex_binding: VertexBinding,
    pub shader_path: PathBuf,
}

impl PbrMaterialConfig {
    /// Create a new PBR material config.
    pub fn new(vertex_binding: VertexBinding, shader_path: PathBuf) -> Self {
        Self {
            vertex_binding,
            shader_path,
        }
    }
}

impl MaterialDefinition for PbrMaterialConfig {
    fn vertex_shader(&self) -> ShaderSource {
        ShaderSource::WgslFile(self.shader_path.clone())
    }

    fn fragment_shader(&self) -> ShaderSource {
        ShaderSource::WgslFile(self.shader_path.clone())
    }

    fn vertex_binding(&self) -> VertexBinding {
        self.vertex_binding.clone()
    }

    fn render_state(&self) -> RenderState {
        RenderState {
            depth_test: true,
            depth_write: true,
            cull_backfaces: true,
            alpha_blending: false,
        }
    }

    fn descriptor_layouts(&self) -> Vec<DescriptorSetLayoutBuilder> {
        vec![
            // Set 0: Storage buffers
            DescriptorSetLayoutBuilder::new()
                .add_binding(
                    0,
                    DescriptorType::StorageBuffer,
                    ShaderStages::VERTEX_FRAGMENT,
                )
                .add_binding(
                    1,
                    DescriptorType::StorageBuffer,
                    ShaderStages::VERTEX_FRAGMENT,
                ),
            // Set 1: Textures
            DescriptorSetLayoutBuilder::new()
                .add_binding(0, DescriptorType::SampledImage, ShaderStages::FRAGMENT)
                .add_binding(1, DescriptorType::Sampler, ShaderStages::FRAGMENT),
        ]
    }

    fn domain(&self) -> MaterialDomain {
        MaterialDomain::Surface
    }
}

/// Skinned PBR material config for skeletal animation.
///
/// Uses three descriptor sets:
/// - Set 0: frame_data + objects storage buffers
/// - Set 1: albedo texture + sampler
/// - Set 2: skeleton joint matrices
#[derive(Clone, Debug)]
pub struct SkinnedPbrMaterialConfig {
    pub vertex_binding: VertexBinding,
    pub shader_path: PathBuf,
}

impl SkinnedPbrMaterialConfig {
    /// Create a new skinned PBR material config.
    pub fn new(vertex_binding: VertexBinding, shader_path: PathBuf) -> Self {
        Self {
            vertex_binding,
            shader_path,
        }
    }
}

impl MaterialDefinition for SkinnedPbrMaterialConfig {
    fn vertex_shader(&self) -> ShaderSource {
        ShaderSource::WgslFile(self.shader_path.clone())
    }

    fn fragment_shader(&self) -> ShaderSource {
        ShaderSource::WgslFile(self.shader_path.clone())
    }

    fn vertex_binding(&self) -> VertexBinding {
        self.vertex_binding.clone()
    }

    fn render_state(&self) -> RenderState {
        RenderState {
            depth_test: true,
            depth_write: true,
            cull_backfaces: true,
            alpha_blending: false,
        }
    }

    fn descriptor_layouts(&self) -> Vec<DescriptorSetLayoutBuilder> {
        vec![
            // Set 0: Storage buffers
            DescriptorSetLayoutBuilder::new()
                .add_binding(
                    0,
                    DescriptorType::StorageBuffer,
                    ShaderStages::VERTEX_FRAGMENT,
                )
                .add_binding(
                    1,
                    DescriptorType::StorageBuffer,
                    ShaderStages::VERTEX_FRAGMENT,
                ),
            // Set 1: Textures
            DescriptorSetLayoutBuilder::new()
                .add_binding(0, DescriptorType::SampledImage, ShaderStages::FRAGMENT)
                .add_binding(1, DescriptorType::Sampler, ShaderStages::FRAGMENT),
            // Set 2: Skeleton
            DescriptorSetLayoutBuilder::new().add_binding(
                0,
                DescriptorType::StorageBuffer,
                ShaderStages::VERTEX_FRAGMENT,
            ),
        ]
    }

    fn domain(&self) -> MaterialDomain {
        MaterialDomain::Surface
    }

    fn uses_skeleton(&self) -> bool {
        true
    }
}

/// Full PBR material config with 5 texture maps.
///
/// Uses two descriptor sets with 10 texture bindings:
/// - Set 0: frame_data + objects storage buffers
/// - Set 1: albedo + normal + metallic/roughness + occlusion + emission (each with sampler)
#[derive(Clone, Debug)]
pub struct FullPbrMaterialConfig {
    pub vertex_binding: VertexBinding,
    pub shader_path: PathBuf,
}

impl FullPbrMaterialConfig {
    /// Create a new full PBR material config.
    pub fn new(vertex_binding: VertexBinding, shader_path: PathBuf) -> Self {
        Self {
            vertex_binding,
            shader_path,
        }
    }
}

impl MaterialDefinition for FullPbrMaterialConfig {
    fn vertex_shader(&self) -> ShaderSource {
        ShaderSource::WgslFile(self.shader_path.clone())
    }

    fn fragment_shader(&self) -> ShaderSource {
        ShaderSource::WgslFile(self.shader_path.clone())
    }

    fn vertex_binding(&self) -> VertexBinding {
        self.vertex_binding.clone()
    }

    fn render_state(&self) -> RenderState {
        RenderState {
            depth_test: true,
            depth_write: true,
            cull_backfaces: true,
            alpha_blending: false,
        }
    }

    fn descriptor_layouts(&self) -> Vec<DescriptorSetLayoutBuilder> {
        vec![
            // Set 0: Storage buffers
            DescriptorSetLayoutBuilder::new()
                .add_binding(
                    0,
                    DescriptorType::StorageBuffer,
                    ShaderStages::VERTEX_FRAGMENT,
                )
                .add_binding(
                    1,
                    DescriptorType::StorageBuffer,
                    ShaderStages::VERTEX_FRAGMENT,
                ),
            // Set 1: PBR textures (5 textures + 5 samplers)
            DescriptorSetLayoutBuilder::new()
                .add_binding(0, DescriptorType::SampledImage, ShaderStages::FRAGMENT) // albedo
                .add_binding(1, DescriptorType::Sampler, ShaderStages::FRAGMENT)
                .add_binding(2, DescriptorType::SampledImage, ShaderStages::FRAGMENT) // normal
                .add_binding(3, DescriptorType::Sampler, ShaderStages::FRAGMENT)
                .add_binding(4, DescriptorType::SampledImage, ShaderStages::FRAGMENT) // metallic/roughness
                .add_binding(5, DescriptorType::Sampler, ShaderStages::FRAGMENT)
                .add_binding(6, DescriptorType::SampledImage, ShaderStages::FRAGMENT) // occlusion
                .add_binding(7, DescriptorType::Sampler, ShaderStages::FRAGMENT)
                .add_binding(8, DescriptorType::SampledImage, ShaderStages::FRAGMENT) // emission
                .add_binding(9, DescriptorType::Sampler, ShaderStages::FRAGMENT),
        ]
    }

    fn domain(&self) -> MaterialDomain {
        MaterialDomain::Surface
    }

    fn uses_pbr_textures(&self) -> bool {
        true
    }
}

/// Bindless PBR material config.
///
/// Uses two descriptor sets:
/// - Set 0: frame_data + objects storage buffers
/// - Set 1: bindless texture array (provided externally)
#[derive(Clone, Debug)]
pub struct BindlessPbrMaterialConfig {
    pub vertex_binding: VertexBinding,
    pub shader_path: PathBuf,
}

impl BindlessPbrMaterialConfig {
    /// Create a new bindless PBR material config.
    pub fn new(vertex_binding: VertexBinding, shader_path: PathBuf) -> Self {
        Self {
            vertex_binding,
            shader_path,
        }
    }
}

impl MaterialDefinition for BindlessPbrMaterialConfig {
    fn vertex_shader(&self) -> ShaderSource {
        ShaderSource::WgslFile(self.shader_path.clone())
    }

    fn fragment_shader(&self) -> ShaderSource {
        ShaderSource::WgslFile(self.shader_path.clone())
    }

    fn vertex_binding(&self) -> VertexBinding {
        self.vertex_binding.clone()
    }

    fn render_state(&self) -> RenderState {
        RenderState {
            depth_test: true,
            depth_write: true,
            cull_backfaces: true,
            alpha_blending: false,
        }
    }

    fn descriptor_layouts(&self) -> Vec<DescriptorSetLayoutBuilder> {
        vec![
            // Set 0: Storage buffers (Set 1 is bindless, provided externally)
            DescriptorSetLayoutBuilder::new()
                .add_binding(
                    0,
                    DescriptorType::StorageBuffer,
                    ShaderStages::VERTEX_FRAGMENT,
                )
                .add_binding(
                    1,
                    DescriptorType::StorageBuffer,
                    ShaderStages::VERTEX_FRAGMENT,
                ),
        ]
    }

    fn domain(&self) -> MaterialDomain {
        MaterialDomain::Surface
    }

    fn uses_bindless(&self) -> bool {
        true
    }
}

/// Bindless skinned PBR material config.
///
/// Uses three descriptor sets:
/// - Set 0: frame_data + objects storage buffers
/// - Set 1: bindless texture array (provided externally)
/// - Set 2: skeleton joint matrices
#[derive(Clone, Debug)]
pub struct BindlessSkinnedPbrMaterialConfig {
    pub vertex_binding: VertexBinding,
    pub shader_path: PathBuf,
}

impl BindlessSkinnedPbrMaterialConfig {
    /// Create a new bindless skinned PBR material config.
    pub fn new(vertex_binding: VertexBinding, shader_path: PathBuf) -> Self {
        Self {
            vertex_binding,
            shader_path,
        }
    }
}

impl MaterialDefinition for BindlessSkinnedPbrMaterialConfig {
    fn vertex_shader(&self) -> ShaderSource {
        ShaderSource::WgslFile(self.shader_path.clone())
    }

    fn fragment_shader(&self) -> ShaderSource {
        ShaderSource::WgslFile(self.shader_path.clone())
    }

    fn vertex_binding(&self) -> VertexBinding {
        self.vertex_binding.clone()
    }

    fn render_state(&self) -> RenderState {
        RenderState {
            depth_test: true,
            depth_write: true,
            cull_backfaces: true,
            alpha_blending: false,
        }
    }

    fn descriptor_layouts(&self) -> Vec<DescriptorSetLayoutBuilder> {
        vec![
            // Set 0: Storage buffers
            DescriptorSetLayoutBuilder::new()
                .add_binding(
                    0,
                    DescriptorType::StorageBuffer,
                    ShaderStages::VERTEX_FRAGMENT,
                )
                .add_binding(
                    1,
                    DescriptorType::StorageBuffer,
                    ShaderStages::VERTEX_FRAGMENT,
                ),
            // Set 2: Skeleton (Set 1 is bindless, provided externally)
            DescriptorSetLayoutBuilder::new().add_binding(
                0,
                DescriptorType::StorageBuffer,
                ShaderStages::VERTEX_FRAGMENT,
            ),
        ]
    }

    fn domain(&self) -> MaterialDomain {
        MaterialDomain::Surface
    }

    fn uses_bindless(&self) -> bool {
        true
    }

    fn uses_skeleton(&self) -> bool {
        true
    }
}

/// Dynamic material config created from a MaterialDescriptor.
///
/// This config reads all properties from the descriptor, making it suitable
/// for loading arbitrary materials from TOML files.
#[derive(Clone, Debug)]
pub struct DynamicMaterialConfig {
    vertex_binding: VertexBinding,
    shader_path: PathBuf,
    render_state: RenderState,
    domain: MaterialDomain,
    uses_pbr: bool,
    uses_skeleton: bool,
    uses_bindless: bool,
}

impl DynamicMaterialConfig {
    /// Create a dynamic config from a MaterialDescriptor.
    ///
    /// # Arguments
    /// * `descriptor` - The material descriptor loaded from TOML
    /// * `vertex_binding` - The vertex binding for this material
    /// * `uses_pbr` - Whether this material uses 5 PBR textures
    /// * `uses_skeleton` - Whether this material uses skeletal animation
    /// * `uses_bindless` - Whether this material uses bindless textures
    pub fn new(
        descriptor: &crate::vulkan::material::MaterialDescriptor,
        vertex_binding: VertexBinding,
        uses_pbr: bool,
        uses_skeleton: bool,
        uses_bindless: bool,
    ) -> Self {
        // Extract shader path
        let shader_path = match &descriptor.vertex_shader {
            ShaderSource::WgslFile(path) => path.clone(),
            _ => PathBuf::from("unknown.wgsl"),
        };

        // Use descriptor's render state directly
        let render_state = descriptor.render_state.clone();

        Self {
            vertex_binding,
            shader_path,
            render_state,
            domain: MaterialDomain::Surface,
            uses_pbr,
            uses_skeleton,
            uses_bindless,
        }
    }

    /// Create a standard PBR config.
    pub fn pbr(
        descriptor: &crate::vulkan::material::MaterialDescriptor,
        vertex_binding: VertexBinding,
    ) -> Self {
        Self::new(descriptor, vertex_binding, false, false, false)
    }

    /// Create a skinned PBR config.
    pub fn skinned(
        descriptor: &crate::vulkan::material::MaterialDescriptor,
        vertex_binding: VertexBinding,
    ) -> Self {
        Self::new(descriptor, vertex_binding, false, true, false)
    }

    /// Create a full PBR config (5 textures).
    pub fn full_pbr(
        descriptor: &crate::vulkan::material::MaterialDescriptor,
        vertex_binding: VertexBinding,
    ) -> Self {
        Self::new(descriptor, vertex_binding, true, false, false)
    }

    /// Create a bindless config.
    pub fn bindless(
        descriptor: &crate::vulkan::material::MaterialDescriptor,
        vertex_binding: VertexBinding,
    ) -> Self {
        Self::new(descriptor, vertex_binding, false, false, true)
    }

    /// Create a bindless skinned config.
    pub fn bindless_skinned(
        descriptor: &crate::vulkan::material::MaterialDescriptor,
        vertex_binding: VertexBinding,
    ) -> Self {
        Self::new(descriptor, vertex_binding, false, true, true)
    }
}

impl MaterialDefinition for DynamicMaterialConfig {
    fn vertex_shader(&self) -> ShaderSource {
        ShaderSource::WgslFile(self.shader_path.clone())
    }

    fn fragment_shader(&self) -> ShaderSource {
        ShaderSource::WgslFile(self.shader_path.clone())
    }

    fn vertex_binding(&self) -> VertexBinding {
        self.vertex_binding.clone()
    }

    fn render_state(&self) -> RenderState {
        self.render_state.clone()
    }

    fn descriptor_layouts(&self) -> Vec<DescriptorSetLayoutBuilder> {
        // Set 0: Storage buffers (always present)
        let mut layouts = vec![
            DescriptorSetLayoutBuilder::new()
                .add_binding(
                    0,
                    DescriptorType::StorageBuffer,
                    ShaderStages::VERTEX_FRAGMENT,
                )
                .add_binding(
                    1,
                    DescriptorType::StorageBuffer,
                    ShaderStages::VERTEX_FRAGMENT,
                ),
        ];

        if self.uses_bindless {
            // Set 1 is bindless (provided externally)
            // Set 2: Skeleton if needed
            if self.uses_skeleton {
                layouts.push(DescriptorSetLayoutBuilder::new().add_binding(
                    0,
                    DescriptorType::StorageBuffer,
                    ShaderStages::VERTEX_FRAGMENT,
                ));
            }
        } else {
            // Non-bindless: Set 1 contains textures
            if self.uses_pbr {
                // Full PBR: 5 textures + 5 samplers
                layouts.push(
                    DescriptorSetLayoutBuilder::new()
                        .add_binding(0, DescriptorType::SampledImage, ShaderStages::FRAGMENT)
                        .add_binding(1, DescriptorType::Sampler, ShaderStages::FRAGMENT)
                        .add_binding(2, DescriptorType::SampledImage, ShaderStages::FRAGMENT)
                        .add_binding(3, DescriptorType::Sampler, ShaderStages::FRAGMENT)
                        .add_binding(4, DescriptorType::SampledImage, ShaderStages::FRAGMENT)
                        .add_binding(5, DescriptorType::Sampler, ShaderStages::FRAGMENT)
                        .add_binding(6, DescriptorType::SampledImage, ShaderStages::FRAGMENT)
                        .add_binding(7, DescriptorType::Sampler, ShaderStages::FRAGMENT)
                        .add_binding(8, DescriptorType::SampledImage, ShaderStages::FRAGMENT)
                        .add_binding(9, DescriptorType::Sampler, ShaderStages::FRAGMENT),
                );
            } else {
                // Standard: 1 texture + 1 sampler
                layouts.push(
                    DescriptorSetLayoutBuilder::new()
                        .add_binding(0, DescriptorType::SampledImage, ShaderStages::FRAGMENT)
                        .add_binding(1, DescriptorType::Sampler, ShaderStages::FRAGMENT),
                );
            }

            // Set 2: Skeleton if needed
            if self.uses_skeleton {
                layouts.push(DescriptorSetLayoutBuilder::new().add_binding(
                    0,
                    DescriptorType::StorageBuffer,
                    ShaderStages::VERTEX_FRAGMENT,
                ));
            }
        }

        layouts
    }

    fn domain(&self) -> MaterialDomain {
        self.domain
    }

    fn uses_pbr_textures(&self) -> bool {
        self.uses_pbr
    }

    fn uses_skeleton(&self) -> bool {
        self.uses_skeleton
    }

    fn uses_bindless(&self) -> bool {
        self.uses_bindless
    }
}
