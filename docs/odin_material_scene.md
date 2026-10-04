# Authored material and imported primitive ownership

The application owns `Surface_Material`, the five named `Material_Sampling`
roles and the owned paths in `Texture_Assignments`. GPU handles and pipeline
owners remain in `app/render`. The material tool, Inspector and `.katmat`
actors use these same components and one restoration participant.

Base-color RGB is transported as sRGB and stored in linear space; its alpha is
linear. Emissive RGB is linear and may be HDR. Normal scale is any finite signed
value, occlusion strength is in `[0,1]`, and alpha cutoff is finite and
nonnegative. Coverage mode and double-sided culling are independent of tint
alpha. `has_surface` and `has_sampling` distinguish authored policies from the
selected source's original settings. `has_factors` selects exact authored
base-color/metallic/roughness factors, so an imported zero factor can be edited
without division. Generated primitive children and durable drawable DTOs carry
their effective factors explicitly.

UV coordinates select set 0 or 1 and apply scale, rotation in radians, then
offset. Zero and negative scales are valid. An image choice is `Inherit`,
`Neutral`, a confined image file, or an explicit glTF image index. Its source
identity remains independent of its sampler and UV transform. Application image
snapshots retain the exact accepted decoded revision through Undo and Play/Stop;
portable documents contain source descriptors, not GPU handles or image pixels.

Every material batch preflights all targets and values, prepares one native
candidate, then publishes once. A rejected candidate preserves every component
and history entry. Undo/Redo changes the material component while retaining live
unrelated transforms, animation clocks and scheduled particles. Inspector drags
retain the first accepted value and final accepted value as one history action.

## Imported scenes

Model insertion produces a `GltfGroup` controller and one `GltfPrimitive` child
per active node/mesh primitive. Selectors retain the original global node index
and the primitive ordinal within that node's mesh. Children have independent
material choices and authored transforms; the imported node matrix remains in
the rendering source, preserving shear and reflection. Animation playback is
owned by the source controller and inherited through its parent hierarchy.
The nearest explicit player wins, including an intermediate controller or a
primitive override; an empty group does not block an outer ancestor's player.

All participants share a reference-counted immutable CPU model revision. One
preparation transaction reads a source once even when a document contains many
selectors. Captured snapshots retain that revision; restoring a captured scene
does not reread changed or removed source files. File DTO loading resolves a new
confined revision before publication. Historical `GltfModel` document inputs
expand into this hierarchy during staging, and added children receive fresh
scene keys before baseline publication. Explicit Group/Primitive documents keep
their authored identities and hierarchy.

Group bounds merge current child bounds. Group mesh colliders instead use the
source's genuine aggregate bind-pose triangles, including skinning, morph
weights and imported affine node matrices. A selected primitive collider uses
only that selector. The shared physics collector then bakes authored hierarchy
scale/shear/reflection around the native rigid pose; no bounds approximation is
used.

## Removing a collider

Setting `PhysicsBody.has_collider` to false retires the owned shape, collision
material/filter presence, sensor/trigger state and constraints referencing that
collider in one authored action. The rigid body, velocity and gravity remain.
Undo restores exact dimensions or height samples, material/filter values,
trigger data and constraints. Reattaching a collider with no retained shape
creates a valid unit box and preserves independently authored material/filter
metadata. Native preparation failures roll back the complete proposal.

CPU ownership, durable scene roundtrips, native Box3D contacts and exact
restoration are covered by the application tests. Rendering evidence belongs to
the native material/model validators on each backend; typechecking alone does
not establish GPU coverage, picking or shadow behavior.
