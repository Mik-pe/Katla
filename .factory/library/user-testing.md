# User Testing

Testing surface, required testing skills/tools, and resource cost classification.

## Validation Surface

This mission involves a native Vulkan desktop application with both automated tests and manual editor verification.

**Automated testing (no GPU):**
- `cargo test --workspace` — ECS unit tests, scene serialization unit tests
- `cargo clippy --workspace -- -D warnings` — linting
- `cargo fmt --check` — formatting

**Automated testing (GPU required):**
- `cargo run -- -s` — single-frame mode (25 frames, headless validation, exit code 0/1)

**Manual testing (Vulkan GPU + display):**
- `cargo run` — full editor for visual verification of inspector editing, minimize/restore, Ctrl+S

## Validation Concurrency

Max concurrent validators: 1

Only one Vulkan instance can run at a time (exclusive GPU access). Machine: 8 GB RAM, 4 cores/8 threads, ~2.5 GB free. Vulkan app uses ~500-800 MB.

## Testing Tools

- `tuistory` — for automated TUI interaction (keyboard shortcuts, menu navigation)
- `cargo run -- -s` — headless GPU validation pattern (like `particle_validation` example)
- Manual `cargo run` — visual editor verification

## Flow Validator Guidance: katla-ecs-unit-tests

Surface: katla_ecs unit tests (no GPU, no services)

**Test name mapping:** The validation contract references test names that differ from actual function names in `katla_ecs/src/world.rs`. When mapping assertions to tests, use the actual function names:
- VAL-ECS-017: `test_query_changed_is_subset` (not `test_query_changed_is_subset_of_query`)
- VAL-ECS-018: `test_immutable_get_no_change` (not `test_immutable_get_no_change_detection`)
- VAL-ECS-019: `test_query_changed_multi_component` (not `test_query_changed_multi_component_union`)
- VAL-ECS-020: `test_destroyed_entity_excluded` (not `test_destroyed_entity_excluded_from_query_changed`)

**Performance assertions (VAL-ECS-025, VAL-ECS-026):** These use absolute time thresholds (<500ms for 100K creates, <200ms for 100K mut accesses) rather than direct percentage overhead measurement, since events/change detection are always enabled with no disabled baseline.

## Flow Validator Guidance: scene-serialization-tests

Surface: katla_app scene serialization tests (no GPU, no services)

**Test name mapping:** Scene serialization assertions map to tests in `katla_app/src/scene/mod.rs` and `katla_app/src/scene/migration.rs`:
- VAL-SCENE-001: `test_primitive_round_trip`, `test_all_entity_source_variants_round_trip`
- VAL-SCENE-002: `test_gltf_round_trip`, `test_gltf_entity_with_animation_round_trip`
- VAL-SCENE-003: `test_point_light_round_trip`, `test_point_light_descriptor_round_trip`
- VAL-SCENE-004: `test_particle_emitter_round_trip`, `test_particle_emitter_descriptor_round_trip`
- VAL-SCENE-005: `test_animation_round_trip`, `test_gltf_entity_with_animation_round_trip`
- VAL-SCENE-006: `test_hierarchy_preservation`, `test_parent_child_relationships_round_trip`
- VAL-SCENE-007: `test_entity_count_preservation`
- VAL-SCENE-008: `test_unknown_fields_ignored`, `test_forward_compatibility_unknown_struct_fields`, `test_forward_compatibility_new_entity_source_variant`
- VAL-SCENE-009: `test_version_field_present`, `test_scene_version_defaults`
- VAL-SCENE-010: `test_migration_runs_on_mismatch`
- VAL-SCENE-011: `test_migration_preserves_data`
- VAL-SCENE-012: `test_v1_v2_migration_stub`, `test_migration_bumps_version`
- VAL-SCENE-013: `test_default_scene_round_trip`, `test_build_default_scene_round_trip`, `test_full_default_scene_like_serialization`
- VAL-SCENE-014: `test_empty_scene`
- VAL-SCENE-015: `test_velocity_round_trip`
- VAL-SCENE-016: `test_default_scene_matches_disk`, `test_regenerate_default_scene`
- VAL-SCENE-017: `test_metadata_preservation`
- VAL-SCENE-018: `test_load_spawn_integration`
- VAL-CROSS-005: `test_gltf_entity_with_animation_round_trip`, `test_animation_round_trip`
- VAL-CROSS-006: `test_parent_child_relationships_round_trip`, `test_hierarchy_preservation`
- VAL-CROSS-009: `test_forward_compatibility_error`, `test_forward_compatibility_no_panic`

**Isolation:** All scene serialization tests are pure unit tests (RON round-trips, no GPU needed). They can run concurrently without conflict.

## Resource Notes

- GPU resource tests (VAL-GPU-*) are integration tests requiring Vulkan
- ECS tests (VAL-ECS-*) are pure unit tests, no GPU needed
- Scene serialization unit tests (VAL-SCENE-001 through VAL-SCENE-017) are pure RON round-trips
- Scene integration test (VAL-SCENE-018) requires headless GPU
