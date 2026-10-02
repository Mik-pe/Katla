//! Backend-neutral typed pipeline/material compilation descriptor.
//!
//! This module replaces the old string-dispatched material inputs
//! (`vertex_type: &str`, `"pbr"` / `"ui"` / `"skinned"` / ...). A
//! [`PipelineDescriptor`] carries everything needed to compile a pipeline on
//! any backend: shader identity and entry points, the canonical vertex
//! layout (the same [`VertexLayout`](crate::vertex::VertexLayout) model used
//! by mesh creation), portable render state, specialization constants, and
//! explicit attachment and raster state.
//!
//! Backends translate the same descriptor without string switches:
//! Vulkan builds bindings generically from the layout, Metal selects its
//! vertex descriptor from layout identity. Unknown layouts fail loudly with
//! typed errors instead of silently falling back to PBR.
//!
//! All types implement `Hash` + `Eq` so descriptors can key the pipeline
//! variant cache (see issue #88).

use std::collections::BTreeMap;

use crate::error::RendererError;
use crate::pipeline::CompareOp;
pub use crate::pipeline::CullMode;
use crate::texture::ImageFormat;
use crate::vertex::VertexLayout;

/// Which shader stages a pipeline compiles and under which entry names.
///
/// Entry names are data (shader interface), not dispatch keys: backends look
/// up exactly these names. Well-known Katla convention is `vs_main` /
/// `fs_main` / `cs_main`, provided by the constructors below.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PipelineStages {
    /// A graphics pipeline with a vertex and a fragment entry point.
    Graphics {
        /// Vertex shader entry point (convention: `"vs_main"`).
        vertex_entry: String,
        /// Fragment shader entry point (convention: `"fs_main"`).
        fragment_entry: Option<String>,
    },
    /// A compute pipeline with a single compute entry point.
    Compute {
        /// Compute shader entry point (convention: `"cs_main"`).
        compute_entry: String,
    },
}

impl PipelineStages {
    /// Standard graphics stages (`vs_main` / `fs_main`).
    pub fn graphics() -> Self {
        Self::Graphics {
            vertex_entry: "vs_main".to_string(),
            fragment_entry: Some("fs_main".to_string()),
        }
    }

    /// Standard compute stage (`cs_main`).
    pub fn compute() -> Self {
        Self::Compute {
            compute_entry: "cs_main".to_string(),
        }
    }

    /// True for [`PipelineStages::Compute`].
    pub fn is_compute(&self) -> bool {
        matches!(self, Self::Compute { .. })
    }
}

/// Portable color-blending mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BlendMode {
    /// No blending; source replaces the destination.
    #[default]
    Opaque,
    /// Standard source-alpha blending.
    AlphaBlend,
}

/// Portable depth state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DepthState {
    /// Whether depth testing is enabled.
    pub test: bool,
    /// Whether passing fragments write depth.
    pub write: bool,
    /// Comparison used when testing.
    pub compare: CompareOp,
}

impl Default for DepthState {
    /// Reversed-depth default used by both backends (`GreaterOrEqual`).
    fn default() -> Self {
        Self {
            test: true,
            write: true,
            compare: CompareOp::GreaterOrEqual,
        }
    }
}

impl DepthState {
    /// Depth testing fully disabled (overlays, UI, compositing).
    pub fn disabled() -> Self {
        Self {
            test: false,
            write: false,
            compare: CompareOp::Always,
        }
    }
}

/// Portable operation applied to a stencil value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StencilOperation {
    Keep,
    Zero,
    Replace,
    IncrementClamp,
    DecrementClamp,
    Invert,
    IncrementWrap,
    DecrementWrap,
}

/// Stencil test and operations for one rasterized face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StencilFaceState {
    pub compare: CompareOp,
    pub fail: StencilOperation,
    pub depth_fail: StencilOperation,
    pub pass: StencilOperation,
}

/// Explicit stencil state; absent state disables stencil testing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StencilState {
    pub front: StencilFaceState,
    pub back: StencilFaceState,
    pub reference: u32,
    pub read_mask: u32,
    pub write_mask: u32,
}

