# Odin agent foundation

`odin/agent` builds on the existing Odin ECS/editor ownership. The remaining
complete crate migration is tracked in [TODO](../TODO.md#odin-port).

## Scene calls, context and admission


`decode_call` validates a borrowed `Tool_Call` and returns an owned `Decoded_Call`.
The operation's strings borrow its parsed JSON tree; destroy the decoded owner
once after execution or submission. `submit_call` copies the operation into
`editor.Agent_Harness` before destroying the decode tree. Call IDs belong to the
host adapter's correlation state; mailbox response IDs identify editor session
actions. The existing editor thread executes operations and owns undo history.
Host threads never receive a World pointer. Supply the mailbox a thread-safe
allocator and join host threads before its destruction.

The supported tool names are `spawn_entity`, `destroy_entity`, `duplicate_entity`,
`set_field`, `query_entities`, `list_available_components`, `add_component`,
`remove_component` and `get_component_attributes`. All reject unknown fields.
Entity IDs are decimal strings, including IDs larger than JavaScript's exact
integer range. Overflow, signs, whitespace and numeric JSON IDs are rejected.
Spawns accept `name` and finite three-element `position`, `rotation` and `scale`
arrays; scale defaults to one. These are the existing editor's registered CPU
component fields, not mesh creation. Query limits default to 256 and must be
1–256 when supplied. Duplicate offsets, shapes, model loading, parenting and
application-owned animation/material/behavior/resource operations remain outside
this subset and are not silently ignored.

`scene_context` captures sorted registered component counts and optional selected
component JSON on the editor thread. A stale generational selection yields no
selected data. The snapshot owns its arrays and JSON; its type names borrow the
registry, which must remain alive. It includes the registered serialized state;
it is not a public/private host filtering boundary. Adapter-specific data policy
must precede transporting this observation.

`Rate_Limiter` owns a mutex and preallocated rolling minute of admitted timestamps.
Pass monotonic nonnegative elapsed time to `rate_admit`; backwards clocks fail
without changing admission history. Only `Allowed` records a call. `Wait` and
`Exceeded` return the remaining duration; a delayed caller must retry the same
atomic admission operation, rather than recording unconditionally after a sleep.
This deliberately avoids the Rust bridge's delayed-caller admission race.
Maximum count is at least one and interval at least zero. Keep a shared limiter
at a stable address and join callers before destruction.

The runnable consumer submits JSON on a real host thread, joins it, verifies the
World remained untouched, ticks the mailbox on its owner thread, checks the
result/selection and undoes the scene mutation while preserving a pre-existing entity:

```sh
odin run odin/examples/agent_scene -out:target/odin-agent-scene -vet -strict-style
odin test odin/agent -all-packages -out:target/odin-agent-tests -vet -strict-style
```

MCP transport, LLM HTTP/streaming/configuration, conversational orchestration,
asset/resource tools, typed application requests and their native app consumers
still need migration. This package opens no network or provider connection.

## Validation

The five new agent tests and 34 existing ECS/editor tests pass strict vet/style
checks, native debug AddressSanitizer and optimized execution with allocator leak
tracking. The runnable agent consumer also typechecks with strict checks for
`linux_amd64`. The Rust reference passes all 125 all-feature agent tests and
strict all-target check/Clippy. Provider and native application acceptance remain
pending.
