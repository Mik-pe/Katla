//! Backend-neutral typed pipeline/material compilation descriptor.
//!
//! This module replaces the old string-dispatched material inputs
//! (`vertex_type: &str`, `"pbr"` / `"ui"` / `"skinned"` / ...). A
//! [`PipelineDescriptor`] carries everything needed to compile a pipeline on
//! any backend: shader identity and entry points, the canonical vertex
//! layout (the same [`VertexLayout`](crate::vertex::VertexLayout) model used
//! by mesh creation), portable render state, specialization constants, and
//! explicit per-backend native extension points.
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
        fragment_entry: String,
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
            fragment_entry: "fs_main".to_string(),
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
            // Hash by bits so NaN handling is explicit; validation rejects
            // non-finite values before they can reach a backend.
            Self::F32(v) => v.to_bits().hash(state),
        }
    }
}

/// Vulkan-only pipeline options.
///
/// Anything here affects Vulkan ABI concepts (descriptor set layouts) that
/// have no portable meaning. Other backends ignore this struct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct VulkanPipelineOptions {
    /// Whether this material uses compositing (requires set 2 descriptor set
    /// layout, provided by the frame graph). Default is false.
    pub compositing: bool,
}

/// Metal-only pipeline options.
///
/// Reserved for future Metal-specific pipeline inputs. Other backends ignore
/// this struct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct MetalPipelineOptions {}

/// Explicit per-backend native extension points.
///
/// Portable fields live on [`PipelineDescriptor`] itself; anything that only
/// makes sense on one backend goes here so backends never have to guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct NativePipelineOptions {
    /// Vulkan-only options.
    pub vulkan: VulkanPipelineOptions,
    /// Metal-only options.
    pub metal: MetalPipelineOptions,
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
    /// Wireframe rasterization.
    pub wireframe: bool,
    /// Color attachment format (`Auto` = backend default / deferred).
    pub color_format: ImageFormat,
    /// Specialization constants by constant ID, in deterministic order.
    pub specialization: BTreeMap<u32, SpecializationValue>,
    /// Backend-only extension points.
    pub native: NativePipelineOptions,
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
            wireframe: false,
            color_format: ImageFormat::Auto,
            specialization: BTreeMap::new(),
            native: NativePipelineOptions::default(),
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
            wireframe: false,
            color_format: ImageFormat::B8G8R8A8Srgb,
            specialization: BTreeMap::new(),
            native: NativePipelineOptions::default(),
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
            wireframe: false,
            color_format: ImageFormat::Auto,
            specialization: BTreeMap::new(),
            native: NativePipelineOptions::default(),
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
            wireframe: false,
            color_format: ImageFormat::Auto,
            specialization: BTreeMap::new(),
            native: NativePipelineOptions::default(),
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
            wireframe: false,
            color_format: ImageFormat::Auto,
            specialization: BTreeMap::new(),
            native: NativePipelineOptions::default(),
        }
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
            wireframe: false,
            color_format: ImageFormat::Auto,
            specialization: BTreeMap::new(),
            native: NativePipelineOptions::default(),
        }
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
            fragment_entry: fragment_entry.into(),
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

    /// Enable or disable Vulkan compositing descriptor set layout use.
    pub fn with_vulkan_compositing(mut self, compositing: bool) -> Self {
        self.native.vulkan.compositing = compositing;
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
        match &self.stages {
            PipelineStages::Graphics {
                vertex_entry,
                fragment_entry,
            } => {
                if vertex_entry.is_empty() || fragment_entry.is_empty() {
                    return Err(RendererError::InvalidDescriptor {
                        resource: "material".to_string(),
                        reason:
                            "graphics pipelines need non-empty vertex and fragment entry points"
                                .to_string(),
                    });
                }
                if self.vertex.is_empty() || self.vertex.stride() == 0 {
                    return Err(RendererError::InvalidDescriptor {
                        resource: "material".to_string(),
                        reason: "graphics pipelines need a non-empty vertex layout".to_string(),
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
                require(fragment_entry, naga::ShaderStage::Fragment)?;
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
            fragment_entry: "fs_main".to_string(),
        };
        assert!(is_invalid_descriptor(
            PipelineDescriptor::validate_shader_source(MINIMAL_WGSL, &missing)
        ));
    }
}
