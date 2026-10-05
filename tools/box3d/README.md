# Box3D dependency

Katla's canonical Odin physics owner uses the actual C17 Box3D backend. The
dependency is Box3D v0.1.0, pinned to
`8441b4a06d6d09dcfb0b0f704df4d847d1437b92` from
[the upstream repository](https://github.com/erincatto/box3d).
`tools/build/native.odin` verifies the exact clean checkout and compiles the source
list declared by upstream, with the reproducible planar-hull extension below,
native validation and SIMD enabled. Dependency
source and build output remain in `target/`; upstream uses the MIT license.

`bridge.c` contains layout-stable dependency accessors. Odin owns the retained
body map, whole-batch preflight, deterministic event ordering, application
hierarchy conversion and pose publication. No ECS, app or renderer pointer
crosses into C. The ABI uses explicit single-precision fields and lossless u64
identities. Revision-eight Body is 168 bytes, Pose is 48 bytes, Joint is 64 bytes, Contact
is 48 bytes and Edge is 24 bytes on supported 64-bit targets. Borrowed mesh
and row-major height streams are copied into native geometry and independently
retained by Odin before caller storage is released.
Body, joint and world wrappers have explicit destructors; the dependency
library remains loaded until they have been released. Global world creation and
destruction are serialized because Box3D allocates world IDs globally. Separate
created worlds can step concurrently on their respective owner threads.

```sh
odin run tools/build -- validate physics
```

This runs actual fall/contact, unchanged-body sync, directed sensor transitions,
filters, deletion exits, rotated box/capsule, actual triangle-floor/convex-hull
contact, whole-batch malformed-hull rollback, concurrent world lifecycle,
thread-affinity and actual spherical, revolute, distance-spring and weld tests.
It checks hinge axes/limits, physical spring stiffness across different masses,
native-handle preservation, partial joint-publication rollback, completed angular
motion across owner replacement, and native heap restoration after reset/destroy.
App acceptance resolves a parented falling body, commits its local transform,
receives real enter/exit events, and runs all four joint variants through Play/Stop
with fresh entity references. It repeats with both
the C dependency and Odin instrumented by AddressSanitizer. The ASan dependency
compiler must match Odin's LLVM runtime; the script resolves installed Clang
of that major version, or accepts an explicit `CC`. Portable Linux/Windows Odin
checks confirm type checking; native dependency execution was proven on macOS
arm64. The build driver supports Darwin, Linux and Windows. Windows requires
Clang in a Visual Studio/Windows SDK developer environment and emits an exported
DLL; native Windows execution is not proven on the macOS host.

Body-only descriptions use shape kind `None = 5`: Box3D creates an actual native
body with no shape or fabricated mass. This dependency gives shapeless bodies
zero inverse mass, so gravity does not accelerate them; authored velocity and
pose remain native body state. Collider removal/addition preserves completed
motion, and ignored collider metadata never creates geometry or sensor overlap.
ABI eight loads all 22 required symbols and rejects earlier libraries before
creating any dependency owner.

Primitive boxes, spheres and Y capsules, filters, gravity, authored velocities,
CCD, density and material factors are supported. Static triangle meshes retain
actual Box3D mesh data until their native shape is destroyed. Convex hulls use
the actual native hull builder and reject inputs reaching its 255-point limit;
no truncated hull is substituted. Dynamic and kinematic triangle meshes retain
exact two-sided triangle hulls on the same native body. Concave gaps remain open
in contact, sensor, ray and shape queries. These are zero-volume surfaces, not
extruded prisms or a bounding convex hull. Closed mesh mass, center and inertia
come from signed tetrahedron integration at the authored density; density edits
update actual native mass. Open planar meshes and heightfields retain zero mass
and their authored native velocity, without fabricated replacement mass.
Authored geometry replacements are prepared before removing existing bodies and
preserve their completed native position, rotation, linear and angular motion
when authored pose/velocity were unchanged.

Point-to-point, Y-axis hinge, distance and fixed descriptions use actual Box3D
spherical, revolute, distance-spring and weld constraints. As in the scene
contract, endpoints require dynamic or kinematic bodies with colliders. Distance
uses authored limits' midpoint as rest length (0.5 without limits), stiffness 1
and damping 0.5. The adapter converts these physical coefficients to Box3D's
frequency and damping ratio using native effective mass, including world inverse
inertia and the anchor lever arms, and refreshes the conversion before each step.
It never adds mass or integrates a replacement force outside the dependency.
All finite ordered hinge limits describe a periodic angular interval. Intervals
narrower than a full turn rotate the native reference frame to their center and
constrain the principal angle to half their width on either side. This supports
intervals crossing π and intervals entirely outside ±π without requiring a
multi-turn counter. Widths of at least 2π leave rotation free. Authored values
remain unchanged. Double-precision center/width arithmetic avoids overflow for
finite f32 endpoints. The native angular axis and both local anchors remain exact.

`tools/build/box_adaptations.odin` generates bounded source adaptations at build time:
it removes upstream's ±0.99π angular clamps and minimum distance rest length.
The pinned dependency checkout stays clean. Distance springs use the exact finite
signed midpoint, including zero and values below 0.005, without changing their
physical stiffness or damping. Midpoints use double arithmetic to avoid overflow.
The native regression reads back exact configured rest lengths and the Odin
solver test reaches zero/sub-slop equilibria with sleeping explicitly prevented
by a zero-force wake command. Normal simulation retains native sleep behavior.
Heightfields and exact native spatial queries are supported as described below.

Whole-body and joint admission precedes native mutation. New native body owners
and constraints are prepared and published before old owners are retired; unchanged
descriptions retain their native handles. Failed partial publication destroys
all new constraints without waking their participants, then restores their prior
awake states after every staged connection is detached. This preserves an awake
neighbor's completed velocity when another participant was originally asleep;
the native C regression checks both cleanup orders. Successful swaps retire
constraints before their body owners.
Reset and destruction follow the same constraint-before-body order.

The application initializes the canonical Box3D owner through
`physics_select_box3d` while editing. Missing or incompatible libraries return
failure without changing the scene or selecting a substitute engine. The
superseded Rust/Rapier scene bridge is not an application runtime dependency.

The native query ABI returns exact u64 entity identities and closest-hit points,
normals and distances from Box3D geometry. Rays include sensors by default;
callers may explicitly exclude them. Category/mask filters are reciprocal, and
solid initial overlaps return distance zero using native GJK shape overlap.
Velocity, force and impulse operations apply to actual native bodies on their
owner thread. Application descriptors refresh authoritative velocity after a
command so subsequent synchronization preserves impulses. Trigger queries
copy sorted directed overlap pairs from the last completed native step.


Heightfield scene descriptions retain `rows`, `cols` and row-major `heights` in
an owned `PhysicsBody` component. `physics_heightfield` clones a borrowed source
stream for transfer into `physics_body`; a constructed body's height stream is
transferred into the ECS just like other owned component values. Snapshot,
undo and Play/Stop clone the stream through the component's registered value
operations. Durable scene encoding preserves the Rust `Heightfield` variant.
The whole grid spans `cols` by `rows` local units, centered on the authored
origin; exact positive TRS hierarchy scale is applied before native creation.
The native dependency uses positive grid coordinates. The bridge offsets its
native body by the rotated half-grid and converts published poses back, without
moving the authored entity. The dependency stores heights with 16-bit
quantization over the grid's minimum/maximum range; source values stay unchanged.
The maximum height error is the range divided by 65535, multiplied by Y scale.
Dynamic and kinematic grids instead retain exact two-sided cell triangles on
one native body. Their centered local vertices use the same diagonal as the
source terrain, without native height quantization or a body-origin offset.
Native contact solving, GJK and CCD handle the triangles. Sensor feedback
aggregates all pieces into one directed pair per visitor. Authored transform and
filter changes apply to every piece; complete geometry replacement retains the
previous owner until all candidate native shapes have been admitted.

`Physics_Query_Shape` borrows local geometry for the duration of a query, with
explicit world origin, normalized rotation and positive scale. Box, sphere,
Y capsule, convex hull, triangle mesh and heightfield queries use actual native
GJK overlap/cast operations. Concave query geometry is visited as actual
triangles; convex hulls exceeding Box3D's 64-point query limit are exactly
partitioned into native-hull face tetrahedra around its centroid. They are never
truncated or substituted by bounding boxes. Results use reciprocal filters,
explicit sensor inclusion, solid initial overlap and deterministic ID tie breaks.
`physics_shape_cast` preserves the original shape-cast parameter convention:
`point = origin + direction * distance`, with `distance <= max_distance`.
Direction is not normalized for shape casts; ray casts retain their existing
normalized world-distance contract. Overlap queries return sorted unique IDs.
Query geometry, returned contacts and returned IDs never retain ECS/native
pointers. Caller-owned output slices must be deleted with their captured app
allocator; borrowed input is retained only by the synchronous call.

`physics_contacts` copies every point from actual completed touching manifolds,
with canonical `a < b` entity IDs, the unit normal from A to B, the midpoint of
native world solver witnesses, separation and final-substep normal impulse.
Mesh/heightfield contacts may contain multiple manifolds and points. Sensors do
not invent contact normals. Native manifold pointers are copied immediately and
never escape the bridge. Contact and query operations enforce the native world
owner thread and release all temporary native geometry on failure or success.

ABI-seven acceptance adds asymmetric, rotated and centered heightfield ray casts,
actual falling-body contact, unchanged-owner preservation, source replacement,
whole-batch invalid-shape rollback and zero net native allocation after
destroy. Queries prove primitive hits, nearest sensors, masks, initial overlaps,
concave gaps and an 80-vertex hull containing a target. Contact normal, solver
point, penetration and impulse are asserted against the native heightfield.
The app tests durable ownership/roundtrip, invalid cardinality/overflow values,
scaled parent hierarchy, original cast parameter semantics and genuine Play/Stop
restoration. Normal and matching-LLVM C/Odin AddressSanitizer runs pass all
25 dependency tests and the selected nine app cases. Dependency-global native
heap assertions run with `ODIN_TEST_THREADS=1`; concurrent app tests must not
compare a process-wide counter while other worlds are alive.

`tools/build/box_adaptations.odin` generates one adapted `hull.c` next to each isolated artifact,
while the pinned upstream checkout remains clean. The extension admits only
the exact three-vertex, six-half-edge, two-face zero-volume triangle topology,
preserves native twin/face validation, and provides exact triangle ray tests
instead of the ordinary hull-plane test. All other hull validation remains
unchanged. Native shapes own their cloned hull data; no triangle stack pointer
escapes creation. An 80-vertex convex collider exposes its actual 120 native
wire edges through `physics_collider_edges`, rather than source mesh faces.

Moving-geometry acceptance proves dynamic heightfield motion and contact,
closed-mesh mass/density through real impulses, closed-mesh falling contact,
kinematic concave gaps and an AABB-interior ray that misses the actual triangle.
A CCD sphere at 200 m/s stops on a moving heightfield, and two triangle sensors
produce one enter/exit pair per visitor. The app runs dynamic terrain and a
closed mesh through physical contact and Play/Stop with independent restored
height streams, geometry, density and fresh entity generations.
