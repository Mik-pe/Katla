# ADR: Native Box3D physics ownership

**Status:** Accepted for the canonical Odin engine.

**Decision:** Use the source-pinned Box3D C17 dependency through a checked C ABI;
keep scene authoring, hierarchy, lifecycle and script policy in Odin.

The earlier Rapier/Rust implementation is superseded. The operating contract is
[odin/physics/box3d](../odin/physics/box3d), with application composition in
[odin/app/physics.odin](../odin/app/physics.odin). The exact source pin, native
adaptations, ABI and detailed acceptance are maintained in
[the Box3D dependency contract](../tools/box3d/README.md).

## Decision and boundaries

Box3D owns bodies, shapes, contacts, constraints, broad/narrow-phase queries,
CCD, sleeping and solver integration. The adapter owns native handles and copied
geometry. Application code uses full generational entity identities and typed
body/joint/query descriptions; no ECS or Odin scene pointer crosses into C.
The canonical app initializes Box3D explicitly. Missing libraries, ABI mismatch
and unsupported/invalid inputs fail without selecting another engine or
publishing fabricated simulation results.

The app preflights whole body/joint batches and resolves parent hierarchy before
native mutation. Prepared native replacements publish before old owners retire;
rejection destroys staged owners and preserves live bodies, completed motion,
constraints and awake states. Unchanged synchronization keeps actual native
handles and dynamics. Pose publication validates every target, then stages all
parent-local results before writing ECS components. Fixed and kinematic bodies
retain authored local transforms; dynamic publication uses the supported positive
TRS hierarchy contract.

## Geometry, material and motion

Scene descriptions cover box, sphere, Y-capsule, trimesh, convex hull, heightfield
and body-only motion. Full affine residual deformation is baked into mesh/hull
vertices relative to the rigid body pose, including reflected winding and
hierarchy-induced shear. Height arrays are independently owned through component
cloning, history and Play/Stop. Native geometry remains alive until all dependent
shapes release it.

Fixed terrain uses native mesh/heightfield owners. Moving concave mesh/terrain
uses exact two-sided planar triangle hulls on the same body; gaps remain open.
This is neither extrusion nor one bounding hull. Closed moving meshes obtain
mass, center and inertia from authored volume and density. Open surfaces and
body-only descriptions do not gain invented mass. Native velocity, gravity,
friction, restitution, reciprocal layer/mask filters, density and CCD remain
explicit authored properties.

Point-to-point, hinge, distance and fixed constraints use actual native spherical,
revolute, distance-spring and weld joints. Admission validates endpoints and
finite factors before mutation. Periodic hinge intervals preserve authored limits,
including intervals crossing the principal-angle boundary. Distance springs use
the exact finite signed midpoint, including zero/sub-slop rest lengths, with
native effective-mass conversion. The bounded source adaptations preserve the
pinned upstream checkout; details and tests belong to the dependency contract.

## Queries, contacts and trigger feedback

Ray casts, shape casts and sorted overlaps run against actual native geometry.
Results preserve complete u64 IDs, filter/sensor behavior and documented
initial-overlap semantics. Completed contact snapshots copy native points,
normals and impulses; convex wire overlays consume actual native hull edges.
The graphics renderer must not infer contact normals from overlap membership.

Sensor transitions are directed from trigger to visitor. Multiple triangle
pieces deduplicate by entity; sensor pairs produce both directions. Deletion
produces exits for surviving owners. The app routes completed feedback through
scene-owned trigger rules and direct Luau events. Physics contains no particle,
audio, animation, script or editor policy.

Play/Pause/Resume/Stop is application-owned. Stop restores owned authored body,
joint and heightfield descriptors with fresh entity generations and mapped
references, rather than serializing native handles or treating simulated state
as durable authoring history. Documents persist source identities and typed
geometry/joint properties.

## Build and validate

```sh
python3 scripts/build_katla_odin.py --tests
python3 scripts/validate_odin_box3d.py
python3 scripts/validate_odin_luau.py
```

The Box3D validator builds actual pinned normal/ASan dependencies, executes
native rollback tests, Odin dependency tests and application physics consumers,
then checks portable targets. Use `build_box3d.py --output` for an isolated
matching native artifact; ASan requires the same LLVM major as Odin. Current
ABI8 libraries reject earlier revisions before constructing an owner.

Acceptance must include real contact and motion, concave gaps, CCD, native
queries, failed publication preserving live state, joint ranges, lossless IDs,
owned geometry cleanup and Play/Stop restoration. The native driver supports
Darwin/Linux/Windows builds; cross-target compilation on another host is not
native hardware execution there. [Canonical build instructions](odin_build.md)
and [the native dependency contract](../tools/box3d/README.md) record the exact
commands and remaining platform evidence.