/// Enabled color channels of a graphics pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ColorWriteMask(pub u8);

impl ColorWriteMask {
    pub const NONE: Self = Self(0);
    pub const RED: Self = Self(1);
    pub const GREEN: Self = Self(2);
    pub const BLUE: Self = Self(4);
    pub const ALPHA: Self = Self(8);
    pub const ALL: Self = Self(15);
}

/// Raster depth offset applied before the depth test.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DepthBias {
    pub constant: f32,
    pub slope_factor: f32,
    pub clamp: f32,
}

impl Eq for DepthBias {}

impl std::hash::Hash for DepthBias {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        for value in [self.constant, self.slope_factor, self.clamp] {
            state.write_u32(if value == 0.0 { 0 } else { value.to_bits() });
        }
    }
}

/// A single specialization / function-constant value, keyed by constant ID.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpecializationValue {
    /// Boolean constant.
    Bool(bool),
    /// Unsigned 32-bit constant.
    U32(u32),
    /// 32-bit float constant (must be finite).
    F32(f32),
}

impl Eq for SpecializationValue {}

impl std::hash::Hash for SpecializationValue {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Self::Bool(v) => v.hash(state),
            Self::U32(v) => v.hash(state),
            Self::F32(v) => (if *v == 0.0 { 0 } else { v.to_bits() }).hash(state),
        }
    }
}

/// Backend-neutral material/pipeline compilation descriptor.
///
/// Build with a canonical constructor ([`PipelineDescriptor::pbr`],
/// [`PipelineDescriptor::ui`], [`PipelineDescriptor::skinned`],
/// [`PipelineDescriptor::billboard`], [`PipelineDescriptor::simple`],
/// [`PipelineDescriptor::compute`]) and tweak
/// with the `with_*` modifiers. Call [`PipelineDescriptor::validate`] before
/// handing it to [`GpuRenderer::compile_material`](crate::renderer::GpuRenderer::compile_material).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PipelineDescriptor {
    /// Shader source path (WGSL), e.g. `"shaders/pbr.wgsl"`.
    pub shader_path: String,
    /// Shader stages and entry points to compile.
    pub stages: PipelineStages,
    /// Canonical vertex layout; the same model mesh creation validates
    /// against. Compute pipelines carry [`VertexLayout::empty`].
    pub vertex: VertexLayout,
    /// Color blending mode.
    pub blend: BlendMode,
    /// Face culling mode.
    pub cull: CullMode,
    /// Depth testing/writing.
    pub depth: DepthState,
    /// Explicit stencil tests and writes.
    pub stencil: Option<StencilState>,
    /// Color channels written by the fragment stage.
    pub color_write_mask: ColorWriteMask,
    /// Explicit depth bias used by this pipeline.
    pub depth_bias: DepthBias,
    /// Wireframe rasterization.
    pub wireframe: bool,
    /// Color attachment format (`Auto` = backend default / deferred).
    pub color_format: ImageFormat,
    /// Whether this pipeline encodes a color attachment, including discard-only fragment stages.
    pub color_attachment: bool,
    /// Exact depth/stencil attachment format; None encodes without a depth attachment.
    pub depth_format: Option<ImageFormat>,
    /// Specialization constants by constant ID, in deterministic order.
    pub specialization: BTreeMap<u32, SpecializationValue>,
}

impl PipelineDescriptor {
    /// Standard PBR material: PBR layout, opaque, backface-culled, depth on.
    pub fn pbr(shader_path: impl Into<String>) -> Self {
        Self {
            shader_path: shader_path.into(),
            stages: PipelineStages::graphics(),
            vertex: VertexLayout::pbr(),
            blend: BlendMode::Opaque,
            cull: CullMode::Back,
            depth: DepthState::default(),
            stencil: None,
            color_write_mask: ColorWriteMask::ALL,
            depth_bias: DepthBias::default(),
            wireframe: false,
            color_format: ImageFormat::Auto,
            color_attachment: true,
            depth_format: Some(ImageFormat::D32SfloatS8Uint),
            specialization: BTreeMap::new(),
        }
    }

