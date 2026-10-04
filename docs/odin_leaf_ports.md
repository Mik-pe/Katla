# Odin leaf packages: icons and audio DSP

The independent packages `odin/icons` and `odin/audio/dsp` provide editor icon
data and audio effects. The complete [audio engine](audio_odin.md) owns codecs,
streams, voices, native device output and the editor mixer.

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

Native tests check the private-use range, uniqueness, catalogue membership and
real UTF-8 encode/decode round trips. The actual retained UI and editor overlay
fixtures shape and render the matching ForkAwesome glyphs through the native
font pipeline.

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

## Validation

```sh
odin test odin/icons -vet -strict-style -define:ODIN_TEST_FAIL_ON_BAD_MEMORY=true
odin test odin/audio/dsp -out:target/odin-dsp-asan -sanitize:address -debug -vet -strict-style
python3 scripts/validate_odin_audio.py
```

DSP tests cover filter response, stereo isolation, chunked history, reverb
reset, low-rate delays, invalid-input immutability, effect ordering/capacity,
aux sends/return, zone smoothing and real-thread atomic controls. Allocation
tracking verifies no allocation during processing and no remaining owned memory.
Native clip/stream/device acceptance is described in [audio](audio_odin.md).

[The historical migration receipt](benchmarks/audio-odin-parity.json) retains
source hashes and paired dev/release outputs at 44.1, 48 and 96 kHz: 1,001 rows
and 92,283 scalar values per profile. The retired Rust reference is historical
evidence; current checks execute the Odin DSP and actual native audio consumers.
