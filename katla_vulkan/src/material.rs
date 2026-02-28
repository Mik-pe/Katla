//! Unified material system.
//!
//! Provides a trait-based abstraction for materials, reusing existing types
//! from the vulkan module. This trait describes a material's pipeline requirements
//! without dictating implementation details.
//!
//! # Design
//!
//! The [`MaterialDefinition`] trait reuses existing types:
//! - [`ShaderSource`] for shader code
//! - [`RenderState`] for depth/blending/culling
//! - [`VertexBinding`] for vertex format
//! - [`DescriptorSetLayoutBuilder`] for descriptor layouts
//!
//! Pipeline creation and caching is handled separately in future phases.
//!
//! # Legacy Types
//!
//! This module also re-exports legacy types from `vulkan::material` for backward compatibility:
//! - [`MaterialPipeline`] - Legacy pipeline handle
//! - [`MaterialBuilder`] - Legacy material builder
//! - [`PbrTextureSet`] - PBR texture collection
//! - And more...

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::vulkan::descriptor::{DescriptorBinding, DescriptorSetLayoutBuilder};
use crate::vulkan::pipeline_state::DescriptorType;
use crate::vulkan::vertexbinding::VertexBinding;
use crate::{ImageFormat, ShaderStages};

// Re-export Material trait types for public API
pub use crate::vulkan::material::descriptor::{RenderState, ShaderSource};

//=============================================================================
// Legacy Re-exports (for backward compatibility)
//=============================================================================

// Re-export legacy types from vulkan::material for backward compatibility
pub use crate::vulkan::material::{
    load_material_from_file,
    // Asset loading
    AssetError,
    // Buffer descriptors
    BufferBinding,
    BufferDescriptorSource,
    // Compute pipeline
    ComputePipeline,
    ComputePipelineBuilder,
    ComputePipelineError,
    // Descriptor types
    DescriptorLayoutBuilder,
    // File watching
    FileWatcher,
    // Storage uniforms
    FrameUniforms,
    ImageInfo,
    // Template
    InstanceError,
    MaterialBuildError,
    MaterialBuilder,
    MaterialDescriptor,
    MaterialError,
    MaterialInstance,
    // Parameters
    MaterialParameters,
    // Core legacy types
    MaterialPipeline,
    // Hot reload and registry
    MaterialRegistry,
    MaterialTemplate,
    MaterialValue,
    MemberType,
    ObjectUniforms,
    ParameterError,
    PbrTextureSet,
    Pipeline,
    // Pipeline builder (legacy)
    PipelineBuilder,
    PipelineError,
    ReflectionError,
    ShaderCache,
    ShaderError,
    // Shader module
    ShaderModule,
    // Reflection
    ShaderReflection,
    ShaderStage,
    // Skeleton
    SkeletonDescriptorSet,
    StorageDescriptorSet,
    StorageUniformLayout,
    StorageUniformManager,
    StructLayout,
    StructMember,
    UniformBuffer,
    UniformHandle,
    UniformLayout,
    UniformType,
    WatcherError,
};

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
/// # Implementation Example
///
/// ```ignore
/// struct SkyMaterial;
///
/// impl MaterialDefinition for SkyMaterial {
///     fn vertex_shader(&self) -> ShaderSource {
///         ShaderSource::WgslFile("shaders/sky.wgsl".into())
///     }
///
///     fn fragment_shader(&self) -> ShaderSource {
///         ShaderSource::WgslFile("shaders/sky.wgsl".into())
///     }
///
///     fn vertex_binding(&self) -> VertexBinding {
///         VertexBinding { formats: vec![] } // Fullscreen quad
///     }
///
///     fn render_state(&self) -> RenderState {
///         RenderState {
///             depth_test: true,
///             depth_write: false,
///             cull_backfaces: false,
///             alpha_blending: false,
///         }
///     }
///
///     fn descriptor_layouts(&self) -> Vec<DescriptorSetLayoutBuilder> {
///         vec![
///             DescriptorSetLayoutBuilder::new()
///                 .add_binding(0, DescriptorType::StorageBuffer, ShaderStages::VERTEX_FRAGMENT)
///                 .add_binding(1, DescriptorType::StorageBuffer, ShaderStages::VERTEX_FRAGMENT)
///         ]
///     }
///
///     fn domain(&self) -> MaterialDomain {
///         MaterialDomain::PostProcess
///     }
/// }
/// ```
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

