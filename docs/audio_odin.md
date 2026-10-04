# Odin audio ownership and editor integration

The canonical audio implementation is `odin/audio`, with its existing tested
`dsp` package. It has no ECS, math, renderer or application dependency. Device
output and codecs use the repository-pinned direct C dependency in
[`tools/audio_native`](../tools/audio_native/README.md). No Rust audio bridge is
used. Application ownership lives in `odin/app/audio_*.odin`.

## Implemented feature contracts

| Former Rust feature | Canonical Odin consumer |
| --- | --- |
| WAV/Ogg Vorbis/MP3/FLAC PCM and metadata | `clip_decode`, `metadata`, actual encoded bytes and exact decoder frame counts |
| f32/i16 construction | immutable `clip_from_pcm` / `clip_from_i16` |
| incremental decoder, chunks, seek and exhaustion | owned `Decoder`, `decoder_read`, `decoder_read_chunk`, `decoder_seek` |
| default native output, pause/resume and recovery | paused `engine_create`, real callbacks, three bounded recovery attempts, device enumeration/reopen/poll |
| PCM pool and priority stealing | 64 voices; only strictly lower priority may be stolen; generational stale handles cannot affect replacements |
| streaming pool and ring decoding | eight workers, bounded four-second source rings, decoding outside native callbacks, seek supersedes queued PCM, explicit underrun counters |
| pitch, pan, sample-rate conversion | 24 fractional bits, Catmull-Rom interpolation, stereo constant-power pan, mono upmix and multichannel average downmix |
| start/stop and loop transitions | three millisecond fades; up to 256 source frames of equal-power overlap |
| volume/pan/pitch tween and occlusion | block smoothing, one-pole per-voice occlusion filters |
| master and SFX/Music/Ambient mixer | validated settings, category/master peak and RMS, active and peak voice counts, caller-owned voice-list snapshots |
| master/auxiliary effect chains and reverb zones | existing Odin DSP callbacks, 16 bounded aux buses, owned engine zone reverb, externally owned effects remain borrowed until engine destruction |
| sample clock and scheduled play/stop/volume | bounded 256-event queue, sample-frame clock, first block at/after scheduled time, explicit asynchronous failure counts and error latch |
| randomized/sequential/shuffled sound cues | retained clip variants, seeded PRNG, nonrepeating shuffle cycles, semitone/decibel variations |
| spatial emitter/listener and zone composition | app world transforms, three distance models, listener basis pan, Doppler and actual prepared native physics ray occlusion |
| scripts, editor preview and settings | one ECS-owned `Audio_Runtime`, confined script sound/cue consumers, transactional preview selection, shared live settings/meter/device snapshot |
| source/emitter/listener/zone persistence | registered owned component codecs, rooted scene/prefab DTOs, staged source validation and deep string/history ownership |

The mixer renders bounded 512-frame blocks using preallocated scratch. It performs
no allocation, freeing or decoding on the native callback. A short engine mutex
serializes bounded rendering with main-thread control operations; each streaming
worker copies ring blocks under its separate mutex after decoding outside it.
`engine_collect` releases completed owners on the application thread. Destroying
the engine first joins native callbacks, then decoder workers, then clips and DSP
storage. A `Clip` owner must retain/release explicitly; public handles borrow the
stationary engine and become invalid when it is destroyed.

The source/clip cache is bounded to 256 paths and 64 million PCM samples in total.
`audio_service_reload` publishes a newly decoded source only after validation;
existing voices retain their previous immutable revision. Failed preview source
loading and failed native-device replacement preserve the previous accepted owner.
Decoder source/PCM limits return explicit errors rather than claiming a successful
load. Source bytes are bounded to 64 MiB; a fully decoded clip is bounded to 64
million samples. Stream rings are bounded to four source seconds, capped at one
million frames per voice. Nonempty source paths always pass through retained
`Asset_Roots` and `resources.read_bytes`; codecs receive bytes and cannot bypass
path/symlink confinement. Script paths accept resource-relative paths and the
retained resource root's project-relative prefix.

An empty AudioSource/AudioEmitter is an explicitly unconfigured, inactive authored
descriptor so the inspector can Add and then choose an asset. Source metadata
returns `Unconfigured`; live state reports the unconfigured emitter count. Scene
DTOs use a null path for this intentional state. Malformed nonempty paths still
fail. This state creates no fake clip or voice. Scene audio follows each emitter's
`playing` field independently of simulation mode, matching the original editor
frame loop. Reset stops old entity/script voices while the independent editor
preview has its own explicit stop action.

## Reproducible validation

Run `python3 scripts/validate_odin_audio.py --native --switch-default` for the CPU,
ASan and actual hardware checks. CPU suites keep LSan enabled and fail owned
allocation leaks; macOS suppresses only the observed `CFPrefsPlistSource` and `CFPrefsSearchListSource`
initialization stacks on the external XPC thread.

Native device shutdown has a separate memory boundary. Actual CoreAudio runs
reported 17,304 bytes in 13 allocations belonging to Apple NSXPC autorelease/TSD
and `HALC_ShellObject::PropertiesChanged` initialization, with no audio bridge,
miniaudio or Odin frames in those stacks. To repeat native address/callback
validation with that explicit OS driver boundary, pass
`--native-asan-leaks external-driver`. This disables leak checking only for the
native device process, prints the boundary and leaves every CPU suite at
`detect_leaks=1`. The default `check` policy retains native leak checking and
reports those external allocations too. Native address validation is not a
claim that the OS device process exits without external lifetime allocations. The default-device switch option is macOS only;
it temporarily selects an aggregate backed by the current speakers and restores
and removes it through cleanup. It must run where changing the default output is
appropriate. The default native run verifies real output, pause, failed replacement
rollback and same-device reopen without changing the system default.

The checked-in tiny fixtures are generated sine PCM, Ogg Vorbis, MP3 and FLAC,
not silence placeholders. CPU tests check decoded samples, exact metadata,
resampling/pan/fades, priority/stale handles, release ownership, meters/effects,
clock/events, cue order/variation, streaming loops/seeks/end tails and one-frame loops, rejected nonfinite PCM, rooted scene
DTOs and preview/deleted-entity/script lifecycles. Native physics acceptance uses
the actual Box3D ray query when its library is explicitly supplied.

On the development Apple Silicon host, normal and ASan runs delivered nonzero PCM
through actual CoreAudio callbacks on the MacBook speakers. Pause stopped
callbacks; all four codecs delivered nonzero PCM with both retained clip and worker-ring playback after independent stop intervals. Failed replacement preserved the prior stream; reopen preserved voices.
A real default-output switch to the temporary aggregate and back triggered native
notifications and Odin replacement, with nonzero callbacks after restoration.
This verifies device/callback delivery, not an acoustic microphone recording or
a human listening judgment. Linux/Windows typechecks pass; output hardware on
those platforms has not been exercised here. The UI owner must still verify the
actual mixer/inspector/preview journey in the complete native editor; library and
app service tests do not establish that UI acceptance.