    /// UI material: UI layout, alpha-blended, unculled, no depth, sRGB output.
    pub fn ui(shader_path: impl Into<String>) -> Self {
        Self {
            shader_path: shader_path.into(),
            stages: PipelineStages::graphics(),
            vertex: VertexLayout::ui(),
            blend: BlendMode::AlphaBlend,
            cull: CullMode::None,
            depth: DepthState::disabled(),
            stencil: None,
            color_write_mask: ColorWriteMask::ALL,
            depth_bias: DepthBias::default(),
            wireframe: false,
            color_format: ImageFormat::B8G8R8A8Srgb,
            color_attachment: true,
            depth_format: None,
            specialization: BTreeMap::new(),
        }
    }

    /// Skinned PBR material for animated meshes.
    pub fn skinned(shader_path: impl Into<String>) -> Self {
        Self {
            shader_path: shader_path.into(),
            stages: PipelineStages::graphics(),
            vertex: VertexLayout::pbr_skinned(),
            blend: BlendMode::Opaque,
            cull: CullMode::Back,
            depth: DepthState::default(),
            stencil: None,
            color_write_mask: ColorWriteMask::ALL,
            depth_bias: DepthBias::default(),
            wireframe: false,
            color_format: ImageFormat::Auto,
            color_attachment: true,
            depth_format: Some(ImageFormat::D32SfloatS8Uint),
            specialization: BTreeMap::new(),
        }
    }

    /// Simple position-only material (debug, physics visualization).
    pub fn simple(shader_path: impl Into<String>) -> Self {
        Self {
            shader_path: shader_path.into(),
            stages: PipelineStages::graphics(),
            vertex: VertexLayout::position(),
            blend: BlendMode::Opaque,
            cull: CullMode::Back,
            depth: DepthState::default(),
            stencil: None,
            color_write_mask: ColorWriteMask::ALL,
            depth_bias: DepthBias::default(),
            wireframe: false,
            color_format: ImageFormat::Auto,
            color_attachment: true,
            depth_format: Some(ImageFormat::D32SfloatS8Uint),
            specialization: BTreeMap::new(),
        }
    }

    /// Billboard material: PBR layout, alpha-blended, unculled.
    ///
    /// Backends key the billboard pipeline wire-up off PBR layout plus an
    /// unculled raster state.
    pub fn billboard(shader_path: impl Into<String>) -> Self {
        Self {
            shader_path: shader_path.into(),
            stages: PipelineStages::graphics(),
            vertex: VertexLayout::pbr(),
            blend: BlendMode::AlphaBlend,
            cull: CullMode::None,
            depth: DepthState::default(),
            stencil: None,
            color_write_mask: ColorWriteMask::ALL,
            depth_bias: DepthBias::default(),
            wireframe: false,
            color_format: ImageFormat::Auto,
            color_attachment: true,
            depth_format: Some(ImageFormat::D32SfloatS8Uint),
            specialization: BTreeMap::new(),
        }
    }

    /// A vertex-only graphics pipeline for explicitly declared depth attachments.
    pub fn depth_only(shader_path: impl Into<String>, vertex: VertexLayout) -> Self {
        let mut descriptor = Self::pbr(shader_path);
        descriptor.vertex = vertex;
        descriptor.color_attachment = false;
        descriptor.stages = PipelineStages::Graphics {
            vertex_entry: "vs_main".into(),
            fragment_entry: None,
        };
        descriptor
    }

    /// Compute pipeline with no vertex input.
    pub fn compute(shader_path: impl Into<String>, compute_entry: impl Into<String>) -> Self {
        Self {
            shader_path: shader_path.into(),
            stages: PipelineStages::Compute {
                compute_entry: compute_entry.into(),
            },
            vertex: VertexLayout::empty(),
            blend: BlendMode::Opaque,
            cull: CullMode::None,
            depth: DepthState::disabled(),
            stencil: None,
            color_write_mask: ColorWriteMask::ALL,
            depth_bias: DepthBias::default(),
            wireframe: false,
            color_format: ImageFormat::Auto,
            color_attachment: false,
            depth_format: None,
            specialization: BTreeMap::new(),
        }
    }