//=============================================================================
// Built-in Material Configs
//=============================================================================

use std::path::PathBuf;

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
        descriptor: &super::vulkan::material::MaterialDescriptor,
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
        descriptor: &super::vulkan::material::MaterialDescriptor,
        vertex_binding: VertexBinding,
    ) -> Self {
        Self::new(descriptor, vertex_binding, false, false, false)
    }

    /// Create a skinned PBR config.
    pub fn skinned(
        descriptor: &super::vulkan::material::MaterialDescriptor,
        vertex_binding: VertexBinding,
    ) -> Self {
        Self::new(descriptor, vertex_binding, false, true, false)
    }

    /// Create a full PBR config (5 textures).
    pub fn full_pbr(
        descriptor: &super::vulkan::material::MaterialDescriptor,
        vertex_binding: VertexBinding,
    ) -> Self {
        Self::new(descriptor, vertex_binding, true, false, false)
    }

    /// Create a bindless config.
    pub fn bindless(
        descriptor: &super::vulkan::material::MaterialDescriptor,
        vertex_binding: VertexBinding,
    ) -> Self {
        Self::new(descriptor, vertex_binding, false, false, true)
    }

    /// Create a bindless skinned config.
    pub fn bindless_skinned(
        descriptor: &super::vulkan::material::MaterialDescriptor,
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
fn hash_shader(shader: &ShaderSource) -> u64 {
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
fn hash_vertex_binding(binding: &VertexBinding) -> u64 {
    let mut hasher = DefaultHasher::new();
    binding.formats.len().hash(&mut hasher);
    for format in &binding.formats {
        format.hash(&mut hasher);
    }
    hasher.finish()
}

/// Hash a render state configuration.
fn hash_render_state(state: &RenderState) -> u64 {
    let mut hasher = DefaultHasher::new();
    state.depth_test.hash(&mut hasher);
    state.depth_write.hash(&mut hasher);
    state.cull_backfaces.hash(&mut hasher);
    state.alpha_blending.hash(&mut hasher);
    hasher.finish()
}

/// Hash descriptor layouts.
fn hash_layouts(layouts: &[DescriptorSetLayoutBuilder]) -> u64 {
    let mut hasher = DefaultHasher::new();
    layouts.len().hash(&mut hasher);
    for layout in layouts {
        hash_layout(layout, &mut hasher);
    }
    hasher.finish()
}

/// Hash a single descriptor set layout.
fn hash_layout(layout: &DescriptorSetLayoutBuilder, hasher: &mut DefaultHasher) {
    let bindings = layout.bindings();
    bindings.len().hash(hasher);
    for binding in bindings {
        hash_descriptor_binding(binding, hasher);
    }
    // Hash push_descriptor flag
    layout.is_push_descriptor().hash(hasher);
}

/// Hash a single descriptor binding.
fn hash_descriptor_binding(binding: &DescriptorBinding, hasher: &mut DefaultHasher) {
    binding.binding.hash(hasher);
    hash_descriptor_type(&binding.descriptor_type, hasher);
    binding.descriptor_count.hash(hasher);
    hash_shader_stages(&binding.shader_stages, hasher);
}

/// Hash a descriptor type.
fn hash_descriptor_type(ty: &DescriptorType, hasher: &mut DefaultHasher) {
    // Use discriminant for stable hashing across versions
    let discriminant = match ty {
        DescriptorType::UniformBuffer => 0u8,
        DescriptorType::StorageBuffer => 1,
        DescriptorType::SampledImage => 2,
        DescriptorType::Sampler => 3,
        DescriptorType::CombinedImageSampler => 4,
        DescriptorType::UniformTexelBuffer => 5,
        DescriptorType::StorageTexelBuffer => 6,
        DescriptorType::InputAttachment => 7,
        DescriptorType::StorageImage => 8,
    };
    discriminant.hash(hasher);
}

/// Hash shader stages.
fn hash_shader_stages(stages: &ShaderStages, hasher: &mut DefaultHasher) {
    stages.vertex.hash(hasher);
    stages.fragment.hash(hasher);
    stages.compute.hash(hasher);
    stages.geometry.hash(hasher);
    stages.tessellation_control.hash(hasher);
    stages.tessellation_evaluation.hash(hasher);
}

//=============================================================================
// Material Pipeline Cache
//=============================================================================

use std::collections::HashMap;
use std::rc::Rc;

use crate::handle::{PipelineHandle, ResourceStorage};
use crate::sync::VkRenderPass;
use crate::vulkan::context::VulkanContext;
use crate::vulkan::pipeline_state::{CullMode, FrontFace};

/// Error type for material pipeline cache operations.
#[derive(Debug)]
pub enum MaterialCacheError {
    /// Failed to create pipeline
    PipelineCreationFailed(String),
    /// Shader compilation failed
    ShaderCompilationFailed(String),
    /// Invalid material configuration
    InvalidConfiguration(String),
}

impl std::fmt::Display for MaterialCacheError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MaterialCacheError::PipelineCreationFailed(msg) => {
                write!(f, "Pipeline creation failed: {}", msg)
            }
            MaterialCacheError::ShaderCompilationFailed(msg) => {
                write!(f, "Shader compilation failed: {}", msg)
            }
            MaterialCacheError::InvalidConfiguration(msg) => {
                write!(f, "Invalid material configuration: {}", msg)
            }
        }
    }
}

