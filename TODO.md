# TODO

## Gizmo

### UX

- [ ] Add plane-drag support (e.g., XY, XZ, YZ planes) for translate and scale modes
- [ ] Calibrate scale sensitivity to screen-space movement (magic 0.01 constant is not zoom-aware)

## Outline + Overlay

### Code Quality

- [ ] Move wallhack overlay logic out of tonemapping shader — `tonemapping.wgsl` has outline-specific `stencil_indicator` sampling baked in; extract to a separate fullscreen overlay pass or make it data-driven

### Refactoring

- [ ] Remove `Option<PipelineHandle>` wrappers in `OutlineState` — all fields are always initialized during `init_outline_pipelines`, use a once-cell pattern or init-time struct
- [ ] Share the empty descriptor set layout for skinned outline pipelines as a renderer-level resource instead of burying creation in `init_outline_pipelines`
