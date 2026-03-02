//! Vertex data definition for mesh geometry.

pub use crate::vulkan::vertex_attr_set::VertexAttributeSet;
pub use crate::vulkan::vertex_attribute::{AttributeBinding, AttributeType};
pub use crate::vulkan::vertexbinding::{
    get_pbr_vertex_binding, get_skinned_vertex_binding, VertexBinding, VertexFormat,
};