impl std::error::Error for MaterialCacheError {}

/// Cache for material pipelines keyed by MaterialKey.
///
/// This cache deduplicates pipeline creation by reusing existing pipelines
/// when materials have compatible configurations. Pipelines are stored in
/// a central `ResourceStorage` and referenced by opaque `PipelineHandle`.
///
/// # Example
///
/// ```ignore
/// use katla_vulkan::{MaterialPipelineCache, Material, MaterialKey};
///
/// // Create cache
/// let mut cache = MaterialPipelineCache::new(context.clone());
///
/// // Get or create pipeline for a material
/// let handle = cache.get_or_create(&my_material)?;
///
/// // Subsequent calls with same material return same handle
/// let handle2 = cache.get_or_create(&my_material)?;
/// assert_eq!(handle, handle2);
///
/// // Access the pipeline via handle
/// let pipeline = cache.get_pipeline(handle);
/// ```
pub struct MaterialPipelineCache {
    context: Rc<VulkanContext>,
    cache: HashMap<MaterialKey, PipelineHandle>,
    storage: ResourceStorage<MaterialPipeline>,
}

impl MaterialPipelineCache {
    /// Create a new empty pipeline cache (internal).
    pub(crate) fn new(context: Rc<VulkanContext>) -> Self {
        Self {
            context,
            cache: HashMap::new(),
            storage: ResourceStorage::new(),
        }
    }

    /// Get or create a pipeline for the given material.
    ///
    /// If a compatible pipeline already exists in the cache, returns its handle.
    /// Otherwise, creates a new pipeline, caches it, and returns the new handle.
    ///
    /// # Arguments
    /// * `material` - MaterialDefinition implementation to create pipeline for
    ///
    /// # Returns
    /// * `Ok(PipelineHandle)` - Handle to the cached or newly created pipeline
    /// * `Err(MaterialCacheError)` - If pipeline creation fails
    pub fn get_or_create<M: MaterialDefinition + ?Sized>(
        &mut self,
        material: &M,
    ) -> Result<PipelineHandle, MaterialCacheError> {
        let key = MaterialKey::from_material(material);

        if let Some(&handle) = self.cache.get(&key) {
            return Ok(handle);
        }

        let pipeline = self.create_pipeline_for_material(material)?;
        let handle = PipelineHandle::new(self.storage.insert(pipeline));

        self.cache.insert(key, handle);
        Ok(handle)
    }

    /// Get or create a pipeline for bindless materials.
    ///
    /// Bindless materials require the bindless texture layout from
    /// BindlessTextureManager. This method creates pipelines that use
    /// the bindless texture array instead of individual texture descriptors.
    ///
    /// # Arguments
    /// * `material` - MaterialDefinition implementation to create pipeline for
    /// * `bindless_layout` - Descriptor set layout from BindlessTextureManager
    pub(crate) fn get_or_create_bindless<M: MaterialDefinition + ?Sized>(
        &mut self,
        material: &M,
        bindless_layout: crate::sync::VkDescriptorSetLayout,
    ) -> Result<PipelineHandle, MaterialCacheError> {
        let key = MaterialKey::from_material(material);

        if let Some(&handle) = self.cache.get(&key) {
            return Ok(handle);
        }

        let pipeline = self.create_bindless_pipeline(material, bindless_layout)?;
        let handle = PipelineHandle::new(self.storage.insert(pipeline));

        self.cache.insert(key, handle);
        Ok(handle)
    }

