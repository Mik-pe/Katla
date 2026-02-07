//! Uniform buffer layout system for materials.
//!
//! Provides a type-safe way to define uniform buffer layouts without magic byte sizes.
//! Supports dynamic layout composition and extensibility.

/// Descriptor for a single uniform field in the buffer.
#[derive(Debug, Clone, Copy)]
pub enum UniformField {
    /// 4x4 matrix (64 bytes)
    Mat4,
    /// 3-component vector (12 bytes)
    Vec3,
    /// 4-component vector (16 bytes)
    Vec4,
    /// Single float (4 bytes)
    Float,
}

impl UniformField {
    /// Get the size of this field in bytes.
    pub const fn size(&self) -> usize {
        match self {
            UniformField::Mat4 => 64,  // 4x4 floats * 4 bytes
            UniformField::Vec3 => 12,   // 3 floats * 4 bytes
            UniformField::Vec4 => 16,   // 4 floats * 4 bytes
            UniformField::Float => 4,   // 1 float * 4 bytes
        }
    }
}

/// Layout descriptor for a uniform buffer.
///
/// Defines what fields are present and their order, automatically calculating
/// the total buffer size and field offsets.
#[derive(Debug, Clone)]
pub struct UniformLayout {
    fields: Vec<UniformField>,
    offsets: Vec<usize>,
    total_size: usize,
}

impl UniformLayout {
    /// Create a new empty uniform layout.
    pub fn new() -> Self {
        Self {
            fields: Vec::new(),
            offsets: Vec::new(),
            total_size: 0,
        }
    }

    /// Add a field to the layout and return the modified layout.
    ///
    /// # Example
    /// ```no_run
    /// use katla_vulkan::vulkan::material::uniform_layout::{UniformLayout, UniformField};
    ///
    /// let layout = UniformLayout::new()
    ///     .with_field(UniformField::Mat4)  // model matrix
    ///     .with_field(UniformField::Mat4)  // view matrix
    ///     .with_field(UniformField::Mat4)  // projection matrix
    ///     .with_field(UniformField::Vec4); // color
    /// ```
    pub fn with_field(mut self, field: UniformField) -> Self {
        self.offsets.push(self.total_size);
        self.fields.push(field);
        self.total_size += field.size();
        self
    }

    /// Get the total size of the uniform buffer in bytes.
    pub fn total_size(&self) -> usize {
        self.total_size
    }

    /// Get the offset of a field by index.
    pub fn field_offset(&self, index: usize) -> Option<usize> {
        self.offsets.get(index).copied()
    }

    /// Get the number of fields in the layout.
    pub fn field_count(&self) -> usize {
        self.fields.len()
    }

    /// Check if this layout contains a field at the given index.
    pub fn has_field_at(&self, index: usize) -> bool {
        index < self.fields.len()
    }
}

impl Default for UniformLayout {
    fn default() -> Self {
        Self::new()
    }
}

/// Predefined uniform layouts for common material types.
impl UniformLayout {
    /// Standard PBR layout: world, view, projection matrices (192 bytes).
    pub fn pbr_matrices() -> Self {
        Self::new()
            .with_field(UniformField::Mat4)  // world
            .with_field(UniformField::Mat4)  // view
            .with_field(UniformField::Mat4)  // projection
    }

    /// PBR layout with color: world, view, projection matrices + color (208 bytes).
    pub fn pbr_with_color() -> Self {
        Self::pbr_matrices()
            .with_field(UniformField::Vec4)  // color
    }

    /// Minimal layout for testing (192 bytes, matrices only).
    pub fn matrices_only() -> Self {
        Self::pbr_matrices()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_sizes() {
        assert_eq!(UniformField::Mat4.size(), 64);
        assert_eq!(UniformField::Vec3.size(), 12);
        assert_eq!(UniformField::Vec4.size(), 16);
        assert_eq!(UniformField::Float.size(), 4);
    }

    #[test]
    fn test_pbr_matrices_size() {
        let layout = UniformLayout::pbr_matrices();
        assert_eq!(layout.total_size(), 192);
        assert_eq!(layout.field_count(), 3);
    }

    #[test]
    fn test_pbr_with_color_size() {
        let layout = UniformLayout::pbr_with_color();
        assert_eq!(layout.total_size(), 208);
        assert_eq!(layout.field_count(), 4);
    }

    #[test]
    fn test_field_offsets() {
        let layout = UniformLayout::pbr_with_color();
        assert_eq!(layout.field_offset(0), Some(0));   // world matrix
        assert_eq!(layout.field_offset(1), Some(64));  // view matrix
        assert_eq!(layout.field_offset(2), Some(128)); // projection matrix
        assert_eq!(layout.field_offset(3), Some(192)); // color
        assert_eq!(layout.field_offset(4), None);      // out of bounds
    }

    #[test]
    fn test_custom_layout() {
        let layout = UniformLayout::new()
            .with_field(UniformField::Vec4)  // position
            .with_field(UniformField::Float) // scale
            .with_field(UniformField::Vec4); // color

        assert_eq!(layout.total_size(), 36); // 16 + 4 + 16
        assert_eq!(layout.field_count(), 3);
        assert_eq!(layout.field_offset(0), Some(0));
        assert_eq!(layout.field_offset(1), Some(16));
        assert_eq!(layout.field_offset(2), Some(20));
    }
}
