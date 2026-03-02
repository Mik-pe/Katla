//! Built-in material configurations.

use std::path::PathBuf;

use bitflags::bitflags;

use crate::vulkan::descriptor::DescriptorSetLayoutBuilder;
use crate::vulkan::pipeline_state::{DescriptorType, ShaderStages};
use crate::vulkan::vertexbinding::VertexBinding;

use super::{MaterialDefinition, MaterialDomain, RenderState, ShaderSource};

bitflags! {
    /// Flags controlling material behavior.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct PbrMaterialFlags: u32 {
        /// Uses bindless texture array (Set 1 provided externally)
        const USES_BINDLESS = 1 << 0;
        /// Uses skeletal animation (adds skeleton descriptor set)
        const USES_SKELETON = 1 << 1;
        /// Uses full PBR textures (5 textures instead of 1)
        const USES_PBR_TEXTURES = 1 << 2;
        /// Uses alpha blending
        const ALPHA_BLENDING = 1 << 3;
    }
}

/// Unified PBR material configuration with flags.
///
/// This config can represent all common PBR material variants through flags:
/// - Basic: storage buffers + single texture
/// - Full PBR: storage buffers + 5 PBR textures
/// - Bindless: storage buffers only (textures provided externally)
/// - Skinned: adds skeleton descriptor set
/// - Any combination of the above
///
/// # Example
///
/// ```ignore
/// use katla_gfx::{PbrMaterialConfig, PbrMaterialFlags, VertexBinding};
/// use std::path::PathBuf;
///
/// // Basic PBR material
/// let basic = PbrMaterialConfig::new(vertex_binding, PathBuf::from("shader.wgsl"));
///
/// // Bindless skinned material
/// let skinned = PbrMaterialConfig::bindless_skinned(vertex_binding, PathBuf::from("skinned.wgsl"));
///
/// // Custom configuration
/// let custom = PbrMaterialConfig::new(vertex_binding, PathBuf::from("custom.wgsl"))
///     .with_bindless()
///     .with_alpha_blending();
/// ```
#[derive(Clone, Debug)]
pub struct PbrMaterialConfig {
    pub vertex_binding: VertexBinding,
    pub shader_path: PathBuf,
    pub render_state: RenderState,
    pub flags: PbrMaterialFlags,
}

impl PbrMaterialConfig {
    /// Create a new PBR material config with default settings.
    ///
    /// Default settings:
    /// - Render state: depth test/write enabled, backface culling, no alpha blending
    /// - Flags: none (basic PBR with single texture)
    pub fn new(vertex_binding: VertexBinding, shader_path: PathBuf) -> Self {
        Self {
            vertex_binding,
            shader_path,
            render_state: RenderState {
                depth_test: true,
                depth_write: true,
                cull_backfaces: true,
                alpha_blending: false,
            },
            flags: PbrMaterialFlags::empty(),
        }
    }

    // === Builder methods ===

    /// Enable bindless textures.
    ///
    /// When enabled, Set 1 (textures) is provided externally by the bindless texture manager.
    pub fn with_bindless(mut self) -> Self {
        self.flags |= PbrMaterialFlags::USES_BINDLESS;
        self
    }

    /// Enable skeletal animation.
    ///
    /// Adds a skeleton descriptor set (Set 2) for joint matrices.
    pub fn with_skeleton(mut self) -> Self {
        self.flags |= PbrMaterialFlags::USES_SKELETON;
        self
    }

    /// Enable full PBR textures (5 textures instead of 1).
    ///
    /// Uses albedo, normal, metallic/roughness, occlusion, and emission textures.
    pub fn with_pbr_textures(mut self) -> Self {
        self.flags |= PbrMaterialFlags::USES_PBR_TEXTURES;
        self
    }

    /// Enable alpha blending.
    ///
    /// Also sets alpha_blending in the render state.
    pub fn with_alpha_blending(mut self) -> Self {
        self.flags |= PbrMaterialFlags::ALPHA_BLENDING;
        self.render_state.alpha_blending = true;
        self
    }

    /// Set a custom render state.
    pub fn with_render_state(mut self, render_state: RenderState) -> Self {
        // Sync alpha blending flag with render state
        if render_state.alpha_blending {
            self.flags |= PbrMaterialFlags::ALPHA_BLENDING;
        }
        self.render_state = render_state;
        self
    }

