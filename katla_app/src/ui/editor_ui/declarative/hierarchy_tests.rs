use super::*;
use crate::ui::editor_ui::types::is_entity_visible_fast;
use katla_ecs::World;
use katla_math::{Vec2, Vec3};
use katla_ui::declarative::{ViewTree, widgets::selectable::Selectable};

fn entity(id: EntityId, parent_id: Option<EntityId>) -> EntityInfo {
    EntityInfo {
        id,
        name: format!("row {}", id.id()),
        position: Vec3::ZERO,
        rotation: Vec3::ZERO,
        scale: Vec3::new(1.0, 1.0, 1.0),
        entity_type: "Empty".into(),
        components: vec![],
        depth: 0,
        has_children: false,
        parent_id,
        point_light: None,
        particle_emitter: None,
        script_path: None,
        perspective: None,
        directional_light: None,
        audio_emitter: None,
        audio_source: None,
        has_audio_listener: false,
        collider_shape: None,
        rigid_body: None,
        physics_material: None,
        material: None,
    }
}

#[test]
fn test_hierarchy_metadata_matches_independent_ancestry_walks() {
    let mut world = World::new();
    let ids: Vec<_> = (0..256).map(|_| world.create_entity()).collect();
    let missing_parent = world.create_entity();
    let mut entities: Vec<_> = ids
        .iter()
        .enumerate()
        .map(|(index, &id)| {
            let parent = match index {
                0 => None,
                1 => Some(missing_parent),
                _ => Some(ids[(index - 1) / 2]),
            };
            entity(id, parent)
        })
        .collect();
    entities.reverse();
    let parents: HashMap<_, _> = entities.iter().map(|e| (e.id, e.parent_id)).collect();
    for stride in [1, 2, 7] {
        let expanded: HashSet<_> = ids.iter().step_by(stride).copied().collect();
        let filtered: Vec<_> = entities.iter().collect();
        let rows = prepare_hierarchy_rows(&entities, &filtered, &expanded);
        for (entity, row) in filtered.iter().zip(rows) {
            let mut depth = 0;
            let mut parent = entity.parent_id;
            while let Some(id) = parent {
                depth += 1;
                parent = parents.get(&id).copied().flatten();
            }
            assert_eq!(row.depth, depth, "{:?}", entity.id);
            assert_eq!(
                row.visible,
                is_entity_visible_fast(entity, &parents, &expanded),
                "{:?}",
                entity.id
            );
            assert_eq!(
                row.has_children,
                filtered.iter().any(|e| e.parent_id == Some(entity.id))
            );
        }
    }
}

#[test]
fn test_filtered_children_use_filtered_rows_and_full_ancestry() {
    let mut world = World::new();
    let root = world.create_entity();
    let child = world.create_entity();
    let grandchild = world.create_entity();
    let entities = vec![
        entity(root, None),
        entity(child, Some(root)),
        entity(grandchild, Some(child)),
    ];
    let filtered = vec![&entities[2], &entities[0]];
    assert_eq!(
        prepare_hierarchy_rows(&entities, &filtered, &HashSet::new()),
        vec![
            HierarchyRow {
                depth: 2,
                visible: false,
                has_children: false,
            },
            HierarchyRow {
                depth: 0,
                visible: true,
                has_children: false,
            },
        ]
    );
    let rows = prepare_hierarchy_rows(&entities, &filtered, &HashSet::from([root, child]));
    assert!(rows.iter().all(|row| row.visible));
    assert!(prepare_hierarchy_rows(&entities, &[], &HashSet::new()).is_empty());
}

#[test]
fn test_filtered_roots_ignore_unfiltered_child_chains() {
    let mut world = World::new();
    let ids: Vec<_> = (0..5).map(|_| world.create_entity()).collect();
    let entities = vec![
        entity(ids[0], None),
        entity(ids[1], Some(ids[0])),
        entity(ids[2], Some(ids[1])),
        entity(ids[3], None),
        entity(ids[4], Some(ids[3])),
    ];
    let filtered = vec![&entities[3], &entities[0]];
    assert_eq!(
        prepare_hierarchy_rows(&entities, &filtered, &HashSet::new()),
        vec![
            HierarchyRow {
                depth: 0,
                visible: true,
                has_children: false,
            };
            filtered.len()
        ]
    );
}

#[test]
fn test_missing_parent_still_counts_as_an_ancestor_and_requires_expansion() {
    let mut world = World::new();
    let missing_parent = world.create_entity();
    let child = world.create_entity();
    let entities = vec![entity(child, Some(missing_parent))];
    let filtered = vec![&entities[0]];
    let rows = prepare_hierarchy_rows(&entities, &filtered, &HashSet::new());
    assert_eq!(rows[0].depth, 1);
    assert!(!rows[0].visible);
    let rows = prepare_hierarchy_rows(&entities, &filtered, &HashSet::from([missing_parent]));
    assert!(rows[0].visible);
}

