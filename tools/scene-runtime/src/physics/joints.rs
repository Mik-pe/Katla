//! Prepared native constraints share the body's complete-batch admission boundary.
use super::{Body, BodyType, Entry, Shape};
use rapier3d::prelude::*;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(super) enum Kind {
    PointToPoint,
    Hinge,
    Distance,
    Fixed,
}
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct Joint {
    pub entity_id: String,
    kind: Kind,
    a: String,
    b: String,
    anchor_a: [f32; 3],
    anchor_b: [f32; 3],
    limits: Option<[f32; 2]>,
}
pub(super) struct Prepared {
    pub authored: Joint,
    pub a: u64,
    pub b: u64,
    pub native: GenericJoint,
}
pub(super) struct Owned {
    authored: Joint,
    handle: ImpulseJointHandle,
    a: RigidBodyHandle,
    b: RigidBodyHandle,
}
fn id(text: &str) -> Result<u64, String> {
    if text.is_empty()
        || (text.len() > 1 && text.starts_with('0'))
        || !text.bytes().all(|c| c.is_ascii_digit())
    {
        return Err("Joint reference must be canonical decimal u64".into());
    }
    text.parse().map_err(|_| "Joint reference overflow".into())
}
pub(super) fn prepare(
    input: Vec<Joint>,
    bodies: &BTreeMap<u64, (Body, Option<SharedShape>)>,
) -> Result<BTreeMap<u64, Prepared>, String> {
    if input.len() > 100_000 {
        return Err("Joint budget exceeded".into());
    }
    let mut staged = BTreeMap::new();
    for joint in input {
        let key = id(&joint.entity_id)?;
        let a = id(&joint.a)?;
        let b = id(&joint.b)?;
        if a == b {
            return Err("Joint endpoints must be distinct".into());
        }
        for endpoint in [a, b] {
            let (body, _) = bodies.get(&endpoint).ok_or("Missing joint endpoint")?;
            if body.body_type == BodyType::Fixed || matches!(body.shape, Shape::None) {
                return Err(
                    "Joint endpoints require a dynamic or kinematic body and collider".into(),
                );
            }
        }
        if !joint
            .anchor_a
            .iter()
            .chain(&joint.anchor_b)
            .all(|v| v.is_finite() && v.abs() <= 1.0e8)
        {
            return Err("Invalid joint anchor".into());
        }
        if let Some([minimum, maximum]) = joint.limits
            && (!minimum.is_finite()
                || !maximum.is_finite()
                || minimum > maximum
                || minimum.abs() > 1.0e8
                || maximum.abs() > 1.0e8)
        {
            return Err("Invalid joint limits".into());
        }
        let anchor_a = Vector::from_array(joint.anchor_a);
        let anchor_b = Vector::from_array(joint.anchor_b);
        let native: GenericJoint = match joint.kind {
            Kind::PointToPoint => SphericalJointBuilder::new()
                .local_anchor1(anchor_a)
                .local_anchor2(anchor_b)
                .build()
                .into(),
            Kind::Hinge => {
                let mut builder = RevoluteJointBuilder::new(Vector::new(0.0, 1.0, 0.0))
                    .local_anchor1(anchor_a)
                    .local_anchor2(anchor_b);
                if let Some(limits) = joint.limits {
                    builder = builder.limits(limits);
                }
                builder.build().into()
            }
            Kind::Distance => {
                let [minimum, maximum] = joint.limits.unwrap_or([0.0, 1.0]);
                SpringJointBuilder::new((minimum + maximum) * 0.5, 1.0, 0.5)
                    .local_anchor1(anchor_a)
                    .local_anchor2(anchor_b)
                    .build()
                    .into()
            }
            Kind::Fixed => FixedJointBuilder::new()
                .local_anchor1(anchor_a)
                .local_anchor2(anchor_b)
                .build()
                .into(),
        };
        if staged
            .insert(
                key,
                Prepared {
                    authored: joint,
                    a,
                    b,
                    native,
                },
            )
            .is_some()
        {
            return Err("Duplicate joint entity".into());
        }
    }
    Ok(staged)
}
pub(super) fn validate_owners(
    owned: &BTreeMap<u64, Owned>,
    native: &ImpulseJointSet,
) -> Result<(), String> {
    if owned.values().any(|entry| !native.contains(entry.handle)) {
        return Err("Missing native joint owner".into());
    }
    Ok(())
}
pub(super) fn remove_missing(
    owned: &mut BTreeMap<u64, Owned>,
    native: &mut ImpulseJointSet,
    staged: &BTreeMap<u64, Prepared>,
) {
    owned.retain(|id, entry| {
        if staged.contains_key(id) {
            return true;
        }
        native.remove(entry.handle, true);
        false
    });
}
pub(super) fn publish(
    owned: &mut BTreeMap<u64, Owned>,
    native: &mut ImpulseJointSet,
    bodies: &BTreeMap<u64, Entry>,
    staged: BTreeMap<u64, Prepared>,
) {
    for (id, prepared) in staged {
        let a = bodies[&prepared.a].body;
        let b = bodies[&prepared.b].body;
        if let Some(entry) = owned.get(&id) {
            if entry.authored == prepared.authored
                && entry.a == a
                && entry.b == b
                && native.contains(entry.handle)
            {
                continue;
            }
            native.remove(entry.handle, true);
        }
        let handle = native.insert(a, b, prepared.native, true);
        owned.insert(
            id,
            Owned {
                authored: prepared.authored,
                handle,
                a,
                b,
            },
        );
    }
}
