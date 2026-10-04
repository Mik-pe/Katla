# Box3D dependency

Katla's Odin scene can explicitly select the actual C17 Box3D backend. The
dependency is Box3D v0.1.0, pinned to
`8441b4a06d6d09dcfb0b0f704df4d847d1437b92` from
[the upstream repository](https://github.com/erincatto/box3d).
`scripts/build_box3d.py` verifies the exact clean checkout and compiles the source
list declared by upstream, with native validation and SIMD enabled. Dependency
source and build output remain in `target/`; upstream uses the MIT license.

`bridge.c` contains layout-stable dependency accessors. Odin owns the retained
body map, whole-batch preflight, deterministic event ordering, application
hierarchy conversion and pose publication. No ECS, app or renderer pointer
crosses into C. The ABI uses explicit single-precision fields and lossless u64
identities. Revision-four Body is 136 bytes, Pose is 48 bytes and Joint is 64 bytes on the supported
64-bit targets; borrowed mesh pointer/count fields are copied before native use.
Body, joint and world wrappers have explicit destructors; the dependency
library remains loaded until they have been released. Global world creation and
destruction are serialized because Box3D allocates world IDs globally. Separate
created worlds can step concurrently on their respective owner threads.

```sh
python3 scripts/validate_odin_box3d.py
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
arm64. The build driver supports Darwin and Linux; Windows needs a native C17
dependency build before runtime acceptance can be claimed.

Body-only descriptions use shape kind `None = 5`: Box3D creates an actual native
body with no shape or fabricated mass. This dependency gives shapeless bodies
zero inverse mass, so gravity does not accelerate them; authored velocity and
pose remain native body state. Collider removal/addition preserves completed
motion, and ignored collider metadata never creates geometry or sensor overlap.
ABI four rejects earlier libraries before creating any dependency owner.

Primitive boxes, spheres and Y capsules, filters, gravity, authored velocities,
CCD, density and material factors are supported. Static triangle meshes retain
actual Box3D mesh data until their native shape is destroyed. Convex hulls use
the actual native hull builder and reject inputs reaching its 255-point limit;
no truncated hull is substituted. Unsupported moving triangle meshes fail
admission because this Box3D revision only contacts static mesh shapes.
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
Box3D v0.1.0 admits hinge bounds only within `[-0.99π, 0.99π]` and distance rest
lengths at least `0.005`. The broader scene descriptions remain valid for Rapier;
Box3D returns `Unsupported` before altering any bodies instead of accepting
upstream's silent clamping. Heightfield colliders and spatial-query bindings
remain unresolved migration work.

Whole-body and joint admission precedes native mutation. New native body owners
and constraints are prepared and published before old owners are retired; unchanged
descriptions retain their native handles. Failed partial publication destroys
all new constraints without waking their participants, then restores their prior
awake states after every staged connection is detached. This preserves an awake
neighbor's completed velocity when another participant was originally asleep;
the native C regression checks both cleanup orders. Successful swaps retire
constraints before their body owners.
Reset and destruction follow the same constraint-before-body order.

The established
Rapier backend remains the default. Backend selection is explicit and allowed
only while editing; missing libraries return failure without changing the
scene or selecting a substitute backend.
