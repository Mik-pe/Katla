# Backend-independent Odin render graph

`odin/gfx` owns one declaration, compiler and prepared execution contract for both
native backends. `Graph` owns logical resources and copied typed packets;
`Compiled_Graph` owns exact live pass order and buffer/image range hazards. The
application composes scene, particle, shadow, postprocessing, picking and UI passes.
Neither compiler nor native allocation ownership depends on ECS or editor policy.

Native preflight validates reflected bindings, resources, ranges and every packet
before encoding. `Prepared_Graph` joins logical resources with accepted physical
handles and explicit alias handoffs. Allocation groups use actual backend-reported
requirements and disjoint live intervals. Metal and Vulkan translate the same
prepared execution into their own command model and native synchronization scopes.

Each `Frame_Token` contains its stationary owner, slot and acquisition generation.
A returned `Submission` identifies accepted queue work. In-flight resources survive
public handle removal until exact submission completion. Failed preparation or
recording publishes neither partial scene state nor an accepted capture.

See [gfx contracts](gfx_odin.md), [native backend contracts](metal_backend.md)
and [capture diagnostics](render_graph_capture.md). The former Rust proposal is
preserved in [the archive](archive/rust-backend_agnostic_render_graph.md).
