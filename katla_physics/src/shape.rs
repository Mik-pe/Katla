//! Collision shape definitions.
//!
//! Primitive shapes used for collision detection. Each shape is defined in
//! local space relative to the entity's transform.

use serde::{Deserialize, Serialize};

/// A sphere defined by its radius.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SphereShape {
    pub radius: f32,
}

impl SphereShape {
    pub fn new(radius: f32) -> Self {
        Self { radius }
    }
}

/// An axis-aligned box defined by half-extents along each axis.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BoxShape {
    pub half_extents: [f32; 3],
}

impl BoxShape {
    pub fn new(half_extents: katla_math::Vec3) -> Self {
        Self {
            half_extents: [half_extents.x(), half_extents.y(), half_extents.z()],
        }
    }

    pub fn from_extents(width: f32, height: f32, depth: f32) -> Self {
        Self {
            half_extents: [width * 0.5, height * 0.5, depth * 0.5],
        }
    }

    pub fn half_extents_vec(&self) -> katla_math::Vec3 {
        katla_math::Vec3::new(
            self.half_extents[0],
            self.half_extents[1],
            self.half_extents[2],
        )
    }
}

/// A capsule defined by half-height (distance from center to hemisphere center)
/// and radius.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CapsuleShape {
    pub half_height: f32,
    pub radius: f32,
}

impl CapsuleShape {
    pub fn new(half_height: f32, radius: f32) -> Self {
        Self {
            half_height,
            radius,
        }
    }
}

/// A heightfield defined by a 2D grid of height values.
///
/// Heights are stored in row-major order with `cols` values per row and `rows` rows total.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeightfieldShape {
    pub rows: u32,
    pub cols: u32,
    pub heights: Vec<f32>,
}

impl HeightfieldShape {
    pub fn new(rows: u32, cols: u32, heights: Vec<f32>) -> Self {
        assert_eq!(heights.len(), (rows * cols) as usize);
        Self {
            rows,
            cols,
            heights,
        }
    }
}
