> Historical Rust contract at [8b76a167](https://github.com/Mik-pe/Katla/blob/8b76a1670bf961db7afd2151545b5c845c413835/docs/metal-binding-residency.md). See the current document in docs.

# Metal 4 binding and residency ownership

Katla uses one descriptor binding ABI defined in `metal/binding_schema.rs`.
The same canonical resource map drives Naga's MSL generation and per-entry-point
reflection. Only resources reachable from that entry point contribute required
bindings. Buffer, texture, and sampler namespaces are independent. Graphics
profiles preserve descriptor-set mappings; compute-only graphics modules assign
flat slots in descriptor binding order. Specialized particle, UI, outline and skinned
shadow profiles supply their corresponding maps. Native vertex attributes use
buffer slot 10, outside WGSL descriptor sets. Naga runtime-array size metadata
uses the profile's sizes-buffer slot.

Reflection includes each buffer's minimum declared type size. CPU graphics and
compute preflight resolves actual native views, validates their range and live
required bindings, and checks native residency before creating the pass encoder.
Submission diagnostics serialize frame slot and generation, reflected table
layouts and actual retained native membership for each residency scope.

The bindless descriptor is an array of GPU texture resource IDs in shared
storage, referenced by a buffer entry of `MTL4ArgumentTable`. This indirection
supports 4096 textures while Metal 4 tables expose at most 128 texture entries.
It is not a separate stage-binding mechanism: shaders consume the ID array
through the argument table's canonical buffer entry.
Reflection validates this backing buffer against the shader's declared texture
array capacity using the native resource-ID stride, rather than treating the
opaque WGSL binding-array type as a zero-byte buffer.

A bindless manager publishes immutable snapshots. Every snapshot owns its GPU
ID array, generation and committed native residency set. Replacement, removal,
and CPU slot reuse produce another snapshot; command submissions retain the
snapshot they encoded, so previous GPU references cannot change in place.
Persistent resource mutations invalidate cached snapshots. Graph resources use
a separate transient update API; three cached immutable snapshots are keyed by
the exact sorted graph bindless-address/resource-ID pairs. Returning to an
unchanged frame slot reuses its existing array and native set. Sparse dirty-slot
bookkeeping clears only changed addresses. Publication after a real mutation
copies changed entries into a replacement array and builds residency once.
Encoder setup retains one snapshot and attaches one native set without
traversing registered textures.

Submission-local residency contains explicitly bound and graph-live allocations.
`MetalResidency` retains every member, includes each resource's owning heap,
deduplicates members, and counts heap bytes once. Commit seals membership.
Memoryless tile attachments have no native residency allocation and are excluded
from sets; Metal rejects their inclusion. They also cannot enter shader-visible
bindless tables. Missing allocations and additions after commit fail validation. The native set
and associated tables must remain owned until feedback reports the exact
submission complete; a previous-frame wait or fixed number of retired frames
cannot replace that contract.

Renderer resource managers separately own immutable persistent-buffer residency
snapshots for meshes and explicit imported buffers. Registration, destruction
and replacement build the next set before changing live resource records.
Command submissions retain the snapshot they used. Dynamic meshes replace
vertex and index allocations on every successful update, preserving capacities
when shrinking and growing geometrically, so CPU uploads cannot overwrite a
buffer still referenced by another submission.

Native tests exercise residency deduplication, missing resources, sealed
membership, heap ownership and physical-byte accounting, plus bindless streamed
replacement and slot reuse with retained in-flight snapshots. Run the setup
microbenchmark with:

```
cargo test -p katla_gfx --release --lib test_benchmark_encoder_setup_registered_texture_scaling -- --ignored --nocapture
```

The benchmark reports median and p95 CPU time for retaining an immutable
snapshot, verifying its native backing buffer and populating a native Metal 4
argument-table address. Cases register 1, 64, 512 and 4096 distinct textures.
It measures encoder binding setup; resource creation/publication and GPU work
are outside the timed region.

Measured on 2026-10-02 with Apple M5, 24 GiB unified memory, macOS 27.0,
optimized builds and launch-time Metal API validation. Each case contains 15
samples of 100,000 setup operations. Membership construction and GPU execution
are excluded; this is a native binding microbenchmark, not a frame-rate result.

| Registered textures | Median setup (ns) | p95 setup (ns) |
| ---: | ---: | ---: |
| 1 | 30.4 | 36.4 |
| 64 | 30.8 | 34.9 |
| 512 | 32.9 | 34.1 |
| 4096 | 31.4 | 32.7 |

The measured setup path has no linear registry term. End-to-end frame pacing,
publication throughput and supported `macos-26` CI remain separate validation.

Cached graph slot publication also remains independent of the registry size.
The same hardware and sampling protocol measured:

| Registered textures | Median slot publication (ns) | p95 (ns) |
| ---: | ---: | ---: |
| 1 | 43.4 | 84.2 |
| 64 | 37.4 | 65.3 |
| 512 | 39.1 | 54.8 |
| 4096 | 37.4 | 38.9 |

This timed region rotates three real Metal textures through one graph bindless
address and calls the production snapshot publication method. The registry,
three initial snapshots and native residency sets are populated before timing.
Run `test_benchmark_slot_publication_registered_texture_scaling` with the same
release/ignored/nocapture options to reproduce it.