    /// Get a pipeline by handle.
    pub fn get_pipeline(&self, handle: PipelineHandle) -> Option<&MaterialPipeline> {
        if handle.is_none() {
            return None;
        }
        self.storage.get(handle.index())
    }

    /// Get a mutable pipeline by handle.
    pub fn get_pipeline_mut(&mut self, handle: PipelineHandle) -> Option<&mut MaterialPipeline> {
        if handle.is_none() {
            return None;
        }
        self.storage.get_mut(handle.index())
    }

    /// Create a bindless pipeline for a material.
    fn create_bindless_pipeline<M: MaterialDefinition + ?Sized>(
        &self,
        material: &M,
        bindless_layout: crate::sync::VkDescriptorSetLayout,
    ) -> Result<MaterialPipeline, MaterialCacheError> {
        let render_state = material.render_state();
        let vertex_binding = material.vertex_binding();

        if !material.uses_bindless() {
            return Err(MaterialCacheError::InvalidConfiguration(
                "get_or_create_bindless() requires uses_bindless() to return true".to_string(),
            ));
        }

        let vert_shader =
            self.load_shader(&material.vertex_shader(), ash::vk::ShaderStageFlags::VERTEX)?;
        let frag_shader = self.load_shader(
            &material.fragment_shader(),
            ash::vk::ShaderStageFlags::FRAGMENT,
        )?;

        let layout_builders = material.descriptor_layouts();
        let mut vk_layouts: Vec<ash::vk::DescriptorSetLayout> = Vec::new();
        let mut wrapped_layouts: Vec<crate::sync::VkDescriptorSetLayout> = Vec::new();

        if let Some(builder) = layout_builders.first() {
            let wrapped = builder.clone().build(&self.context).map_err(|e| {
                MaterialCacheError::PipelineCreationFailed(format!(
                    "Descriptor layout failed: {:?}",
                    e
                ))
            })?;
            vk_layouts.push(wrapped.vk());
            wrapped_layouts.push(wrapped);
        }

        vk_layouts.push(bindless_layout.vk());

        if material.uses_skeleton() {
            if let Some(builder) = layout_builders.get(1) {
                let wrapped = builder.clone().build(&self.context).map_err(|e| {
                    MaterialCacheError::PipelineCreationFailed(format!(
                        "Skeleton layout failed: {:?}",
                        e
                    ))
                })?;
                vk_layouts.push(wrapped.vk());
                wrapped_layouts.push(wrapped);
            }
        }

        let mut pipeline_builder = PipelineBuilder::new(self.context.clone())
            .with_shaders(vert_shader.module, frag_shader.module)
            .with_entry_points(
                vert_shader.entry_point.clone(),
                frag_shader.entry_point.clone(),
            )
            .with_vertex_input(
                vec![vertex_binding.get_binding_desc(0)],
                vertex_binding.get_attribute_desc(0),
            )
            .with_depth_test(
                render_state.depth_test,
                render_state.depth_write,
                crate::vulkan::pipeline_state::CompareOp::Greater,
            )
            .with_descriptor_layouts(vk_layouts.clone())
            .with_rendering_formats(Some(material.color_format()), Some(material.depth_format()));

        if render_state.cull_backfaces {
            pipeline_builder =
                pipeline_builder.with_cull_mode(CullMode::Back, FrontFace::CounterClockwise);
        } else {
            pipeline_builder =
                pipeline_builder.with_cull_mode(CullMode::None, FrontFace::CounterClockwise);
        }

        if render_state.alpha_blending {
            pipeline_builder = pipeline_builder.with_alpha_blending();
        }

        let pipeline = pipeline_builder
            .build(VkRenderPass::from(ash::vk::RenderPass::null()))
            .map_err(|e| MaterialCacheError::PipelineCreationFailed(format!("{:?}", e)))?;

        let uniform_set_layout = vk_layouts
            .first()
            .copied()
            .unwrap_or(ash::vk::DescriptorSetLayout::null());
        let skeleton_set_layout = if material.uses_skeleton() {
            vk_layouts.get(2).copied()
        } else {
            None
        };

        let material_pipeline = if material.uses_skeleton() {
            MaterialPipeline::new_bindless_skinned(
                pipeline,
                uniform_set_layout,
                skeleton_set_layout.unwrap_or(ash::vk::DescriptorSetLayout::null()),
                self.context.clone(),
            )
        } else {
            MaterialPipeline::new_bindless(pipeline, uniform_set_layout, self.context.clone())
        };

        Ok(material_pipeline)
    }

