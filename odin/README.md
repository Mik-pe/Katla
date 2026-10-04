# Katla in Odin

This directory is the shared home for the progressive Odin port. Work continues
on `port/odin`, with package boundaries following the engine's ownership model.
Each completed package can be imported and exercised without a renderer.

| Package | Responsibility | Contract |
| --- | --- | --- |
| `ecs` | Entity identities, sparse storage, queries, resources, events, commands, scheduling | [ECS port](../docs/ecs_odin.md) |
| `editor` | Optional reflection, JSON, reversible scene actions and agent mailbox | [ECS/editor port](../docs/ecs_odin.md) |
| `agent` | Validated JSON scene calls, selected context and synchronized admission | [Agent foundation](../docs/agent_odin.md) |
| `examples/agent_scene` | Actual host-thread JSON → editor tick → observation → undo | `odin run odin/examples/agent_scene` |
| `math` | Vectors, matrices, quaternion rotation, transforms, bounds, intersections and color | [Math port](../docs/math_odin.md) |
| `icons` | All Katla icon codepoints and precache data | [Leaf ports](../docs/odin_leaf_ports.md) |
| `audio/dsp` | Filters, reverb, zone controls, effect chains and aux processing | [Audio DSP](../docs/odin_leaf_ports.md#audio-dsp) |
| `examples/movement` | Runnable ECS + math composition | `odin run odin/examples/movement` |
| `workload`, `compile_workload`, `editor_workload` | Consumers for the recorded ECS compilation comparison | [Measurements](../docs/ecs_odin.md#validation-and-compilation-measurements) |
| `math_reference` | Numeric consumer paired with Rust for migration validation | `python3 scripts/compare_math_port.py` |
| `icon_reference`, `audio_reference` | Compiled icon catalogue and offline DSP consumers paired with Rust | [Leaf validation](../docs/odin_leaf_ports.md#validation-and-current-boundary) |

Use imports relative to the consuming package, for example:

```odin
import ecs "../../ecs"
import km "../../math"
```

Odin's directory packages and relative imports are documented in the
[language overview](https://odin-lang.org/docs/overview/#packages).
There is no package manager or third-party runtime dependency in these ports;
they use the compiler's `core` packages. The math package's declaration is
`katla_math` to avoid colliding with `core:math`; consumers can alias it to `km`.

From the repository root:

```sh
python3 scripts/validate_odin.py
```

This runs ECS/editor, agent, math, icon and audio DSP tests with strict vet/style checks,
an actual ECS/math consumer, icon parity and Rust/Odin math/DSP comparisons in dev and release. It needs
Odin, Python 3 and Rust/Cargo. See the math contract for native sanitizer commands.

## Continuing the migration

Add each next engine responsibility as a package under `odin/`. Keep math and
ECS, icons and audio DSP independent; optional editor tooling can import ECS; application composition
imports the packages it needs. A future GPU core must retain its generic resource
and execution responsibility rather than depending on ECS, math or editor policy.

Port real consumers alongside their packages and test behavior before switching
an application subsystem. Keep deliberate contract changes documented: the math
port intentionally unifies quaternion/matrix/TRS rotation and omits Euler APIs.
GPU paths will need native output and lifetime acceptance on each target backend;
the current CPU packages do not establish rendering acceptance.

The Rust engine remains runnable while this directory grows. It supplies reference
behavior and measurement baselines until a complete Odin application can replace
it. There is no FFI bridge or automatic language switch. Once a subsystem's consumers
move, remove its superseded implementation and update imports and documentation
together rather than maintaining two production paths.
