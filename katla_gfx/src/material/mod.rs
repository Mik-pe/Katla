//! Unified material system.
//!
//! Provides material types for rendering. Materials are created using the
//! VulkanRenderer material creation API (see `vulkan::material::compiler`).

mod definition;
mod render_state;
mod shader_source;
mod types;
mod ui;

// Public API
pub use definition::MaterialDomain;
pub use render_state::RenderState;
pub use shader_source::ShaderSource;

// Public material type
pub use types::Material;
