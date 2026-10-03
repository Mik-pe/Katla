//! Accessor iteration for glTF animation channels and inverse bind matrices.

use gltf::buffer::Data;
use katla_math::{Mat4, Vec4};

/// Reads animation accessors including sparse and interleaved data.
pub struct AttributeParser<'a> {
    buffers: &'a [Data],
}

impl<'a> AttributeParser<'a> {
    /// Borrow decoded glTF buffers.
    pub fn new(buffers: &'a [Data]) -> Self {
        Self { buffers }
    }

    /// Read scalar animation keyframes.
    pub fn parse_scalars(&self, accessor: gltf::Accessor<'a>) -> Vec<f32> {
        gltf::accessor::Iter::<f32>::new(accessor, |buffer| {
            self.buffers.get(buffer.index()).map(|data| &data.0[..])
        })
        .map(Iterator::collect)
        .unwrap_or_default()
    }

    /// Read translation or scale keyframes.
    pub fn parse_positions(&self, accessor: gltf::Accessor<'a>) -> Vec<[f32; 3]> {
        gltf::accessor::Iter::<[f32; 3]>::new(accessor, |buffer| {
            self.buffers.get(buffer.index()).map(|data| &data.0[..])
        })
        .map(Iterator::collect)
        .unwrap_or_default()
    }

    /// Read quaternion keyframes in XYZW order.
    pub fn parse_tangents(&self, accessor: gltf::Accessor<'a>) -> Vec<[f32; 4]> {
        gltf::accessor::Iter::<[f32; 4]>::new(accessor, |buffer| {
            self.buffers.get(buffer.index()).map(|data| &data.0[..])
        })
        .map(Iterator::collect)
        .unwrap_or_default()
    }

    /// Read inverse bind matrices without changing their column-major layout.
    pub fn parse_matrices(&self, accessor: gltf::Accessor<'a>) -> Vec<Mat4> {
        gltf::accessor::Iter::<[[f32; 4]; 4]>::new(accessor, |buffer| {
            self.buffers.get(buffer.index()).map(|data| &data.0[..])
        })
        .map(|values| values.map(|matrix| Mat4(matrix.map(Vec4::from))).collect())
        .unwrap_or_default()
    }
}