    /// Create a pipeline for a material using the MaterialDefinition trait directly.
    fn create_pipeline_for_material<M: MaterialDefinition + ?Sized>(
        &self,
        material: &M,
    ) -> Result<MaterialPipeline, MaterialCacheError> {
        let render_state = material.render_state();
        let vertex_binding = material.vertex_binding();

        if material.uses_bindless() {
            return Err(MaterialCacheError::InvalidConfiguration(
                "Bindless materials require bindless_layout. Use get_or_create_bindless() instead."
                    .to_string(),
            ));
        }

        let vert_shader =
            self.load_shader(&material.vertex_shader(), ash::vk::ShaderStageFlags::VERTEX)?;
        let frag_shader = self.load_shader(
            &material.fragment_shader(),
            ash::vk::ShaderStageFlags::FRAGMENT,
        )?;

        let layout_builders = material.descriptor_layouts();
        let mut vk_layouts: Vec<ash::vk::DescriptorSetLayout> =
            Vec::with_capacity(layout_builders.len());
        let mut wrapped_layouts: Vec<crate::sync::VkDescriptorSetLayout> =
            Vec::with_capacity(layout_builders.len());

        for builder in &layout_builders {
            let wrapped = builder.clone().build(&self.context).map_err(|e| {
                MaterialCacheError::PipelineCreationFailed(format!(
                    "Descriptor layout failed: {:?}",
                    e
                ))
            })?;
            vk_layouts.push(wrapped.vk());
            wrapped_layouts.push(wrapped);
        }

        let mut pipeline_builder = PipelineBuilder::new(self.context.clone())
            .with_shaders(vert_shader.module, frag_shader.module)
            .with_entry_points(
                vert_shader.entry_point.clone(),
                frag_shader.entry_point.clone(),
            )
            .with_vertex_input(
                vec![vertex_binding.get_binding_desc(0)],
                vertex_binding.get_attribute_desc(0),
            )
            .with_depth_test(
                render_state.depth_test,
                render_state.depth_write,
                crate::vulkan::pipeline_state::CompareOp::Greater,
            )
            .with_descriptor_layouts(vk_layouts.clone())
            .with_rendering_formats(Some(material.color_format()), Some(material.depth_format()));

        if render_state.cull_backfaces {
            pipeline_builder =
                pipeline_builder.with_cull_mode(CullMode::Back, FrontFace::CounterClockwise);
        } else {
            pipeline_builder =
                pipeline_builder.with_cull_mode(CullMode::None, FrontFace::CounterClockwise);
        }

        if render_state.alpha_blending {
            pipeline_builder = pipeline_builder.with_alpha_blending();
        }

        let pipeline = pipeline_builder
            .build(VkRenderPass::from(ash::vk::RenderPass::null()))
            .map_err(|e| MaterialCacheError::PipelineCreationFailed(format!("{:?}", e)))?;

        let material_pipeline = self.create_material_pipeline(
            pipeline,
            wrapped_layouts,
            material.domain(),
            material.uses_skeleton(),
        );

        Ok(material_pipeline)
    }

    /// Load a shader from ShaderSource.
    fn load_shader(
        &self,
        source: &ShaderSource,
        stage: ash::vk::ShaderStageFlags,
    ) -> Result<ShaderModule, MaterialCacheError> {
        let entry_point = std::ffi::CString::new(if stage == ash::vk::ShaderStageFlags::VERTEX {
            "vs_main"
        } else {
            "fs_main"
        })
        .unwrap();

        match source {
            ShaderSource::WgslFile(path) => ShaderModule::from_wgsl(
                self.context.device.clone(),
                path,
                stage,
                entry_point.to_str().unwrap(),
            )
            .map_err(|e| MaterialCacheError::ShaderCompilationFailed(format!("{:?}", e))),
            ShaderSource::WgslString(code) => ShaderModule::from_wgsl_string(
                self.context.device.clone(),
                code,
                stage,
                entry_point.to_str().unwrap(),
            )
            .map_err(|e| MaterialCacheError::ShaderCompilationFailed(format!("{:?}", e))),
            ShaderSource::PreCompiled(bytes) => {
                ShaderModule::from_bytes(self.context.device.clone(), bytes.clone(), stage, "main")
                    .map_err(|e| MaterialCacheError::ShaderCompilationFailed(format!("{:?}", e)))
            }
        }
    }

