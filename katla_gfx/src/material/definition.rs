//! Material key types for pipeline caching.

use std::hash::Hash;

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