    // === Convenience constructors ===

    /// Create a bindless PBR material.
    ///
    /// Uses bindless textures with full PBR texture set.
    pub fn bindless(vertex_binding: VertexBinding, shader_path: PathBuf) -> Self {
        Self::new(vertex_binding, shader_path)
            .with_bindless()
            .with_pbr_textures()
    }

    /// Create a skinned PBR material.
    ///
    /// Uses skeletal animation with full PBR textures.
    pub fn skinned(vertex_binding: VertexBinding, shader_path: PathBuf) -> Self {
        Self::new(vertex_binding, shader_path)
            .with_skeleton()
            .with_pbr_textures()
    }

    /// Create a bindless skinned PBR material.
    ///
    /// Uses both bindless textures and skeletal animation.
    pub fn bindless_skinned(vertex_binding: VertexBinding, shader_path: PathBuf) -> Self {
        Self::new(vertex_binding, shader_path)
            .with_bindless()
            .with_skeleton()
            .with_pbr_textures()
    }

    /// Create a full PBR material with 5 textures.
    pub fn full_pbr(vertex_binding: VertexBinding, shader_path: PathBuf) -> Self {
        Self::new(vertex_binding, shader_path).with_pbr_textures()
    }

    // === Accessors ===

    /// Returns true if this material uses bindless textures.
    pub fn uses_bindless(&self) -> bool {
        self.flags.contains(PbrMaterialFlags::USES_BINDLESS)
    }

    /// Returns true if this material uses skeletal animation.
    pub fn uses_skeleton(&self) -> bool {
        self.flags.contains(PbrMaterialFlags::USES_SKELETON)
    }

    /// Returns true if this material uses full PBR textures (5 textures).
    pub fn uses_pbr_textures(&self) -> bool {
        self.flags.contains(PbrMaterialFlags::USES_PBR_TEXTURES)
    }

    /// Returns true if this material uses alpha blending.
    pub fn has_alpha_blending(&self) -> bool {
        self.flags.contains(PbrMaterialFlags::ALPHA_BLENDING)
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
        self.render_state.clone()
    }

