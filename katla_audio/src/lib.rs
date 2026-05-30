//! # katla_audio — Real-time audio playback engine
//!
//! Standalone audio crate with no dependencies on other Katla crates. Provides low-latency
//! audio playback via [`AudioEngine`], with support for one-shot sounds, streaming audio,
//! per-voice effects, and category-based volume control.
//!
//! # Quick start
//!
//! ```no_run
//! use std::path::Path;
//! use std::sync::Arc;
//! use katla_audio::{AudioEngine, load_audio};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let engine = AudioEngine::new()?;
//!     let buffer = load_audio(Path::new("sound.wav"))?;
//!     let voice = engine.play(&Arc::new(buffer));
//!     engine.resume()?;
//!     Ok(())
//! }
//! ```
//!
//! # Thread architecture
//!
//! There are two threads involved:
//!
//! 1. **Main thread** — your game/application code. You call [`AudioEngine`] and
//!    [`VoiceHandle`] methods here.
//! 2. **Audio thread** — a real-time callback managed by cpal. The mixer's
//!    [`AudioMixer::render()`](mixer::AudioMixer) function runs here periodically
//!    (typically every ~2–10 ms) to fill the output buffer.
//!
//! The audio thread has strict constraints: it must complete within the buffer deadline
//! or the user will hear glitches. All public APIs are designed so that **main-thread
//! callers never need to worry about this**, provided the rules below are followed.
//!
//! # What is safe to call from the main thread
//!
//! All methods on [`AudioEngine`] and [`VoiceHandle`] / [`StreamingVoiceHandle`] are
//! designed for main-thread use:
//!
//! - **Playback**: [`AudioEngine::play()`], [`AudioEngine::play_looping()`],
//!   [`AudioEngine::play_streaming()`]
//! - **Voice control**: [`VoiceHandle::set_volume()`], [`VoiceHandle::set_pan()`],
//!   [`VoiceHandle::set_pitch()`], [`VoiceHandle::stop()`], and `_tweened` variants
//! - **Streaming voice control**: [`StreamingVoiceHandle::set_volume()`],
//!   [`StreamingVoiceHandle::seek()`], [`StreamingVoiceHandle::stop()`], etc.
//! - **Category/master volume**: [`AudioEngine::set_master_volume()`],
//!   [`AudioEngine::set_category_volume()`]
//! - **Effects**: [`AudioEngine::add_master_effect()`], [`AudioEngine::add_aux_bus()`],
//!   [`AudioEngine::set_zone_reverb()`]
//!
//! These internally use one of three mechanisms to cross the thread boundary safely
//! (see next section).
//!
//! # Thread safety mechanisms
//!
//! | Mechanism | Used for | Blocking? |
//! |-----------|----------|-----------|
//! | **Atomic operations** | Per-voice volume, pan, pitch, occlusion; master/category volumes; zone reverb params | No |
//! | **SPSC command queue** | `stop()` and `stop_all()` — queued from main thread, drained at the start of each render cycle | No |
//! | **`Mutex<MixerState>`** | Voice pool allocation, voice slot lookup, effect chain setup, streaming decoder access | Brief lock |
//!
//! **Atomics** provide fully lock-free parameter updates. Setting volume, pan, or pitch
//! on a voice writes an `AtomicU32` (f32 bits) that the audio thread reads on the next
//! render cycle with `Ordering::Relaxed`. There is no contention.
//!
//! **The command queue** is a single-producer single-consumer ring buffer
//! (capacity 256). The main thread pushes commands; the audio thread pops them at the
//! start of [`AudioMixer::render()`](mixer::AudioMixer). This avoids locking for
//! stop requests while ensuring they are processed in render order.
//!
//! **The `Mutex<MixerState>`** guards the voice pool, voice index map, effect chains,
//! aux buses, and streaming decoders. It is held briefly by:
//! - The main thread for play/allocate operations and property queries (e.g. `volume()`,
//!     `position()`, `state()`).
//! - The audio thread for the entire render cycle.
//!
//! Because the main-thread lock durations are short (voice allocation is O(1) with a
//! free list, slot lookups are via a HashMap), contention is minimal in practice.
//!
//! # Memory allocation
//!
//! The voice pool is pre-allocated (64 regular voices, 8 streaming voices). Playing a
//! sound reuses an existing slot rather than allocating. If the pool is full, the
//! lowest-priority voice is stolen. Streaming voices use a pre-allocated ring buffer
//! (~4 seconds at 44.1 kHz stereo).
//!
//! `play()` does allocate an `Arc` clone and a HashMap insert, which is a few hundred
//! nanoseconds — acceptable on the main thread but **do not call play() from a
//! real-time audio callback**.
//!
//! # What NOT to do
//!
//! - **Do not call any `AudioEngine` or `VoiceHandle` methods from within a custom
//!   audio callback or any real-time thread.** All public methods may lock the
//!   `MixerState` mutex. If the audio thread already holds it during `render()`, the
//!   calling thread will block until the render cycle completes — which is fine from
//!   the main thread, but would be a deadlock if called from the audio thread itself.
//!
//! - **Do not hold `VoiceHandle` references and expect them to stay valid forever.**
//!   Voices are recycled when they finish playing. Calling methods on a handle whose
//!   voice has been recycled will silently succeed but affect whatever new voice now
//!   occupies that slot. Use [`VoiceHandle::state()`] to check liveness if needed.
//!
//! - **Do not call `play()` at extreme rates** (e.g. thousands per frame). Each play
//!   call locks the mutex and inserts into the voice index. A few dozen per frame is
//!   fine.
//!
//! # Audio categories
//!
//! Three sub-mix channels with independent volume controls:
//! - **SFX** — short sound effects
//! - **Music** — background music
//! - **Ambient** — environmental audio
//!
//! Master volume controls the final output level. Category volumes are applied per-voice
//! during mixing.

mod buffer;
mod clock;
mod command_queue;
mod effect;
mod engine;
mod error;
mod levels;
pub mod metadata;
mod mixer;
mod scheduled_event;
mod sound_cue;
mod streaming;
mod streaming_voice;
mod voice;

pub use buffer::{
    AudioBuffer, DecodedAudio, SampleFormat, load_audio, load_flac, load_mp3, load_ogg, load_wav,
};
pub use command_queue::AudioCategoryValue;
pub use effect::biquad::{BiquadFilter, FilterKind};
pub use effect::reverb::ReverbEffect;
pub use effect::{AudioEffect, AuxBus, EffectChain};
pub use engine::{AudioCategory, AudioEngine};
pub use error::AudioError;
pub use levels::{ChannelLevels, LevelsSnapshot};
pub use metadata::{AudioMetadata, audio_metadata};
pub use sound_cue::{CuePlayMode, SoundCue};
pub use streaming::StreamingDecoder;
pub use streaming_voice::StreamingVoiceHandle;
pub use voice::{
    AuxBusId, VoiceHandle, VoiceId, VoicePriority, VoiceState, compute_pan_gains, db_to_linear,
    linear_to_db,
};

#[cfg(test)]
mod tests;
