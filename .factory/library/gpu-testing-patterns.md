# GPU Testing Patterns

## DuplicateContext Pattern for Editor Actions

When implementing editor actions that need unit testing but operate on `Application` (which holds GPU-heavy fields like `Window`, `VulkanRenderer`), extract a narrow context struct holding only the `&mut` references needed:

```rust
struct DuplicateContext<'a> {
    world: &'a mut World,
    particle_system: &'a mut Option<GlobalParticleSystem>,
    gpu_resource_tracker: &'a mut GpuResourceTracker,
    selected_entity: &'a mut Option<EntityId>,
}
```

This allows testing with `particle_system: &mut None` and `gpu_resource_tracker: &mut GpuResourceTracker::new()` without needing a real GPU.

## destroy_emitter() Config Zeroing Side Effect

`GlobalParticleSystem::destroy_emitter()` zeroes the `emit_rate` in the `EmitterConfig` slot and pushes the index to `free_slots`. This means between `destroy_emitter()` and `create_emitter()`, `recompute_estimated_max_alive()` will see zeroed configs.

The editor action's destroy/reset/recreate sequence handles this correctly because `create_emitter()` restores configs and calls `recompute_estimated_max_alive()` again. But any code calling `reset_all()` independently should be aware that `free_slots` may retain stale entries after destroy cycles.

## reset_all() Does Not Clear free_slots

`GlobalParticleSystem::reset_all()` resets counters, emitter states, and reinitializes GPU buffers, but does NOT clear `emitter_pool.free_slots`. After a destroy/reset/recreate cycle (as in the editor action), `free_slots` retain stale entries from `destroy_emitter()`. This is harmless because `create_emitter()` pops from `free_slots`, but if `reset_all()` were called independently, `free_slots` could accumulate across multiple reset cycles.
