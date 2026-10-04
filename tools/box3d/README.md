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
identities. Revision-three Body is 136 bytes and Pose is 48 bytes on the supported
64-bit targets; borrowed mesh pointer/count fields are copied before native use.
Body and world wrappers have explicit destructors; the dependency
library remains loaded until they have been released. Global world creation and
destruction are serialized because Box3D allocates world IDs globally. Separate
created worlds can step concurrently on their respective owner threads.

```sh
python3 scripts/validate_odin_box3d.py
```

This runs actual fall/contact, unchanged-body sync, directed sensor transitions,
filters, deletion exits, rotated box/capsule, actual triangle-floor/convex-hull
contact, whole-batch malformed-hull rollback, concurrent world lifecycle and
thread-affinity tests. App acceptance resolves a parented falling body, commits
its local transform and receives real enter/exit events. It repeats with both
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
ABI three rejects older libraries that would interpret kind five as a capsule.

Primitive boxes, spheres and Y capsules, filters, gravity, authored velocities,
CCD, density and material factors are supported. Static triangle meshes retain
actual Box3D mesh data until their native shape is destroyed. Convex hulls use
the actual native hull builder and reject inputs reaching its 255-point limit;
no truncated hull is substituted. Unsupported moving triangle meshes fail
admission because this Box3D revision only contacts static mesh shapes.
Authored geometry replacements are prepared before removing existing bodies and
preserve their completed native motion when pose/velocity were unchanged.
Native joints, heightfield colliders and spatial-query bindings remain separate
migration work. The established
Rapier backend remains the default. Backend selection is explicit and allowed
only while editing; missing libraries return failure without changing the
scene or selecting a substitute backend.
