# katla_audio

Standalone audio crate with no dependencies on other Katla crates.

## Architecture

The audio pipeline is: **File → Decoder → AudioBuffer → Mixer → Voice → Output Stream**

- **`AudioEngine`** — Entry point. Opens the default output device via `cpal`, owns the output stream and the `AudioMixer`. Created paused; call `resume()` to start playback.
- **`AudioMixer`** — Thread-safe mixer that owns active voices. Rendered on the audio callback thread (real-time priority). Uses `Arc` + atomics for lock-free voice control from the main thread.
- **`Voice`** — A single playing sound. Fixed-point resampling for pitch, linear interpolation, stereo panning. All per-voice parameters (volume, pan, pitch) are atomic for lock-free access.
- **`VoiceHandle`** — Main-thread handle to a voice. Wraps `Arc<Mixer>` + `VoiceId`. Safe to clone and hold.
- **`AudioBuffer`** — Decoded PCM data (f32 interleaved). Reference-counted via `Arc` for cheap sharing between voices.
- **`StreamingDecoder`** — Chunk-by-chunk decoder for long audio files (music). Reads WAV in blocks to avoid loading entire file into memory.

## Supported Formats

- **WAV** — via `hound` (PCM float and integer)
- **OGG Vorbis** — via `lewton`

Use `load_audio(path)` to auto-detect by extension, or `load_wav`/`load_ogg` directly.

## Audio Categories

Three sub-mix channels with independent volume controls:
- **SFX** — short sound effects
- **Music** — background music
- **Ambient** — environmental audio

Master volume controls the final output level. Category volumes are applied per-voice during mixing.

## Thread Safety

The audio callback runs on a real-time thread managed by `cpal`. All voice mutations (volume, pan, pitch, stop) use atomic operations — no locks on the audio thread. `VoiceHandle` can be freely shared on the main thread.

## Constraints

- This crate must NOT depend on any other Katla crate
- All output is stereo (mono sources are upmixed, multi-channel is mixed down)
- Sample format is always f32 internally
- Pitch uses fixed-point resampling (24-bit fractional part) for deterministic behavior
