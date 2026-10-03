# Odin leaf packages: icons and audio DSP

The next standalone packages after ECS and math live in `odin/icons` and
`odin/audio/dsp`. Neither depends on a renderer, ECS or Katla math. Their scope
is complete icon data and the audio effect layer; they do not provide an Odin
audio engine. [The shared Odin tree](../odin/README.md) is the entry point.

## Icons

The package exports all 132 Rust `ForkAwesome` constants as named `rune` values,
the `ALL_ICONS` catalogue (name and codepoint), and the 39-entry `COMMON_ICONS`
precache array in reference order. Consumers import it under a namespace:

```odin
import icons "path/to/odin/icons"

play_icon := icons.PLAY
common := icons.COMMON_ICONS
// A font cache can consume common[:] while the local array remains alive.
```

Values are Unicode private-use codepoints. UTF-8 encoding is independent of
font rendering; a UI consumer must supply the matching icon font. This port
does not package a font atlas or claim that glyphs have been rendered.
Odin's constant arrays are values; copy a catalogue into a variable for runtime
indexing or slicing. `ALL_ICONS` enables enumeration without reflection or a
parallel manually maintained Rust fixture.

`scripts/check_icon_port.py` builds and executes the Odin consumer, then compares
every name/value pair, catalogue order and precache entry against the Rust
exports. Native tests check the private-use range, uniqueness, catalogue membership
and real UTF-8 encode/decode round trips.

## Audio DSP

| Rust responsibility | Odin API |
| --- | --- |
| Low/high-pass biquad, Q=0.707, separate channel history, live cutoff | `Biquad`, `biquad_init`, `biquad_set_cutoff`, `biquad_clear`, `biquad_process` |
| Four comb + two allpass delays per side, sample-rate scaling, wet/decay/dampening | `Reverb`, `reverb_init/destroy/clear`, `reverb_set_wet/decay/dampening`, `reverb_process` |
| Atomically controlled, block-smoothed zone reverb | `Zone_Targets`, `zone_targets_set`, `Zone_Reverb`, `zone_reverb_init/destroy/process` |
| Ordered effect chain | Borrowed `Effect` callback/state pairs, `Effect_Chain`, `chain_add/process`, `effect_biquad/reverb/zone_reverb` |
| Per-voice aux accumulation, effect processing and return | `Aux_Bus`, `aux_bus_init/destroy/prepare/accumulate/process/mix_into` |
| Equal-power panning and amplitude conversion | `compute_pan_gains`, `db_to_linear`, `linear_to_db` |

The processing API accepts interleaved `[]f32` buffers containing complete mono
or stereo frames. An empty valid block succeeds; a partial frame or other channel
count returns `Invalid_Block` before changing samples or history. This is the
mono/stereo engine contract rather than Rust's incidental multi-channel fallbacks.

```odin
import dsp "path/to/odin/audio/dsp"

filter: dsp.Biquad
err := dsp.biquad_init(&filter, .Low_Pass, 800, 48000)
assert(err == .None)

reverb: dsp.Reverb
assert(dsp.reverb_init(&reverb, 48000) == .None)
defer dsp.reverb_destroy(&reverb)

chain: dsp.Effect_Chain
assert(dsp.chain_add(&chain, dsp.effect_biquad(&filter)) == .None)
assert(dsp.chain_add(&chain, dsp.effect_reverb(&reverb)) == .None)
samples: [256]f32
samples[0] = 1
assert(dsp.chain_process(&chain, samples[:], 2) == .None)
```

Reverb owns its twelve delay allocations and retains their originating allocator.
Aux buses own scratch storage allocated to a maximum sample capacity at init.
Destroy each owner exactly once; destroy functions also accept zero/destroyed
values. Do not shallow-copy owners and then process or free both copies. Borrowed
effect states must remain valid at stable addresses until their chains stop using
them. Destruction frees buffer memory, not borrowed effect state.

