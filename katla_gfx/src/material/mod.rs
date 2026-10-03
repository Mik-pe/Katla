//! Material system.
//!
//! Create materials through `GpuRenderer::compile_material` with a
//! backend-neutral `PipelineDescriptor`. Surface semantics belong to the app.

mod definition;

pub use definition::MaterialDomain;
