//! Unified material system.
//!
//! Provides material types and pipeline caching. Materials are defined using
//! MaterialDefinition for flexible, data-driven pipeline creation.

mod definition;
mod render_state;
mod shader_source;
mod template;
mod types;

// Public configuration types
pub use template::MaterialDefinition;

// Public API
pub use definition::MaterialDomain;
pub use render_state::RenderState;
pub use shader_source::ShaderSource;

// Public material type
pub use types::Material;
