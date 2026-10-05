# Odin editor overlays

The application prepares world-space triangles for translation, rotation and scale gizmos, collider diagnostics, completed contact points, reverb volumes and light/particle billboards. Gizmo shafts, tips, scale cubes, plane handles and rotation rings use the same triangles for CPU ray hover. Their projected scale is 80 pixels under the current camera, including independent perspective, front, side and top views. World/local orientation comes from the shell's explicit orthonormal basis. Hover and capture change the actual uploaded color. Entity identity has an explicit presence flag: the initial valid `Entity_Id(0)` is preserved.

Collider diagnostics consume the same authored hierarchy and scaled shape descriptions as native physics. Box edges, sphere rings, capsule caps, exact triangle meshes and heightfield cells are real geometry. Convex hull edges come from the selected Box3D world's actual hull topology and completed transform. The host calls `physics_prepare` once at its frame boundary before collecting four diagnostic views; the renderer never synchronizes or steps physics. Contact markers and normal segments come from `physics_contacts` completed solver witnesses. Editing before any completed step legitimately has no contacts. Bodies without a collider produce no collider geometry.

Reverb boxes use the complete world transform and authored extent, decay and wet values. Meshless point lights and particle emitters use real ForkAwesome lightbulb/fire glyphs. Native font bitmap memory is copied immediately, before the next raster operation. Each glyph fits a transparent 64-pixel texture with a margin; its billboard is camera-facing and 40 pixels across. The textures remain immutable after initialization. An optional authored `Scene_Billboard` extension overrides the icon, sRGB tint and size, or attaches an icon to another scene entity; its RGB is decoded before HDR blending.

`overlay_mesh_prepare` owns CPU vertices and triangle identity records. `overlay_native_append` appends one ordinary HDR render pass before the scene's final tone phase, loading the scene color and depth attachments. Diagnostic geometry and icons compare depth; gizmo phases use `Always` and do not write depth. Linear colors and coverage blend into RGBA16F before the existing tone operator and single display transfer. No feature dispatch or hidden renderer operation belongs in the GPU core.

The host resets one `Overlay_Graph` when it truncates/rebuilds its combined graph. Four views share the imported glyph roles without giving the same physical texture overlapping logical aliases. Every view receives a separate immutable vertex upload. `Overlay_Frame` cleanup removes its public vertex/identity parents after acceptance or abort; accepted native recordings retain their own owners. A stationary overlay owner rejects destruction while CPU frame owners remain outstanding.

`overlay_picking_draws` converts genuine billboard ranges into the existing masked integer picking pipeline. It reuses the visible glyph texture, UV and per-vertex alpha with the same effective-alpha 0.01 cutoff. Existing encoded scene identities are retained; additional meshless entities receive new codes associated with complete generational IDs. The upload's world positions use an explicit identity object matrix. Transparent icon corners retain background/object IDs, rather than selecting the entire billboard quad. Gizmo hover is handled by exact CPU triangles; it does not masquerade as a captured entity ID.

## Native acceptance

`odin/examples/editor_overlays` runs both actual Metal and Vulkan adapters under validation. It checks four distinct cameras, all three gizmo modes, highlight changes, gizmos inside an occluding cube, collider and reverb geometry, a real completed Box3D contact and alpha-tested light/fire IDs. Public uploads and glyph/pipeline parents are removed while accepted work owns them. Its tracker must return to zero and Vulkan validation must report zero errors.

Build with the explicit source-built image library when selecting ASan dependencies:

```sh
odin build odin/examples/editor_overlays -vet -strict-style -sanitize:address \
  -out:/tmp/katla-editor-overlays-asan
```

The executable takes the offline shader compiler, Vulkan loader, pinned font library and coherent Box3D library, in that order. Run from the project root with `MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1` and the explicit Vulkan ICD. ASan native runs use a matching ASan font/Box3D build. The GPU process disables only process-exit leak scanning for Apple framework state; CPU render tests also run with `ASAN_OPTIONS=detect_leaks=1` and no suppressions.

The same executable additionally proves explicit camera depth on genuine scene/model/particle packets. Forward remains the editor default; reverse selects upfront Greater/GreaterEqual variants, including integer picking, while always-visible gizmos and independent forward shadow cascades retain their own contracts. The uploaded mesh must match the supplied view-projection and clip-Y values. Three frames forward → reverse → forward preserve exact picked entity IDs and restore every forward color byte. Native planar samples reject patterned shadow acne and expanded selection shells inside visible faces without removing real cast shadows.