    /// Create a MaterialPipeline from the raw pipeline and layouts.
    fn create_material_pipeline(
        &self,
        pipeline: Pipeline,
        layouts: Vec<crate::sync::VkDescriptorSetLayout>,
        domain: MaterialDomain,
        uses_skeleton: bool,
    ) -> MaterialPipeline {
        // Convert layouts to vk types
        let vk_layouts: Vec<ash::vk::DescriptorSetLayout> =
            layouts.iter().map(|l| l.vk()).collect();

        // Determine layout assignment based on domain and skeleton
        let uniform_set_layout = vk_layouts
            .first()
            .copied()
            .unwrap_or(ash::vk::DescriptorSetLayout::null());
        let texture_set_layout = vk_layouts.get(1).copied();
        let skeleton_set_layout = if uses_skeleton {
            vk_layouts.get(2).copied()
        } else {
            None
        };

        // Create appropriate MaterialPipeline based on configuration
        match domain {
            MaterialDomain::Ui => {
                // UI materials use two sets: uniform (set 0) and push descriptor (set 1)
                let push_descriptor_layout = vk_layouts
                    .get(1)
                    .copied()
                    .unwrap_or(ash::vk::DescriptorSetLayout::null());
                MaterialPipeline::new_ui(
                    pipeline,
                    uniform_set_layout,
                    push_descriptor_layout,
                    self.context.clone(),
                )
            }
            _ => {
                // Standard surface/post-process/particle materials
                if uses_skeleton {
                    MaterialPipeline::new_storage_skinned(
                        pipeline,
                        uniform_set_layout,
                        texture_set_layout.unwrap_or(ash::vk::DescriptorSetLayout::null()),
                        skeleton_set_layout.unwrap_or(ash::vk::DescriptorSetLayout::null()),
                        self.context.clone(),
                    )
                } else if let Some(tex_layout) = texture_set_layout {
                    MaterialPipeline::new_storage(
                        pipeline,
                        uniform_set_layout,
                        tex_layout,
                        self.context.clone(),
                    )
                } else {
                    MaterialPipeline::new_custom(pipeline, uniform_set_layout, self.context.clone())
                }
            }
        }
    }

    /// Get the number of cached pipelines.
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Check if a pipeline exists for the given material.
    pub fn contains<M: MaterialDefinition + ?Sized>(&self, material: &M) -> bool {
        let key = MaterialKey::from_material(material);
        self.cache.contains_key(&key)
    }

    /// Clear all cached pipelines.
    pub fn clear(&mut self) {
        self.cache.clear();
        self.storage.clear();
    }

    /// Remove a specific pipeline from the cache.
    ///
    /// Returns true if the pipeline was in the cache and was removed.
    pub fn remove<M: MaterialDefinition + ?Sized>(&mut self, material: &M) -> bool {
        let key = MaterialKey::from_material(material);
        if let Some(handle) = self.cache.remove(&key) {
            self.storage.remove(handle.index());
            true
        } else {
            false
        }
    }

    /// Get statistics about the cache.
    pub fn stats(&self) -> MaterialCacheStats {
        let mut by_domain = HashMap::new();
        for key in self.cache.keys() {
            *by_domain.entry(key.domain).or_insert(0) += 1;
        }
        MaterialCacheStats {
            total_pipelines: self.cache.len(),
            by_domain,
        }
    }
}

impl Drop for MaterialPipelineCache {
    fn drop(&mut self) {
        if !self.cache.is_empty() {
            log::debug!(
                "MaterialPipelineCache dropping with {} pipelines",
                self.cache.len()
            );
        }
    }
}

/// Statistics about the material pipeline cache.
#[derive(Debug, Clone)]
pub struct MaterialCacheStats {
    /// Total number of cached pipelines
    pub total_pipelines: usize,
    /// Pipelines grouped by domain
    pub by_domain: HashMap<MaterialDomain, usize>,
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
        hash_descriptor_type(&DescriptorType::UniformBuffer, &mut hasher1);
        let hash1 = hasher1.finish();

        let mut hasher2 = DefaultHasher::new();
        hash_descriptor_type(&DescriptorType::UniformBuffer, &mut hasher2);
        let hash2 = hasher2.finish();

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, {
            let mut hasher = DefaultHasher::new();
            hash_descriptor_type(&DescriptorType::StorageBuffer, &mut hasher);
            hasher.finish()
        });
    }
}
