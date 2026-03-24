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
    /// Current LOCAL transforms for each joint (from animation)
    pub local_transforms: Vec<Mat4>,
    /// Current world transforms for each joint (computed from hierarchy)
    pub world_transforms: Vec<Mat4>,
    /// Final skinning matrices (world_transform * inverse_bind_matrix)
    /// These are what get uploaded to the GPU
    pub joint_transforms: Vec<Mat4>,
    /// Parent index for each joint (None = root)
    /// Index is into the joint array, not GLTF node index
    pub parent_indices: Vec<Option<usize>>,
}

impl Skeleton {
    /// Create a new skeleton
    pub fn new(name: impl Into<String>, joint_count: usize) -> Self {
        Self {
            name: name.into(),
            local_transforms: vec![Mat4::identity(); joint_count],
            world_transforms: vec![Mat4::identity(); joint_count],
            joint_transforms: vec![Mat4::identity(); joint_count],
            parent_indices: vec![None; joint_count],
        }
    }

    /// Create a skeleton with parent information
    pub fn with_parents(name: impl Into<String>, parent_indices: Vec<Option<usize>>) -> Self {
        let joint_count = parent_indices.len();
        Self {
            name: name.into(),
            local_transforms: vec![Mat4::identity(); joint_count],
            world_transforms: vec![Mat4::identity(); joint_count],
            joint_transforms: vec![Mat4::identity(); joint_count],
            parent_indices,
        }
    }

    /// Create a skeleton with parent information and initial local transforms (rest pose)
    pub fn with_rest_pose(
        name: impl Into<String>,
        parent_indices: Vec<Option<usize>>,
        local_transforms: Vec<Mat4>,
    ) -> Self {
        let joint_count = parent_indices.len();
        Self {
            name: name.into(),
            local_transforms,
            world_transforms: vec![Mat4::identity(); joint_count],
            joint_transforms: vec![Mat4::identity(); joint_count],
            parent_indices,
        }
    }

    /// Get the number of joints
    pub fn joint_count(&self) -> usize {
        self.joint_transforms.len()
    }

    /// Update a specific joint's LOCAL transform
    pub fn set_local_transform(&mut self, joint_index: usize, transform: Mat4) {
        if joint_index < self.local_transforms.len() {
            self.local_transforms[joint_index] = transform;
        }
    }

    /// Get a specific joint's world transform
    pub fn get_joint_transform(&self, joint_index: usize) -> Option<Mat4> {
        self.joint_transforms.get(joint_index).cloned()
    }

    /// Compute world transforms from local transforms using hierarchy.
    /// Must be called after updating local_transforms and before using joint_transforms.
    pub fn compute_world_transforms(&mut self) {
        // Process joints in order - since parents come before children in GLTF,
        // we can compute world transforms in a single pass
        for i in 0..self.world_transforms.len() {
            let local = self.local_transforms[i];
            if let Some(Some(parent_idx)) = self.parent_indices.get(i) {
                if *parent_idx < self.world_transforms.len() {
                    self.world_transforms[i] = self.world_transforms[*parent_idx] * local;
                } else {
                    self.world_transforms[i] = local;
                }
            } else {
                self.world_transforms[i] = local;
            }
        }
    }

    /// Compute final skinning matrices by applying inverse bind matrices.
    /// Must be called after compute_world_transforms().
    pub fn compute_skinning_matrices(&mut self, inverse_bind_matrices: &[Mat4]) {
        for (i, skin_matrix) in self.joint_transforms.iter_mut().enumerate() {
            if i < inverse_bind_matrices.len() {
                *skin_matrix = self.world_transforms[i].mul(&inverse_bind_matrices[i]);
            }
        }
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