    fn descriptor_layouts(&self) -> Vec<DescriptorSetLayoutBuilder> {
        // Set 0: Storage buffers (always present)
        let mut layouts = vec![DescriptorSetLayoutBuilder::new()
            .add_binding(
                0,
                DescriptorType::StorageBuffer,
                ShaderStages::VERTEX_FRAGMENT,
            )
            .add_binding(
                1,
                DescriptorType::StorageBuffer,
                ShaderStages::VERTEX_FRAGMENT,
            )];

        if self.uses_bindless() {
            // Set 1 is bindless (provided externally)
            // Set 2: Skeleton if needed
            if self.uses_skeleton() {
                layouts.push(DescriptorSetLayoutBuilder::new().add_binding(
                    0,
                    DescriptorType::StorageBuffer,
                    ShaderStages::VERTEX_FRAGMENT,
                ));
            }
        } else {
            // Non-bindless: Set 1 contains textures
            if self.uses_pbr_textures() {
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
            if self.uses_skeleton() {
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
        MaterialDomain::Surface
    }

    fn uses_bindless(&self) -> bool {
        self.flags.contains(PbrMaterialFlags::USES_BINDLESS)
    }

    fn uses_skeleton(&self) -> bool {
        self.flags.contains(PbrMaterialFlags::USES_SKELETON)
    }

    fn uses_pbr_textures(&self) -> bool {
        self.flags.contains(PbrMaterialFlags::USES_PBR_TEXTURES)
    }

    fn is_transparent(&self) -> bool {
        self.has_alpha_blending()
    }
}

/// Skinned PBR material config for skeletal animation.
///
/// Uses three descriptor sets:
/// - Set 0: frame_data + objects storage buffers
/// - Set 1: albedo texture + sampler
/// - Set 2: skeleton joint matrices
#[derive(Clone, Debug)]
pub(crate) struct SkinnedPbrMaterialConfig {
    pub vertex_binding: VertexBinding,
    pub shader_path: PathBuf,
}

impl SkinnedPbrMaterialConfig {
    /// Create a new skinned PBR material config.
    pub(crate) fn new(vertex_binding: VertexBinding, shader_path: PathBuf) -> Self {
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
pub(crate) struct FullPbrMaterialConfig {
    pub vertex_binding: VertexBinding,
    pub shader_path: PathBuf,
}

impl FullPbrMaterialConfig {
    /// Create a new full PBR material config.
    pub(crate) fn new(vertex_binding: VertexBinding, shader_path: PathBuf) -> Self {
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
pub(crate) struct BindlessPbrMaterialConfig {
    pub vertex_binding: VertexBinding,
    pub shader_path: PathBuf,
}

impl BindlessPbrMaterialConfig {
    /// Create a new bindless PBR material config.
    pub(crate) fn new(vertex_binding: VertexBinding, shader_path: PathBuf) -> Self {
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
pub(crate) struct BindlessSkinnedPbrMaterialConfig {
    pub vertex_binding: VertexBinding,
    pub shader_path: PathBuf,
}

impl BindlessSkinnedPbrMaterialConfig {
    /// Create a new bindless skinned PBR material config.
    pub(crate) fn new(vertex_binding: VertexBinding, shader_path: PathBuf) -> Self {
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
pub(crate) struct DynamicMaterialConfig {
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
    pub(crate) fn new(
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
    pub(crate) fn pbr(
        descriptor: &crate::vulkan::material::MaterialDescriptor,
        vertex_binding: VertexBinding,
    ) -> Self {
        Self::new(descriptor, vertex_binding, false, false, false)
    }

    /// Create a skinned PBR config.
    pub(crate) fn skinned(
        descriptor: &crate::vulkan::material::MaterialDescriptor,
        vertex_binding: VertexBinding,
    ) -> Self {
        Self::new(descriptor, vertex_binding, false, true, false)
    }

    /// Create a full PBR config (5 textures).
    pub(crate) fn full_pbr(
        descriptor: &crate::vulkan::material::MaterialDescriptor,
        vertex_binding: VertexBinding,
    ) -> Self {
        Self::new(descriptor, vertex_binding, true, false, false)
    }

    /// Create a bindless config.
    pub(crate) fn bindless(
        descriptor: &crate::vulkan::material::MaterialDescriptor,
        vertex_binding: VertexBinding,
    ) -> Self {
        Self::new(descriptor, vertex_binding, false, false, true)
    }

    /// Create a bindless skinned config.
    pub(crate) fn bindless_skinned(
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
        let mut layouts = vec![DescriptorSetLayoutBuilder::new()
            .add_binding(
                0,
                DescriptorType::StorageBuffer,
                ShaderStages::VERTEX_FRAGMENT,
            )
            .add_binding(
                1,
                DescriptorType::StorageBuffer,
                ShaderStages::VERTEX_FRAGMENT,
            )];

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

//=============================================================================
// Tests
//=============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vulkan::vertexbinding::VertexFormat;

    fn create_test_vertex_binding() -> VertexBinding {
        VertexBinding {
            formats: vec![VertexFormat::RGB32f, VertexFormat::RG32f],
        }
    }

    mod pbr_material_flags {
        use super::*;

        #[test]
        fn test_empty_flags() {
            let flags = PbrMaterialFlags::empty();
            assert!(!flags.contains(PbrMaterialFlags::USES_BINDLESS));
            assert!(!flags.contains(PbrMaterialFlags::USES_SKELETON));
            assert!(!flags.contains(PbrMaterialFlags::USES_PBR_TEXTURES));
            assert!(!flags.contains(PbrMaterialFlags::ALPHA_BLENDING));
        }

        #[test]
        fn test_flag_combination() {
            let flags = PbrMaterialFlags::USES_BINDLESS | PbrMaterialFlags::USES_SKELETON;
            assert!(flags.contains(PbrMaterialFlags::USES_BINDLESS));
            assert!(flags.contains(PbrMaterialFlags::USES_SKELETON));
            assert!(!flags.contains(PbrMaterialFlags::USES_PBR_TEXTURES));
        }

        #[test]
        fn test_all_flags() {
            let flags = PbrMaterialFlags::all();
            assert!(flags.contains(PbrMaterialFlags::USES_BINDLESS));
            assert!(flags.contains(PbrMaterialFlags::USES_SKELETON));
            assert!(flags.contains(PbrMaterialFlags::USES_PBR_TEXTURES));
            assert!(flags.contains(PbrMaterialFlags::ALPHA_BLENDING));
        }

        #[test]
        fn test_flag_bit_values() {
            assert_eq!(PbrMaterialFlags::USES_BINDLESS.bits(), 1 << 0);
            assert_eq!(PbrMaterialFlags::USES_SKELETON.bits(), 1 << 1);
            assert_eq!(PbrMaterialFlags::USES_PBR_TEXTURES.bits(), 1 << 2);
            assert_eq!(PbrMaterialFlags::ALPHA_BLENDING.bits(), 1 << 3);
        }
    }

    mod pbr_material_config_builders {
        use super::*;

        #[test]
        fn test_new_creates_empty_flags() {
            let config =
                PbrMaterialConfig::new(create_test_vertex_binding(), PathBuf::from("test.wgsl"));

            assert!(!config.uses_bindless());
            assert!(!config.uses_skeleton());
            assert!(!config.uses_pbr_textures());
            assert!(!config.has_alpha_blending());
        }

        #[test]
        fn test_with_bindless() {
            let config =
                PbrMaterialConfig::new(create_test_vertex_binding(), PathBuf::from("test.wgsl"))
                    .with_bindless();

            assert!(config.uses_bindless());
            assert!(!config.uses_skeleton());
        }

        #[test]
        fn test_with_skeleton() {
            let config =
                PbrMaterialConfig::new(create_test_vertex_binding(), PathBuf::from("test.wgsl"))
                    .with_skeleton();

            assert!(!config.uses_bindless());
            assert!(config.uses_skeleton());
        }

        #[test]
        fn test_with_pbr_textures() {
            let config =
                PbrMaterialConfig::new(create_test_vertex_binding(), PathBuf::from("test.wgsl"))
                    .with_pbr_textures();

            assert!(config.uses_pbr_textures());
        }

        #[test]
        fn test_with_alpha_blending() {
            let config =
                PbrMaterialConfig::new(create_test_vertex_binding(), PathBuf::from("test.wgsl"))
                    .with_alpha_blending();

            assert!(config.has_alpha_blending());
            assert!(config.render_state.alpha_blending);
        }

        #[test]
        fn test_chained_builders() {
            let config =
                PbrMaterialConfig::new(create_test_vertex_binding(), PathBuf::from("test.wgsl"))
                    .with_bindless()
                    .with_skeleton()
                    .with_pbr_textures()
                    .with_alpha_blending();

            assert!(config.uses_bindless());
            assert!(config.uses_skeleton());
            assert!(config.uses_pbr_textures());
            assert!(config.has_alpha_blending());
        }

        #[test]
        fn test_with_render_state_syncs_alpha() {
            let render_state = RenderState {
                depth_test: true,
                depth_write: true,
                cull_backfaces: false,
                alpha_blending: true,
            };

            let config =
                PbrMaterialConfig::new(create_test_vertex_binding(), PathBuf::from("test.wgsl"))
                    .with_render_state(render_state);

            assert!(config.has_alpha_blending());
        }
    }

    mod pbr_material_config_convenience_constructors {
        use super::*;

        #[test]
        fn test_bindless_constructor() {
            let config = PbrMaterialConfig::bindless(
                create_test_vertex_binding(),
                PathBuf::from("test.wgsl"),
            );

            assert!(config.uses_bindless());
            assert!(config.uses_pbr_textures());
            assert!(!config.uses_skeleton());
            assert!(!config.has_alpha_blending());
        }

        #[test]
        fn test_skinned_constructor() {
            let config = PbrMaterialConfig::skinned(
                create_test_vertex_binding(),
                PathBuf::from("test.wgsl"),
            );

            assert!(config.uses_skeleton());
            assert!(config.uses_pbr_textures());
            assert!(!config.uses_bindless());
            assert!(!config.has_alpha_blending());
        }

        #[test]
        fn test_bindless_skinned_constructor() {
            let config = PbrMaterialConfig::bindless_skinned(
                create_test_vertex_binding(),
                PathBuf::from("test.wgsl"),
            );

            assert!(config.uses_bindless());
            assert!(config.uses_skeleton());
            assert!(config.uses_pbr_textures());
            assert!(!config.has_alpha_blending());
        }

        #[test]
        fn test_full_pbr_constructor() {
            let config = PbrMaterialConfig::full_pbr(
                create_test_vertex_binding(),
                PathBuf::from("test.wgsl"),
            );

            assert!(config.uses_pbr_textures());
            assert!(!config.uses_bindless());
            assert!(!config.uses_skeleton());
            assert!(!config.has_alpha_blending());
        }
    }

    mod pbr_material_config_trait_impl {
        use super::*;

        #[test]
        fn test_vertex_shader() {
            let config =
                PbrMaterialConfig::new(create_test_vertex_binding(), PathBuf::from("shader.wgsl"));

            match config.vertex_shader() {
                ShaderSource::WgslFile(path) => assert_eq!(path, PathBuf::from("shader.wgsl")),
                _ => panic!("Expected WgslFile shader source"),
            }
        }

        #[test]
        fn test_fragment_shader() {
            let config =
                PbrMaterialConfig::new(create_test_vertex_binding(), PathBuf::from("shader.wgsl"));

            match config.fragment_shader() {
                ShaderSource::WgslFile(path) => assert_eq!(path, PathBuf::from("shader.wgsl")),
                _ => panic!("Expected WgslFile shader source"),
            }
        }

        #[test]
        fn test_render_state_default() {
            let config =
                PbrMaterialConfig::new(create_test_vertex_binding(), PathBuf::from("test.wgsl"));

            let state = config.render_state();
            assert!(state.depth_test);
            assert!(state.depth_write);
            assert!(state.cull_backfaces);
            assert!(!state.alpha_blending);
        }

        #[test]
        fn test_domain() {
            let config =
                PbrMaterialConfig::new(create_test_vertex_binding(), PathBuf::from("test.wgsl"));

            assert_eq!(config.domain(), MaterialDomain::Surface);
        }

        #[test]
        fn test_is_transparent() {
            let opaque =
                PbrMaterialConfig::new(create_test_vertex_binding(), PathBuf::from("test.wgsl"));
            let transparent =
                PbrMaterialConfig::new(create_test_vertex_binding(), PathBuf::from("test.wgsl"))
                    .with_alpha_blending();

            assert!(!opaque.is_transparent());
            assert!(transparent.is_transparent());
        }

        #[test]
        fn test_descriptor_layouts_basic() {
            let config =
                PbrMaterialConfig::new(create_test_vertex_binding(), PathBuf::from("test.wgsl"));

            let layouts = config.descriptor_layouts();
            assert_eq!(layouts.len(), 2); // Set 0 + Set 1 (single texture)
        }

        #[test]
        fn test_descriptor_layouts_bindless() {
            let config =
                PbrMaterialConfig::new(create_test_vertex_binding(), PathBuf::from("test.wgsl"))
                    .with_bindless();

            let layouts = config.descriptor_layouts();
            assert_eq!(layouts.len(), 1); // Only Set 0 (Set 1 is bindless, external)
        }

        #[test]
        fn test_descriptor_layouts_bindless_skinned() {
            let config = PbrMaterialConfig::bindless_skinned(
                create_test_vertex_binding(),
                PathBuf::from("test.wgsl"),
            );

            let layouts = config.descriptor_layouts();
            assert_eq!(layouts.len(), 2); // Set 0 + Set 2 (skeleton)
        }

        #[test]
        fn test_descriptor_layouts_full_pbr() {
            let config = PbrMaterialConfig::full_pbr(
                create_test_vertex_binding(),
                PathBuf::from("test.wgsl"),
            );

            let layouts = config.descriptor_layouts();
            assert_eq!(layouts.len(), 2); // Set 0 + Set 1 (5 textures)
        }

        #[test]
        fn test_descriptor_layouts_skinned() {
            let config = PbrMaterialConfig::skinned(
                create_test_vertex_binding(),
                PathBuf::from("test.wgsl"),
            );

            let layouts = config.descriptor_layouts();
            assert_eq!(layouts.len(), 3); // Set 0 + Set 1 (textures) + Set 2 (skeleton)
        }
    }
}
