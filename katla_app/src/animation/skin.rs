use katla_ecs::Component;
use katla_math::Mat4;
use std::fmt;

/// Skin data for skeletal animation.
///
/// A skin defines the skeleton (joints) and how vertices are bound to those joints.
#[derive(Debug, Clone, Component)]
pub struct Skin {
    /// Name of this skin
    pub name: String,
    /// Joint nodes (indices into the GLTF node hierarchy)
    pub joints: Vec<usize>,
    /// Inverse bind matrices for each joint
    ///
    /// Transforms vertices from mesh space to joint local space
    pub inverse_bind_matrices: Vec<Mat4>,
}

impl Skin {
    /// Create a new skin
    pub fn new(
        name: impl Into<String>,
        joints: Vec<usize>,
        inverse_bind_matrices: Vec<Mat4>,
    ) -> Self {
        Self {
            name: name.into(),
            joints,
            inverse_bind_matrices,
        }
    }

    /// Get the number of joints in this skin
    pub fn joint_count(&self) -> usize {
        self.joints.len()
    }

    /// Check if this is a valid skin
    pub fn is_valid(&self) -> bool {
        !self.joints.is_empty() && self.joints.len() == self.inverse_bind_matrices.len()
    }
}

impl fmt::Display for Skin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Skin '{}' with {} joints", self.name, self.joint_count())
    }
}

/// Joint transform hierarchy for skeletal animation.
///
/// Stores the current transform of each joint in a skeleton.
#[derive(Debug, Clone, Component)]
pub struct Skeleton {
    /// Name of this skeleton
    pub name: String,
    /// Current world transforms for each joint
    pub joint_transforms: Vec<Mat4>,
}

impl Skeleton {
    /// Create a new skeleton
    pub fn new(name: impl Into<String>, joint_count: usize) -> Self {
        Self {
            name: name.into(),
            joint_transforms: vec![Mat4::identity(); joint_count],
        }
    }

    /// Get the number of joints
    pub fn joint_count(&self) -> usize {
        self.joint_transforms.len()
    }

    /// Update a specific joint's transform
    pub fn set_joint_transform(&mut self, joint_index: usize, transform: Mat4) {
        if joint_index < self.joint_transforms.len() {
            self.joint_transforms[joint_index] = transform;
        }
    }

    /// Get a specific joint's transform
    pub fn get_joint_transform(&self, joint_index: usize) -> Option<Mat4> {
        self.joint_transforms.get(joint_index).cloned()
    }
}

/// Joint indices and weights for vertex skinning.
///
/// Each vertex can be influenced by up to 4 joints.
#[derive(Debug, Copy, Clone, Default, Component)]
#[repr(C)]
pub struct JointWeights {
    /// Indices of the 4 joints that influence this vertex
    pub joint_indices: [u16; 4],
    /// Weights of each joint influence (must sum to 1.0)
    pub weights: [f32; 4],
}

impl JointWeights {
    /// Create new joint weights
    pub fn new(joint_indices: [u16; 4], weights: [f32; 4]) -> Self {
        Self {
            joint_indices,
            weights,
        }
    }

    /// Create from raw arrays (GLTF format)
    ///
    /// GLTF stores joints as u8 but we use u16 for flexibility
    pub fn from_gltf(joints: [u8; 4], weights: [f32; 4]) -> Self {
        Self {
            joint_indices: [
                joints[0] as u16,
                joints[1] as u16,
                joints[2] as u16,
                joints[3] as u16,
            ],
            weights,
        }
    }
}
