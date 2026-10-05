//! SOA (Structure of Arrays) vertex attribute set collection.
//!
//! This module provides a collection type for managing separate attribute buffers,
//! used for flexible rendering pipelines and efficient GPU memory access patterns.

use std::collections::HashMap;

use super::vertex_attribute::{AttributeBinding, AttributeType};

/// Collection of attribute buffers for a mesh (SOA layout).
///
/// In SOA (Structure of Arrays) layout, each attribute type has
/// its own buffer, enabling:
/// - Depth-only passes (only bind position buffer)
/// - Shadow mapping (position only, no need for normals/UVs)
/// - Deferred rendering (G-buffer fills bind only needed attributes)
/// - Better cache locality (position traversal doesn't pull unused data)
pub struct VertexAttributeSet {
    attributes: HashMap<AttributeType, AttributeBinding>,
    vertex_count: u32,
}

impl VertexAttributeSet {
    /// Create a new empty vertex attribute set.
    pub fn new(vertex_count: u32) -> Self {
        Self {
            attributes: HashMap::new(),
            vertex_count,
        }
    }

    /// Add an attribute binding to this set.
    pub fn add_attribute(&mut self, binding: AttributeBinding) {
        self.attributes.insert(binding.attr_type, binding);
    }

    /// Get an attribute binding by type.
    pub fn get(&self, attr_type: AttributeType) -> Option<&AttributeBinding> {
        self.attributes.get(&attr_type)
    }

    /// Get the vertex count.
    pub fn vertex_count(&self) -> u32 {
        self.vertex_count
    }

    /// Get all attribute types in this set.
    pub fn attribute_types(&self) -> Vec<AttributeType> {
        self.attributes.keys().copied().collect()
    }

    /// Check if this set has specific attributes.
    pub fn has_attributes(&self, required: &[AttributeType]) -> bool {
        required
            .iter()
            .all(|attr| self.attributes.contains_key(attr))
    }
}
