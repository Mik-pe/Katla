//! Opt-in CPU-only benchmark of the actual hierarchy view's descriptor build.
//! Include this unchanged from the baseline hierarchy module for comparisons.

use super::*;
use katla_ecs::World;
use katla_math::{Vec2, Vec3};
use katla_ui::declarative::ViewTree;
use std::{
    hint::black_box,
    time::{Duration, Instant},
};

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
#[ignore = "opt-in CPU hierarchy benchmark; run with --ignored --nocapture"]
fn test_benchmark_hierarchy_build() {
    let budget = Duration::from_millis(
        std::env::var("KATLA_BENCH_SAMPLE_MS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(50),
    );
    println!("scenario,entities,threads,matches,sample,iterations,ns_per_iteration,checksum");
    for size in [128, 1024, 4096] {
        for (name, chain, trailing_roots, expand, search) in [
            ("hierarchy_flat", false, false, false, ""),
            ("hierarchy_chain_expanded", true, false, true, ""),
            ("hierarchy_chain_collapsed", true, false, false, ""),
            ("hierarchy_chain_search", true, false, false, "row"),
            ("hierarchy_deep_then_roots", true, true, true, ""),
        ] {
            let mut world = World::new();
            let mut parent = None;
            let chain_length = if trailing_roots { size / 2 } else { size };
            let mut entities: Vec<_> = (0..size)
                .map(|index| {
                    let id = world.create_entity();
                    let next = entity(
                        id,
                        if chain && index < chain_length {
                            parent
                        } else {
                            None
                        },
                    );
                    parent = Some(id);
                    next
                })
                .collect();
            if trailing_roots {
                entities[..chain_length].reverse();
            }
            let mut hierarchy_state = HierarchyState::default();
            if expand {
                hierarchy_state.expanded_entities = entities.iter().map(|e| e.id).collect();
            }
            let mut tree = ViewTree::new();
            tree.env_mut().set(HierarchyDrawCtx {
                bounds: Rect2D::from_origin_size(Vec2::ZERO, Vec2::new(320.0, 800.0)),
                entities,
                hierarchy_state,
                theme: ColorScheme::default(),
                search_filter: search.into(),
                selected_entity: None,
            });
            for _ in 0..3 {
                tree.build_from(&HierarchyView);
            }
            let expected_nodes = tree.iter_nodes().count();
            assert!(expected_nodes >= size);
            for sample in 0..7 {
                let start = Instant::now();
                let mut iterations = 0;
                let mut checksum = 0.0;
                loop {
                    black_box(&mut tree).build_from(black_box(&HierarchyView));
                    checksum += black_box(tree.iter_nodes().count()) as f64;
                    iterations += 1;
                    if start.elapsed() >= budget {
                        break;
                    }
                }
                let ns = start.elapsed().as_secs_f64() * 1e9 / iterations as f64;
                assert_eq!(checksum, (iterations * expected_nodes) as f64);
                println!("{name},{size},1,{size},{sample},{iterations},{ns:.3},{checksum:.6}");
            }
        }
    }
}