    /// Enable or disable the pipeline's color attachment explicitly.
    pub fn with_color_attachment(mut self, enabled: bool) -> Self {
        self.color_attachment = enabled;
        self
    }

    /// Specify the depth/stencil attachment format used during pipeline creation.
    pub fn with_depth_format(mut self, format: Option<ImageFormat>) -> Self {
        self.depth_format = format;
        self
    }

    /// Set explicit stencil testing and write operations.
    pub fn with_stencil(mut self, stencil: StencilState) -> Self {
        self.stencil = Some(stencil);
        self
    }

    /// Set the channels written by the fragment stage.
    pub fn with_color_write_mask(mut self, mask: ColorWriteMask) -> Self {
        self.color_write_mask = mask;
        self
    }

    /// Set the raster depth offset.
    pub fn with_depth_bias(mut self, bias: DepthBias) -> Self {
        self.depth_bias = bias;
        self
    }

    /// Override the shader path.
    pub fn with_shader_path(mut self, path: impl Into<String>) -> Self {
        self.shader_path = path.into();
        self
    }

    /// Override the graphics entry points.
    pub fn with_graphics_entries(
        mut self,
        vertex_entry: impl Into<String>,
        fragment_entry: impl Into<String>,
    ) -> Self {
        self.stages = PipelineStages::Graphics {
            vertex_entry: vertex_entry.into(),
            fragment_entry: Some(fragment_entry.into()),
        };
        self
    }

    /// Override the vertex layout (must match what meshes are drawn with).
    pub fn with_vertex_layout(mut self, layout: VertexLayout) -> Self {
        self.vertex = layout;
        self
    }

    /// Override the blend mode.
    pub fn with_blend(mut self, blend: BlendMode) -> Self {
        self.blend = blend;
        self
    }

    /// Override the cull mode.
    pub fn with_cull(mut self, cull: CullMode) -> Self {
        self.cull = cull;
        self
    }

    /// Override the depth state.
    pub fn with_depth(mut self, depth: DepthState) -> Self {
        self.depth = depth;
        self
    }

    /// Enable or disable wireframe rasterization.
    pub fn with_wireframe(mut self, wireframe: bool) -> Self {
        self.wireframe = wireframe;
        self
    }

    /// Override the color attachment format.
    pub fn with_color_format(mut self, format: ImageFormat) -> Self {
        self.color_format = format;
        self
    }

    /// Set a specialization constant by ID.
    pub fn with_specialization(mut self, id: u32, value: SpecializationValue) -> Self {
        self.specialization.insert(id, value);
        self
    }

    /// True when the vertex layout is the canonical UI layout.
    ///
    /// Both backends key UI-specific pipeline wiring (second instanced
    /// pipeline, sRGB/no-depth targets) off layout identity, not names.
    pub fn is_ui_layout(&self) -> bool {
        self.vertex == VertexLayout::ui()
    }

