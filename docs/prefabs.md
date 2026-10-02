# Mesh recipes and prefabs

Katla separates reusable static geometry from scene composition:

| Asset | Purpose | Runtime result |
| --- | --- | --- |
| `.katmesh`, version 1 | Named geometry parts and baked local transforms | One PBR vertex/index stream, bounds and shared CPU collider geometry |
| `.katprefab`, version 1 | One rooted scene v3 subtree | Fresh editable entities, materials and component references |
| `.katla`, version 3 | A complete authored scene | Expanded prefab instances with persistent scene keys and mesh asset references |

These services belong to `katla_app`; the graphics core receives ordinary mesh
resources. Animated/skinned assets continue through the GLTF pipeline.

## Build an object

Put parts that share one material and lifecycle in the same mesh recipe. The
chair frame example combines a seat, back and four legs into one indexed mesh
(144 vertices, 72 triangles). Split the cushion into another mesh entity so its
material can vary independently. A prefab contains an empty root and those two
children, with local placement, drawable PBR settings and an optional mesh
collider on the frame. Four chairs need two shared chair mesh uploads and eight
geometry draws; this is mesh sharing, not automatic draw batching.

Open [chair.katprefab](../resources/prefabs/chair.katprefab) and the
[frame recipe](../resources/meshes/chair-frame.katmesh) for complete files.
[Prefab workshop](../assets/scenes/prefab-workshop.katla) places four expanded
instances with different cushion colors. Double-click a `.katmesh` or
`.katprefab` in the asset browser to instantiate it at the origin and select its
root. Focus fits the subtree, including roots without a drawable. Transform
inspection edits local placement; rendering, culling and spatial AI queries use
current world matrices.

## Mesh contract

A recipe contains `version`, `name` and `parts`. Each part has a unique `id`,
`transform` and `geometry`. Geometry uses a strict `kind` tag: `cube`, `sphere`,
`plane`, `cylinder`, `cone`, `torus` or `triangles`. The `prefab` tool's `describe`
operation supplies examples of the actual JSON schemas. On disk the same data
uses RON, with fixed-size arrays written as tuples:

```ron
(
    version: 1,
    name: "Seat",
    parts: [(
        id: "seat",
        transform: (position: (0.0, 0.45, 0.0)),
        geometry: (kind: "cube", size: (0.8, 0.1, 0.8)),
    )],
)
```

Indexed triangles contain XYZ `positions`, CCW `indices`, optional XYZ
`normals` and optional UV `uvs`. Attribute counts must match positions; indices
must form valid triangles. Missing normals are area weighted. Missing UVs use
zero UVs and an orthogonal tangent. UV seams and hard edges require duplicated
vertices. Degenerate triangles and undefined generated normals are rejected.
Part transforms bake into positions, inverse-transpose normals and orthogonalized
tangents. Reflections reverse winding and tangent handedness.

Files are limited to 64 MiB, recipes to 1,024 parts, combined output to 1,000,000
vertices and 6,000,000 indices. Names and IDs are bounded; values must be finite,
scales nonzero, primitive dimensions positive and tessellation within the scene
validator's bounds. Geometry/part transform magnitudes are limited to 1,000,000.
Validation budgets allocations before primitive generation. Saving compiles the
recipe before atomically replacing the file.

The active geometry cache uses the complete canonical geometry/transform
recipe, excluding authoring names and part IDs. Identical geometry from different
paths shares a generational GPU handle and retained CPU geometry. Each drawable
owns a tracked reference. The final release removes GPU geometry and both CPU
cache entries, so a later instantiation uploads a fresh handle. Materials remain
per entity. Combining parts does not weld, union, simplify or create LODs.

## Prefab and persistence contract

A prefab contains `version`, a `root` scene key and a `scene` in the current
[scene schema](../katla_app/src/scene/README.md). Exactly one root has no parent;
its transform must be identity. Every other entity descends from it. Placement
is supplied when instantiating. Child ordering and labels have no identity role.