One processing thread owns histories, chains and buses. Initialize, configure the
chain and allocate bus capacity before real-time processing. The built-in effects,
bus prepare/accumulate/process/mix and parameter setters allocate nothing during
processing. A chain contains at most 16 effects; overflow and oversized aux blocks
return errors rather than resizing on the audio thread. Custom callbacks must
honor the same allocation/thread constraints. A callback failure leaves earlier
effects applied; malformed block validation happens before any effect runs.

The bus's default send level is used by its caller when a voice has no override;
`aux_bus_accumulate` accepts the already selected effective level, matching the
Rust mixer. Accumulation and return require matching buffer sizes. Levels must be
finite and nonnegative; params and buffer fields are implementation state and
must not be mutated concurrently. Sample buffers should contain finite PCM values.

Zone targets use separate relaxed atomic float-bit controls. Use
`zone_targets_set` from the control thread; never read/write its bit fields as
ordinary memory while processing. Three parameter fields are not a transactional
snapshot, matching the Rust design. Targets must outlive their zone reverb and
all threads that publish parameters. The processor smooths by 0.08 once per
nonempty block; its response therefore depends on block cadence. An effectively
inactive zone outputs silence and does not advance delay history, as in Rust.

Cutoff must be finite, positive and strictly below Nyquist, and sample rate must
be positive. Reverb delay lines have a minimum length of one sample at very low
rates, preventing the Rust implementation's zero-sized delay indexing failure.
Finite wet/decay/dampening values are clamped; NaN/infinity is rejected before
mutation. Zone targets are clamped before smoothing. These input checks,
complete-frame validation, preallocation and explicit ownership are intentional
Odin API changes. Allocation exhaustion during initialization follows the Odin
runtime allocator behavior; processing offers no growth fallback.

## Validation and current boundary

From the repository root:

```sh
python3 scripts/validate_odin.py
odin test odin/audio/dsp -out:target/odin-dsp-asan -sanitize:address -debug -vet -strict-style
odin test odin/audio/dsp -out:target/odin-dsp-release -o:speed -vet -strict-style
python3 scripts/compare_audio_dsp.py --report docs/benchmarks/audio-odin-parity.json
cargo test -p katla_icons -p katla_audio --locked
cargo check -p katla_icons -p katla_audio --all-targets --locked
cargo clippy -p katla_icons -p katla_audio --all-targets --locked -- -D warnings
```

The two icon and ten DSP tests cover DC filter response, stereo isolation, chunked
history continuity, reverb tail/reset, low-rate delays, invalid-input immutability,
effect order/capacity, aux summing/return, zone silence/smoothing, real-thread
atomic publication, gain conversion and owner cleanup. A tracking allocator
verifies zero allocations over 64 full filter/reverb/aux processing blocks and
zero remaining owner allocations after cleanup. Native AddressSanitizer and
optimized tests pass on arm64; x86 `linux_amd64` typechecking also passes.

The offline consumer matches Rust at 44.1, 48 and 96 kHz in mono and stereo,
including live cutoff and reverb parameter changes, ordered chains, aux sends and
gain helpers. Each dev/release profile compares 960 DSP blocks plus 41 gain rows:
1,001 records and 92,283 scalar values. Numeric differences were zero at the
printed precision (nine decimal places in scientific notation) on this host. Tolerances remain 2e-5 absolute and
5e-5 relative. [The receipt](benchmarks/audio-odin-parity.json) records versions,
source and output hashes and both results. Zone reverb is private in the Rust
crate and is validated by the independent Odin tests rather than the public
Rust consumer. Rust audio/icons pass 43 library tests and one audio doctest;
two icon doctests are ignored, and strict check/Clippy pass.

`odin/audio` currently contains DSP only. PCM buffer loading, WAV/OGG/MP3/FLAC
codecs and metadata, voice/resampling/pooling, full category mixer, audio clock,
scheduling/cues/streaming and native device output still belong to the Rust
engine. No audio device is opened during these tests, and no audible playback,
callback deadline, thread-priority or hardware-output acceptance is claimed.
See [remaining Odin work](../TODO.md#odin-port) before composing an audio engine.