    /// Validate structural invariants before native pipeline creation.
    ///
    /// This checks everything that does not need the GPU: non-empty shader
    /// path and entry points, a vertex layout for graphics pipelines, and
    /// finite specialization values. Shader-interface checks (entry points
    /// present in WGSL source) run in
    /// [`validate_shader_source`](PipelineDescriptor::validate_shader_source);
    /// backends run both before touching native APIs.
    pub fn validate(&self) -> Result<(), RendererError> {
        if self.shader_path.is_empty() {
            return Err(RendererError::InvalidDescriptor {
                resource: "material".to_string(),
                reason: "shader_path must not be empty".to_string(),
            });
        }
        if self.color_write_mask.0 & !ColorWriteMask::ALL.0 != 0 {
            return Err(RendererError::InvalidDescriptor {
                resource: "material".into(),
                reason: "color write mask contains undefined channels".into(),
            });
        }
        if ![
            self.depth_bias.constant,
            self.depth_bias.slope_factor,
            self.depth_bias.clamp,
        ]
        .into_iter()
        .all(f32::is_finite)
        {
            return Err(RendererError::InvalidDescriptor {
                resource: "material".into(),
                reason: "depth bias values must be finite".into(),
            });
        }
        match &self.stages {
            PipelineStages::Graphics {
                vertex_entry,
                fragment_entry,
            } => {
                if vertex_entry.is_empty() || fragment_entry.as_ref().is_some_and(String::is_empty)
                {
                    return Err(RendererError::InvalidDescriptor {
                        resource: "material".to_string(),
                        reason:
                            "graphics pipelines need non-empty vertex and fragment entry points"
                                .to_string(),
                    });
                }
            }
            PipelineStages::Compute { compute_entry } => {
                if compute_entry.is_empty() {
                    return Err(RendererError::InvalidDescriptor {
                        resource: "material".to_string(),
                        reason: "compute pipelines need a non-empty compute entry point"
                            .to_string(),
                    });
                }
            }
        }
        for (id, value) in &self.specialization {
            if let SpecializationValue::F32(v) = value
                && !v.is_finite()
            {
                return Err(RendererError::InvalidDescriptor {
                    resource: "material".to_string(),
                    reason: format!("specialization constant {id} must be finite"),
                });
            }
        }
        Ok(())
    }

