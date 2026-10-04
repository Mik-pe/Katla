# Odin renderer shader families

The editor registers one aggregate family covering its four scene consumers,
models, lighting/shadows, postprocessing, particle compute/render programs,
overlays, UI and integer picking. A compile or native preparation failure keeps
all previously accepted handles and descriptor snapshots. Publication is a
single owner-thread operation after every candidate has prepared successfully.
A prepared scene frame, pending particle preparation or live overlay upload
returns Busy; the service retries at a later frame boundary.

The scene candidate receives eight selected-entry artifacts in this order:
Surface, Postprocess, Sky, Grid, Cull, ShadowPrimitives, ShadowModels, Model.
`shadow_primitives.wgsl` and `shadow_models.wgsl` include their distinct geometry
ABI declarations and the shared `shadow_geometry.wgsl` implementation. Particle
artifacts are Emit, Simulate, DrawCommand, DispatchCommand, Render. Overlay has
one vertex/fragment module. The watcher resolves transitive includes, so changing
`lighting_common.wgsl` or particle `common.wgsl` prepares the entire aggregate.

Candidates validate logical selected-entry bindings, inputs, outputs and compute
workgroups against the accepted ABI. Native binding slots and translated entry
names may change. Every candidate owns deep copies of selected metadata, strings,
MSL and SPIR-V; future scene uploads use the new accepted snapshots after the
watcher discards its temporary compile result. A shader replacement preserves
scene geometry, textures, particle data, emitter clocks, glyph textures and
per-slot frame buffers.

All forward/reverse raster variants prepare together; the independent forward
shadow projection remains unchanged. Publication updates future graph packet
references and invalidates compiled graph revisions. Accepted native recordings
retain their previous pipeline/resource owners, so removing old public parents
does not modify a queued color or picking readback. Previous or failed candidates
release their own partial native parents and compiled artifacts.

`odin/examples/editor_overlays` exercises actual native failure after earlier
candidate PSOs were created, Busy rejection, edited primitive/model pixels,
pending previous-frame readback, future native-consumer rebuild and exact shader
restoration on Metal and Vulkan. It also executes replaced particle indirect
billboards and alpha-masked overlay glyphs. Use the offline compiler CLI and
matching native dependency libraries with Metal API validation; combined ASan
runs preserve address checks while disabling only Apple framework process-exit
leak scanning. CPU tests retain leak scanning.