Built-in `Resource`, `Scene` and `File` references resolve from the prefab's own
origin. Capture and scene Save As rebase paths against their destination.
Instantiation allocates fresh runtime IDs and globally unused scene keys, then
remaps parents, joints, trigger targets and registered custom component DTOs.
Preparation uses the scene loader's staging and rollback path; a failed upload,
component decode or capture baseline leaves existing entities and resources
intact. Capturing a live subtree discards its external parent and root placement.
References outside that subtree fail capture.

Custom prefab components require a registered scene codec. Entity references
must use `SceneWriteContext::id` / `SceneReadContext::entity`; opaque payloads
cannot safely be instantiated. Ordinary scene files retain their existing
unknown-component round-trip behavior.

Instantiating expands the template into editable scene entities. Saving the scene
preserves child edits, mesh references, physics, materials and internal references.
Reload reconstructs the meshes from their files. There is no implicit prefab
inheritance, nested template reference or live override synchronization. Writing
a new asset revision leaves live instances intact; instantiate another preview
or reload the scene to read the revised mesh. Template changes apply when creating
new instances. The explicit `remove` operation cleans up a preview subtree and its
resources; prefab insertion/removal is not part of the scene tool's undo stack.

Exact hierarchy matrices preserve nonuniform scale and shear for rendering and
bounds. Mesh colliders bake the affine deformation into CPU vertices at spawn and
use a rigid world pose in Rapier. Kinematic and static poses follow root movement;
dynamic results convert back to parent-local position/rotation without replacing
local scale. Collider dimensions/deformation are prepared at body creation;
primitive collider dimensions are authored separately. Changing scale while a
body is live requires recreating that body. Prefer mesh colliders for geometry
that must follow a scaled recipe exactly, and `ConvexHull` for dynamic bodies.

## AI iteration

MCP and the in-editor co-creator expose the same `prefab` operations and engine
validation. Tool paths are project-relative `.katmesh` / `.katprefab` paths;
traversal and symlink escapes are rejected. Entity IDs are full decimal strings.

1. Call `{"action":"describe"}` or `{"action":"read","path":"resources/prefabs/chair.katprefab"}`.
2. Edit named mesh parts or prefab children in the returned JSON. Call `validate`
   with `path` and the complete `document`; mesh feedback reports bounds,
   vertices, triangles and one draw per mesh entity.
3. Call `write` with the same arguments. Write referenced meshes first, then the
   prefab. Malformed geometry, missing assets and unregistered component codecs
   return errors before replacing files.
4. Call `{"action":"instantiate","path":"resources/prefabs/chair.katprefab","position":[2,0,0]}`.
   Rotation defaults to identity XYZW; scale defaults to `[1,1,1]`. Retain the
   returned `root_entity` and `entities`.
5. Use `editor_view` focus/observe and scene queries to inspect the rendered
   result. Remove the previous preview with `{"action":"remove","root_entity":"..."}`
   before creating another revision.
6. Export an edited live subtree using `{"action":"capture","path":"resources/prefabs/custom-chair.katprefab","root_entity":"..."}`.
   Save the containing scene to persist placement and instance edits.

The Rust API mirrors this flow with `MeshAsset::{load,compile,save}`,
`Prefab::{load,instantiate,capture,save}` and `prefab::{instantiate_asset,remove_instance}`.

## Verification

Portable tests cover schemas, budgets, normal/tangent transforms, reflected
winding, invalid hierarchy and fixture consistency. Ignored native prefab tests
run with API validation and cover AI write/read, shared uploads, scene save/reload,
resource retirement, failed staging rollback, custom/joint/trigger remapping and
scaled mesh collider reconstruction:

```bash
cargo test -p katla_app --lib --all-features --locked mesh_asset
cargo test -p katla_app --lib --all-features --locked prefab
RUST_LOG=info cargo test -p katla_app --lib prefab::native_tests --all-features --locked -- --ignored --test-threads=1
```

Vulkan and capability-gated Metal CI run the shared native fixture. Native
rendering acceptance also loads the workshop through the ordinary PBR graph;
use API validation and inspect all four placements, not only successful loading.
