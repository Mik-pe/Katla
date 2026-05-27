//! Pipeline kind discriminant for consolidated pipeline initialization.
//!
//! Each variant maps to a specific GPU pipeline that must be initialized
//! before the render graph can dispatch passes of that kind. The number of
//! shader paths expected by [`crate::renderer::GpuRenderer::init_pass_pipeline`]
//! is documented per variant.

/// Identifies which pipeline to initialize via `init_pass_pipeline`.
///
/// Path count contract:
/// - **1 path**: Shadow, ShadowSkinned, DepthPrepass, DepthPrepassSkinned,
///   DepthPrepassBillboard, Picking, PickingSkinned, Sky, Tonemap
/// - **2 paths**: StencilIndicator (base + skinned)
/// - **4 paths**: Outline (stencil_mark + stencil_mark_skinned + outline_draw + outline_draw_skinned)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PipelineKind {
    Shadow,
    ShadowSkinned,
    DepthPrepass,
    DepthPrepassSkinned,
    DepthPrepassBillboard,
    Outline,
    StencilIndicator,
    Picking,
    PickingSkinned,
    Sky,
    Tonemap,
}
