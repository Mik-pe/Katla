//! Matched compile workload: eight query arities, typed scheduling and lifecycle.
use katla_ecs::{
    Component, Query, Read, SystemExecutionOrder, SystemParam, TypedSystem, World, Write,
};

const STEP: f32 = 1.0;
#[derive(Component, Default)]
struct C1 {
    value: f32,
}
#[derive(Component, Default)]
struct C2 {
    value: f32,
}
#[derive(Component, Default)]
struct C3 {
    value: f32,
}
#[derive(Component, Default)]
struct C4 {
    value: f32,
}
#[derive(Component, Default)]
struct C5 {
    value: f32,
}
#[derive(Component, Default)]
struct C6 {
    value: f32,
}
#[derive(Component, Default)]
struct C7 {
    value: f32,
}
#[derive(Component, Default)]
struct C8 {
    value: f32,
}

struct Movement;
impl TypedSystem for Movement {
    type Params = Query<(Write<C1>, Read<C2>)>;
    fn run(&mut self, mut query: <Self::Params as SystemParam>::Item<'_>, dt: f32) {
        for (_, (a, b)) in query.iter_mut() {
            a.value += b.value * dt;
        }
    }
}
fn main() {
    let mut world = World::new();
    let mut ids = Vec::new();
    for i in 0..2048 {
        ids.push(world.spawn((
            C1 { value: i as f32 },
            C2 { value: 2.0 },
            C3 { value: 3.0 },
            C4 { value: 4.0 },
            C5 { value: 5.0 },
            C6 { value: 6.0 },
            C7 { value: 7.0 },
            C8 { value: 8.0 },
        )));
    }
    let mut checksum = 0.0_f64;
    for (_, c1) in world.query_ref::<&C1>() {
        checksum += f64::from(c1.value);
    }
    for (_, c1, c2) in world.query_ref::<(&C1, &C2)>() {
        checksum += f64::from(c1.value) + f64::from(c2.value);
    }
    for (_, c1, c2, c3) in world.query_ref::<(&C1, &C2, &C3)>() {
        checksum += f64::from(c1.value) + f64::from(c2.value) + f64::from(c3.value);
    }
    for (_, c1, c2, c3, c4) in world.query_ref::<(&C1, &C2, &C3, &C4)>() {
        checksum +=
            f64::from(c1.value) + f64::from(c2.value) + f64::from(c3.value) + f64::from(c4.value);
    }
    for (_, c1, c2, c3, c4, c5) in world.query_ref::<(&C1, &C2, &C3, &C4, &C5)>() {
        checksum += f64::from(c1.value)
            + f64::from(c2.value)
            + f64::from(c3.value)
            + f64::from(c4.value)
            + f64::from(c5.value);
    }
    for (_, c1, c2, c3, c4, c5, c6) in world.query_ref::<(&C1, &C2, &C3, &C4, &C5, &C6)>() {
        checksum += f64::from(c1.value)
            + f64::from(c2.value)
            + f64::from(c3.value)
            + f64::from(c4.value)
            + f64::from(c5.value)
            + f64::from(c6.value);
    }
    for (_, c1, c2, c3, c4, c5, c6, c7) in world.query_ref::<(&C1, &C2, &C3, &C4, &C5, &C6, &C7)>()
    {
        checksum += f64::from(c1.value)
            + f64::from(c2.value)
            + f64::from(c3.value)
            + f64::from(c4.value)
            + f64::from(c5.value)
            + f64::from(c6.value)
            + f64::from(c7.value);
    }
    for (_, c1, c2, c3, c4, c5, c6, c7, c8) in
        world.query_ref::<(&C1, &C2, &C3, &C4, &C5, &C6, &C7, &C8)>()
    {
        checksum += f64::from(c1.value)
            + f64::from(c2.value)
            + f64::from(c3.value)
            + f64::from(c4.value)
            + f64::from(c5.value)
            + f64::from(c6.value)
            + f64::from(c7.value)
            + f64::from(c8.value);
    }
    world.register_typed_system(Movement, SystemExecutionOrder::NORMAL);
    for _ in 0..3 {
        world.update(STEP);
    }
    for id in ids.iter().step_by(3) {
        assert!(world.destroy_entity(*id));
    }
    for _ in 0..100 {
        world.spawn((C1 { value: 10.0 }, C2 { value: 2.0 }));
    }
    world.update(STEP);
    for (_, a) in world.query_ref::<&C1>() {
        checksum += f64::from(a.value);
    }
    assert!(world.validate().is_ok());
    assert_eq!(world.entity_count(), 1465);
    #[cfg(feature = "editor")]
    {
        use katla_ecs::scene_tool::{
            ComponentRegistry, ComponentRegistryEntry, FieldValue, SceneOp, SceneToolExecutor,
        };
        use katla_ecs::{FieldConstraints, FieldInfo, FieldKind};
        let mut registry = ComponentRegistry::new();
        registry.register(ComponentRegistryEntry {
            type_name: "C1",
            has_component: |w, id| w.get_component::<C1>(id).is_some(),
            create_default: |w, id| w.add_component(id, C1::default()),
            remove_component: |w, id| {
                w.remove_component::<C1>(id);
            },
            get_fields: |_w, _id| {
                vec![FieldInfo {
                    name: "value",
                    display_name: "value",
                    type_name: "f32",
                    kind: FieldKind::Float,
                    constraints: FieldConstraints::default(),
                }]
            },
            get_field_value: |w, id, field| {
                if field == "value" {
                    w.get_component::<C1>(id).map(|v| FieldValue::F32(v.value))
                } else {
                    None
                }
            },
            set_field_value: |w, id, field, value| {
                if field == "value"
                    && let FieldValue::F32(value) = value
                    && let Some(c) = w.get_component_mut::<C1>(id)
                {
                    c.value = value;
                    return Ok(());
                }
                Err(katla_ecs::scene_tool::SceneToolError::WorldError(
                    "invalid field".into(),
                ))
            },
        });
        let entity = ids[1];
        let (result, mut undo) = SceneToolExecutor::execute(
            SceneOp::SetField {
                entity,
                component: "C1".into(),
                field: "value".into(),
                value: serde_json::json!(25.0),
            },
            &mut world,
            &registry,
        )
        .expect("scene set");
        assert!(result.success);
        checksum += f64::from(world.get_component::<C1>(entity).expect("component").value);
        undo.undo_all(&mut world).expect("undo");
    }
    println!("{checksum:.0}");
}