#[test]
fn test_deep_hierarchy_metadata_is_iterative_and_order_independent() {
    let mut world = World::new();
    let mut parent = None;
    let mut entities: Vec<_> = (0..10_000)
        .map(|_| {
            let id = world.create_entity();
            let next = entity(id, parent);
            parent = Some(id);
            next
        })
        .collect();
    let expanded = entities.iter().map(|e| e.id).collect();
    for reverse in [false, true] {
        if reverse {
            entities.reverse();
        }
        let filtered: Vec<_> = entities.iter().collect();
        let rows = prepare_hierarchy_rows(&entities, &filtered, &expanded);
        for (index, row) in rows.iter().enumerate() {
            assert_eq!(row.depth, if reverse { 9_999 - index } else { index });
            assert!(row.visible);
        }
    }
}

#[test]
fn test_independent_roots_following_deep_ancestry_keep_their_own_metadata() {
    let mut world = World::new();
    let mut parent = None;
    let mut entities: Vec<_> = (0..4096)
        .map(|_| {
            let id = world.create_entity();
            let next = entity(id, parent);
            parent = Some(id);
            next
        })
        .collect();
    entities.reverse();
    entities.extend((0..4096).map(|_| entity(world.create_entity(), None)));
    let expanded = entities.iter().map(|e| e.id).collect();
    let filtered: Vec<_> = entities.iter().collect();
    let rows = prepare_hierarchy_rows(&entities, &filtered, &expanded);
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(row.depth, 4095usize.saturating_sub(index));
        assert!(row.visible);
        assert_eq!(row.has_children, index > 0 && index < 4096);
    }
}

#[test]
fn test_display_names_number_duplicates_in_filtered_row_order() {
    let mut world = World::new();
    let mut entities: Vec<_> = (0..4)
        .map(|_| entity(world.create_entity(), None))
        .collect();
    entities[0].name = "Sphere".into();
    entities[1].name = "Cube".into();
    entities[2].name = "Sphere".into();
    entities[3].name = "Sphere".into();
    assert_eq!(
        build_display_names(&entities.iter().collect::<Vec<_>>()),
        ["Sphere.001", "Cube", "Sphere.002", "Sphere.003"]
    );
    assert_eq!(
        build_display_names(&[&entities[3], &entities[0], &entities[1]]),
        ["Sphere.001", "Sphere.002", "Cube"]
    );
}

#[test]
fn test_cycles_have_bounded_depth_and_remain_hidden_in_any_row_order() {
    let mut world = World::new();
    let ids: Vec<_> = (0..7).map(|_| world.create_entity()).collect();
    let mut entities = vec![
        entity(ids[0], Some(ids[1])),
        entity(ids[1], Some(ids[2])),
        entity(ids[2], Some(ids[0])),
        entity(ids[3], Some(ids[1])),
        entity(ids[4], Some(ids[3])),
        entity(ids[5], Some(ids[5])),
        entity(ids[6], None),
    ];
    let parents: HashMap<_, _> = entities.iter().map(|e| (e.id, e.parent_id)).collect();
    let expanded: HashSet<_> = ids.iter().copied().collect();
    let expected: HashMap<_, _> = ids.into_iter().zip([3, 3, 3, 4, 5, 1, 0]).collect();
    for _ in 0..entities.len() {
        entities.rotate_left(1);
        let filtered: Vec<_> = entities.iter().collect();
        let rows = prepare_hierarchy_rows(&entities, &filtered, &expanded);
        for (entity, row) in filtered.iter().zip(rows) {
            assert_eq!(row.depth, expected[&entity.id]);
            assert_eq!(
                row.visible,
                is_entity_visible_fast(entity, &parents, &expanded)
            );
        }
    }
}

#[test]
fn test_collapsed_rows_still_build_and_search_reveals_selection_for_scrolling() {
    let mut world = World::new();
    let mut parent = None;
    let entities: Vec<_> = (0..40)
        .map(|_| {
            let id = world.create_entity();
            let next = entity(id, parent);
            parent = Some(id);
            next
        })
        .collect();
    for search_filter in ["", "row"] {
        let mut tree = ViewTree::new();
        tree.env_mut().set(HierarchyDrawCtx {
            bounds: Rect2D::from_origin_size(Vec2::ZERO, Vec2::new(200.0, 100.0)),
            entities: entities.clone(),
            hierarchy_state: HierarchyState::default(),
            theme: ColorScheme::default(),
            search_filter: search_filter.into(),
            selected_entity: parent,
        });
        tree.build_from(&HierarchyView);
        assert_eq!(
            tree.iter_nodes()
                .filter(|(_, node)| node.widget.as_any().is::<Selectable>())
                .count(),
            entities.len()
        );
        let root = tree.root().unwrap();
        let state = tree.state_arena_mut();
        state.reset_slots();
        state.get_or_create(root, String::new());
        let scroll_id = state.get_or_create(root, 0.0f32);
        let offset = state.get::<f32>(scroll_id).unwrap();
        if search_filter.is_empty() {
            assert_eq!(offset, 0.0);
        } else {
            assert!(offset > 0.0);
        }
    }
}
