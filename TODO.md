# TODO

## Gizmo

### Bugs

- [x] Create cone mesh for translate gizmo tips instead of reusing cube (init_gizmo_resources creates a cube as tip_mesh, but translate gizmo expects cones)
- [x] Fix rotation delta calculation: use gizmo origin screen position as center instead of previous mouse position (works by accident currently, breaks when mouse is at previous position)
- [x] Fix scale mode plane anchor: uses start_origin for ray-plane hit but returns absolute scale factor, causing jumps instead of relative accumulation from drag start
- [x] Make rotation sign conventions consistent: Y-axis uses identity while X/Z negate, causing inconsistent rotation direction
- [x] Fix rotate gizmo hit testing: uses line-segment distance like translate/scale, but rotate gizmo is a torus ring that needs point-to-circle distance

### UX

- [x] Gate W/E/R gizmo shortcuts on keyboard capture state to prevent mode switches while typing in inspector fields
- [ ] Add plane-drag support (e.g., XY, XZ, YZ planes) for translate and scale modes
- [ ] Calibrate scale sensitivity to screen-space movement (magic 0.01 constant is not zoom-aware)

### Code Quality

- [x] Remove dead `pending_mode_change` field from GizmoState (never read or written)
- [x] Remove unused variables: `_tip_center` in generate_translate_draw_calls, `_axis_dir` in generate_rotate_draw_calls
- [x] Use proper near/far clip planes in screen_to_ray instead of z=1.0 and z=0.5
- [x] Deduplicate fallback path in compute_translate_delta and compute_scale_delta when axis is parallel to camera forward
