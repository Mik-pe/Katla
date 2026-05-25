//! Joint/constraint components for physics simulation.
//!
//! Defines joint types (point-to-point, hinge, distance, fixed) as ECS
//! components. Each joint references two entities via their `RigidBody` handles.

use katla_ecs::Component;
use rapier3d::dynamics::ImpulseJointHandle;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum JointType {
    PointToPoint,
    Hinge,
    Distance,
    Fixed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct JointLimits {
    pub min: f32,
    pub max: f32,
}

#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct Joint {
    pub joint_type: JointType,
    pub entity_a: u64,
    pub entity_b: u64,
    pub anchor_a: [f32; 3],
    pub anchor_b: [f32; 3],
    pub limits: Option<JointLimits>,
    #[serde(skip)]
    pub joint_handle: Option<ImpulseJointHandle>,
}

impl Joint {
    pub fn point_to_point(
        entity_a: u64,
        entity_b: u64,
        anchor_a: [f32; 3],
        anchor_b: [f32; 3],
    ) -> Self {
        Self {
            joint_type: JointType::PointToPoint,
            entity_a,
            entity_b,
            anchor_a,
            anchor_b,
            limits: None,
            joint_handle: None,
        }
    }

    pub fn hinge(
        entity_a: u64,
        entity_b: u64,
        anchor_a: [f32; 3],
        anchor_b: [f32; 3],
        limits: Option<JointLimits>,
    ) -> Self {
        Self {
            joint_type: JointType::Hinge,
            entity_a,
            entity_b,
            anchor_a,
            anchor_b,
            limits,
            joint_handle: None,
        }
    }

    pub fn distance(entity_a: u64, entity_b: u64, min_dist: f32, max_dist: f32) -> Self {
        Self {
            joint_type: JointType::Distance,
            entity_a,
            entity_b,
            anchor_a: [0.0; 3],
            anchor_b: [0.0; 3],
            limits: Some(JointLimits {
                min: min_dist,
                max: max_dist,
            }),
            joint_handle: None,
        }
    }

    pub fn fixed(entity_a: u64, entity_b: u64, anchor_a: [f32; 3], anchor_b: [f32; 3]) -> Self {
        Self {
            joint_type: JointType::Fixed,
            entity_a,
            entity_b,
            anchor_a,
            anchor_b,
            limits: None,
            joint_handle: None,
        }
    }

    pub fn is_spawned(&self) -> bool {
        self.joint_handle.is_some()
    }
}
