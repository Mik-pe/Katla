# Odin pipeline preparation and caching

The application prepares pipelines before publication or frame acquisition.
`odin/gfx/shader` resolves WGSL and includes into an owned immutable source
snapshot, hashes compiler identity and options, and invokes the isolated offline
compiler only when the content-addressed artifact is missing or invalid. The
runtime loads generated MSL/SPIR-V and complete per-entry reflection. Build and
runtime use the same source and artifact contract; see [shader contracts](gfx_shader_odin.md).

Each backend deduplicates immutable graphics pipeline descriptors. Identity includes
shader bytes and entries, reflection/layout, vertex formats and strides, attachment
formats, blend/write state, depth state, rasterization and winding. Cache entries
retain weak native identities: public handles and accepted frame slots own actual
native lifetimes. Distinct sampler policies have independent immutable identities.
No registered state can be silently reused for a different descriptor.

Reload prepares all existing model/surface coverage, mirrored/two-sided and shadow
variants from one source revision. Publication swaps the entire accepted family;
compile or native preparation failure retains every previously accepted pipeline.
Submitted slots retain the version they encoded until actual completion.

Persistent shader artifacts and process-local native pipeline deduplication are the
Odin cache contract. The former Rust Metal archive format and its timing records
are [historical implementation evidence](archive/rust-metal_pipeline_cache.md).
Those measurements do not describe Odin launch latency.