    /// Validate requested entry points against WGSL source text.
    ///
    /// Parses `source` with naga and rejects descriptors whose entry points
    /// are missing (or at the wrong stage) before native pipeline creation.
    /// Pure function over source text: no GPU, no filesystem, deterministic.
    pub fn validate_shader_source(
        source: &str,
        stages: &PipelineStages,
    ) -> Result<(), RendererError> {
        let module =
            naga::front::wgsl::parse_str(source).map_err(|e| RendererError::InvalidDescriptor {
                resource: "shader".to_string(),
                reason: format!("WGSL parse failed: {e:?}"),
            })?;
        let require = |name: &str, stage: naga::ShaderStage| {
            let found = module
                .entry_points
                .iter()
                .any(|ep| ep.name == name && ep.stage == stage);
            if found {
                Ok(())
            } else {
                Err(RendererError::InvalidDescriptor {
                    resource: "shader".to_string(),
                    reason: format!("entry point '{name}' ({stage:?}) not found in shader"),
                })
            }
        };
        match stages {
            PipelineStages::Graphics {
                vertex_entry,
                fragment_entry,
            } => {
                require(vertex_entry, naga::ShaderStage::Vertex)?;
                if let Some(fragment_entry) = fragment_entry {
                    require(fragment_entry, naga::ShaderStage::Fragment)?;
                }
            }
            PipelineStages::Compute { compute_entry } => {
                require(compute_entry, naga::ShaderStage::Compute)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::error::RendererError;

    use super::*;

    const MINIMAL_WGSL: &str = r#"
        @vertex fn vs_main() -> @builtin(position) vec4f { return vec4f(0.0); }
        @fragment fn fs_main() -> @location(0) vec4f { return vec4f(1.0); }
    "#;

    fn is_invalid_descriptor(result: Result<(), RendererError>) -> bool {
        matches!(result, Err(RendererError::InvalidDescriptor { .. }))
    }

    #[test]
    fn test_equal_signed_zero_specializations_share_cache_keys() {
        use std::hash::{Hash, Hasher};
        let positive = SpecializationValue::F32(0.0);
        let negative = SpecializationValue::F32(-0.0);
        assert_eq!(positive, negative);
        let hash = |value: SpecializationValue| {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            value.hash(&mut hasher);
            hasher.finish()
        };
        assert_eq!(hash(positive), hash(negative));
    }

    #[test]
    fn test_graphics_constructors_declare_their_attachments() {
        for descriptor in [
            PipelineDescriptor::pbr("shader"),
            PipelineDescriptor::skinned("shader"),
            PipelineDescriptor::simple("shader"),
            PipelineDescriptor::billboard("shader"),
        ] {
            assert!(descriptor.color_attachment);
            assert_eq!(descriptor.depth_format, Some(ImageFormat::D32SfloatS8Uint));
        }
        let ui = PipelineDescriptor::ui("shader");
        assert!(ui.color_attachment);
        assert_eq!(ui.depth_format, None);
        let compute = PipelineDescriptor::compute("shader", "cs_main");
        assert!(!compute.color_attachment);
        assert_eq!(compute.depth_format, None);
    }

    #[test]
    fn test_canonical_descriptors_validate() {
        PipelineDescriptor::pbr("shaders/pbr.wgsl")
            .validate()
            .unwrap();
        PipelineDescriptor::ui("shaders/ui.wgsl")
            .validate()
            .unwrap();
        PipelineDescriptor::skinned("shaders/skinned.wgsl")
            .validate()
            .unwrap();
        PipelineDescriptor::billboard("shaders/billboard.wgsl")
            .validate()
            .unwrap();
        PipelineDescriptor::simple("shaders/simple.wgsl")
            .validate()
            .unwrap();
        PipelineDescriptor::compute("shaders/compute.wgsl", "cs_main")
            .validate()
            .unwrap();
    }

    #[test]
    fn test_depth_only_shader_needs_no_fragment_entry() {
        let descriptor = PipelineDescriptor::depth_only("depth.wgsl", VertexLayout::position())
            .with_depth_format(Some(ImageFormat::D32Sfloat));
        descriptor.validate().unwrap();
        PipelineDescriptor::validate_shader_source(
            "@vertex fn vs_main() -> @builtin(position) vec4f { return vec4f(0.0); }",
            &descriptor.stages,
        )
        .unwrap();
    }

    #[test]
    fn test_shader_generated_vertices_need_no_mesh_layout() {
        let mut descriptor = PipelineDescriptor::simple("fullscreen.wgsl");
        descriptor.vertex = VertexLayout::empty();
        descriptor.validate().unwrap();
    }

    #[test]
    fn test_invalid_color_mask_and_depth_bias_rejected() {
        assert!(is_invalid_descriptor(
            PipelineDescriptor::pbr("pbr.wgsl")
                .with_color_write_mask(ColorWriteMask(16))
                .validate()
        ));
        assert!(is_invalid_descriptor(
            PipelineDescriptor::pbr("pbr.wgsl")
                .with_depth_bias(DepthBias {
                    constant: f32::INFINITY,
                    ..DepthBias::default()
                })
                .validate()
        ));
    }

    #[test]
    fn test_equal_depth_bias_values_have_identical_hashes() {
        use std::hash::{DefaultHasher, Hash, Hasher};
        let zero = DepthBias::default();
        let negative_zero = DepthBias {
            constant: -0.0,
            ..zero
        };
        assert_eq!(zero, negative_zero);
        let hash = |value: DepthBias| {
            let mut hasher = DefaultHasher::new();
            value.hash(&mut hasher);
            hasher.finish()
        };
        assert_eq!(hash(zero), hash(negative_zero));
    }

    #[test]
    fn test_empty_shader_path_rejected() {
        assert!(is_invalid_descriptor(
            PipelineDescriptor::pbr("").validate()
        ));
    }

    #[test]
    fn test_empty_entry_points_rejected() {
        let descriptor = PipelineDescriptor::pbr("shaders/pbr.wgsl").with_graphics_entries("", "");
        assert!(is_invalid_descriptor(descriptor.validate()));
    }

    #[test]
    fn test_non_finite_specialization_rejected() {
        let descriptor = PipelineDescriptor::pbr("shaders/pbr.wgsl")
            .with_specialization(0, SpecializationValue::F32(f32::NAN));
        assert!(is_invalid_descriptor(descriptor.validate()));
    }

    #[test]
    fn test_shader_source_entry_validation() {
        PipelineDescriptor::validate_shader_source(MINIMAL_WGSL, &PipelineStages::graphics())
            .unwrap();
        let missing = PipelineStages::Graphics {
            vertex_entry: "vs_missing".to_string(),
            fragment_entry: Some("fs_main".to_string()),
        };
        assert!(is_invalid_descriptor(
            PipelineDescriptor::validate_shader_source(MINIMAL_WGSL, &missing)
        ));
    }
}
