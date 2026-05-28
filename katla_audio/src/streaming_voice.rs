use std::cell::UnsafeCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};

use crate::command_queue::AudioCategoryValue;
use crate::error::AudioError;
use crate::streaming::StreamingDecoder;
use crate::voice::{CategoryVolumes, VoiceId, VoiceState, compute_pan_gains};

const STREAM_RING_BUFFER_SAMPLES: usize = 44100 * 2 * 4;
const CHUNK_THRESHOLD: usize = 44100 * 2;
const SILENCE_THRESHOLD: f32 = 0.001;
const SILENCE_FRAMES_BEFORE_SKIP: u32 = 3;

const FRAC_BITS: u32 = 24;
const FRAC_MASK: u64 = (1u64 << FRAC_BITS) - 1;
const FIXED_ONE: u64 = 1u64 << FRAC_BITS;

pub struct StreamingVoice {
    id: VoiceId,
    decoder: std::sync::Mutex<StreamingDecoder>,
    ring_buffer: UnsafeCell<Vec<f32>>,
    write_pos: AtomicUsize,
    read_fixed: AtomicU64,
    ring_channels: u16,
    ring_sample_rate: u32,
    volume: AtomicU32,
    pan: AtomicU32,
    pitch: AtomicU32,
    volume_target: AtomicU32,
    pan_target: AtomicU32,
    pitch_target: AtomicU32,
    tween_smoothing: f32,
    silent_frame_count: u32,
    looping: bool,
    finished: AtomicBool,
    category: AudioCategoryValue,
    category_volumes: Arc<CategoryVolumes>,
}

// SAFETY: StreamingVoice is only accessed from two contexts:
// 1. The audio callback thread via `mix_into()`, which runs under `MixerState`'s Mutex.
// 2. The main thread via property setters (volume/pan/pitch) which use atomics.
// The ring_buffer UnsafeCell is only touched inside `mix_into()`, which is always
// called under the mixer's Mutex, so there is no concurrent access to the ring buffer.
unsafe impl Send for StreamingVoice {}
unsafe impl Sync for StreamingVoice {}

impl StreamingVoice {
    pub fn new(
        id: VoiceId,
        mut decoder: StreamingDecoder,
        looping: bool,
        category: AudioCategoryValue,
        category_volumes: Arc<CategoryVolumes>,
    ) -> Result<Self, AudioError> {
        let channels = decoder.channels();
        let sample_rate = decoder.sample_rate();

        if channels == 0 || sample_rate == 0 {
            return Err(AudioError::DecodeFailed(
                "StreamingDecoder has no audio format info".into(),
            ));
        }

        let initial_chunk = decoder.read_chunk().ok_or(AudioError::DecodeFailed(
            "StreamingDecoder produced no initial data".into(),
        ))?;

        let ring_channels = initial_chunk.channels;
        let ring_sample_rate = initial_chunk.sample_rate;

        let mut ring_buffer = vec![0.0f32; STREAM_RING_BUFFER_SAMPLES];
        let copy_len = initial_chunk.samples.len().min(ring_buffer.len());
        ring_buffer[..copy_len].copy_from_slice(&initial_chunk.samples[..copy_len]);

        let initial_write = copy_len * FIXED_ONE as usize;

        Ok(StreamingVoice {
            id,
            decoder: std::sync::Mutex::new(decoder),
            ring_buffer: UnsafeCell::new(ring_buffer),
            write_pos: AtomicUsize::new(initial_write),
            read_fixed: AtomicU64::new(0),
            ring_channels,
            ring_sample_rate,
            volume: AtomicU32::new(1.0f32.to_bits()),
            pan: AtomicU32::new(0.0f32.to_bits()),
            pitch: AtomicU32::new(1.0f32.to_bits()),
            volume_target: AtomicU32::new(1.0f32.to_bits()),
            pan_target: AtomicU32::new(0.0f32.to_bits()),
            pitch_target: AtomicU32::new(1.0f32.to_bits()),
            tween_smoothing: 0.3,
            silent_frame_count: 0,
            looping,
            finished: AtomicBool::new(false),
            category,
            category_volumes,
        })
    }

    pub fn id(&self) -> VoiceId {
        self.id
    }

    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Relaxed)
    }

    pub fn is_looping(&self) -> bool {
        self.looping
    }

    pub fn set_volume(&self, volume: f32) {
        let v = volume.clamp(0.0, 1.0);
        self.volume_target.store(v.to_bits(), Ordering::Relaxed);
        self.volume.store(v.to_bits(), Ordering::Relaxed);
    }

    pub fn set_volume_tweened(&self, volume: f32) {
        self.volume_target
            .store(volume.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }

    pub fn set_pan(&self, pan: f32) {
        let p = pan.clamp(-1.0, 1.0);
        self.pan_target.store(p.to_bits(), Ordering::Relaxed);
        self.pan.store(p.to_bits(), Ordering::Relaxed);
    }

    pub fn set_pan_tweened(&self, pan: f32) {
        self.pan_target
            .store(pan.clamp(-1.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn set_pitch(&self, pitch: f32) {
        let p = pitch.clamp(0.1, 4.0);
        self.pitch_target.store(p.to_bits(), Ordering::Relaxed);
        self.pitch.store(p.to_bits(), Ordering::Relaxed);
    }

    pub fn set_pitch_tweened(&self, pitch: f32) {
        self.pitch_target
            .store(pitch.clamp(0.1, 4.0).to_bits(), Ordering::Relaxed);
    }

    pub fn set_tween_speed(&mut self, speed: f32) {
        self.tween_smoothing = speed.clamp(0.0, 1.0);
    }

    pub fn tick_tweens(&self) {
        let s = self.tween_smoothing;

        let cur = f32::from_bits(self.volume.load(Ordering::Relaxed));
        let tgt = f32::from_bits(self.volume_target.load(Ordering::Relaxed));
        self.volume
            .store((cur + (tgt - cur) * s).to_bits(), Ordering::Relaxed);

        let cur = f32::from_bits(self.pan.load(Ordering::Relaxed));
        let tgt = f32::from_bits(self.pan_target.load(Ordering::Relaxed));
        self.pan
            .store((cur + (tgt - cur) * s).to_bits(), Ordering::Relaxed);

        let cur = f32::from_bits(self.pitch.load(Ordering::Relaxed));
        let tgt = f32::from_bits(self.pitch_target.load(Ordering::Relaxed));
        self.pitch
            .store((cur + (tgt - cur) * s).to_bits(), Ordering::Relaxed);
    }

    fn category_volume(&self) -> f32 {
        let idx = match self.category {
            AudioCategoryValue::Sfx => 0,
            AudioCategoryValue::Music => 1,
            AudioCategoryValue::Ambient => 2,
        };
        f32::from_bits(self.category_volumes.0[idx].load(Ordering::Relaxed))
    }

    fn fill_ring_buffer(&self) {
        // SAFETY: Called from mix_into(), which runs under MixerState's Mutex.
        let ring_buffer = unsafe { &mut *self.ring_buffer.get() };
        let ring_len = ring_buffer.len();

        let write_fixed = self.write_pos.load(Ordering::Relaxed);
        let read_fixed_val = self.read_fixed.load(Ordering::Relaxed) as usize;
        let ring_len_fixed = ring_len * FIXED_ONE as usize;

        let available_fixed = if write_fixed >= read_fixed_val {
            write_fixed - read_fixed_val
        } else {
            ring_len_fixed - read_fixed_val + write_fixed
        };

        if available_fixed / FIXED_ONE as usize > CHUNK_THRESHOLD {
            return;
        }

        let mut decoder = match self.decoder.lock() {
            Ok(d) => d,
            Err(_) => return,
        };

        if decoder.is_exhausted() {
            if self.looping {
                if decoder.seek_to_start().is_err() {
                    return;
                }
            } else {
                return;
            }
        }

        if let Some(chunk) = decoder.read_chunk() {
            let write_idx = write_fixed / FIXED_ONE as usize;
            let copy_len = chunk.samples.len().min(ring_len);
            let first_part = copy_len.min(ring_len - write_idx);
            ring_buffer[write_idx..write_idx + first_part]
                .copy_from_slice(&chunk.samples[..first_part]);
            if copy_len > first_part {
                let second_part = copy_len - first_part;
                ring_buffer[..second_part].copy_from_slice(&chunk.samples[first_part..copy_len]);
            }
            let new_write = (write_fixed + copy_len * FIXED_ONE as usize) % ring_len;
            self.write_pos.store(new_write, Ordering::Relaxed);
        }
    }

    pub fn mix_into(
        &mut self,
        output: &mut [f32],
        output_channels: usize,
        output_sample_rate: u32,
    ) {
        let voice_volume = self.volume() * self.category_volume();

        if voice_volume < SILENCE_THRESHOLD {
            self.silent_frame_count += 1;
            if self.silent_frame_count >= SILENCE_FRAMES_BEFORE_SKIP {
                return;
            }
        } else {
            self.silent_frame_count = 0;
        }

        self.fill_ring_buffer();

        // SAFETY: Only accessed under MixerState's Mutex in the render callback.
        let ring_buffer = unsafe { &*self.ring_buffer.get() };
        let src_channels = self.ring_channels as usize;
        let ring_len = ring_buffer.len();

        let write_fixed = self.write_pos.load(Ordering::Relaxed);
        let write_samples = write_fixed / FIXED_ONE as usize;
        let read_samples = (self.read_fixed.load(Ordering::Relaxed) >> FRAC_BITS) as usize;

        if read_samples >= write_samples {
            if !self.looping {
                self.finished.store(true, Ordering::Relaxed);
            }
            return;
        }

        let pan = f32::from_bits(self.pan.load(Ordering::Relaxed));
        let (left_gain, right_gain) = compute_pan_gains(pan);

        let pitch = f32::from_bits(self.pitch.load(Ordering::Relaxed));
        let rate_ratio = self.ring_sample_rate as f64 / output_sample_rate as f64;
        let step_fixed = (pitch as f64 * FIXED_ONE as f64 * rate_ratio).round() as u64;

        if step_fixed == 0 {
            return;
        }

        let mut fixed_pos = self.read_fixed.load(Ordering::Relaxed);

        if src_channels == 2 && output_channels == 2 {
            for chunk in output.chunks_exact_mut(2) {
                let int_pos = (fixed_pos >> FRAC_BITS) as usize;
                let frac = (fixed_pos & FRAC_MASK) as f32 / FIXED_ONE as f32;

                if int_pos + 3 >= ring_len {
                    break;
                }
                if int_pos >= write_samples {
                    break;
                }

                let l0 = ring_buffer[int_pos];
                let r0 = ring_buffer[int_pos + 1];
                let l1 = ring_buffer[int_pos + 2];
                let r1 = ring_buffer[int_pos + 3];
                let l = l0 + (l1 - l0) * frac;
                let r = r0 + (r1 - r0) * frac;
                chunk[0] += l * voice_volume * left_gain;
                chunk[1] += r * voice_volume * right_gain;

                fixed_pos += step_fixed * 2;
            }
        } else if src_channels == 1 && output_channels == 2 {
            for chunk in output.chunks_exact_mut(2) {
                let int_pos = (fixed_pos >> FRAC_BITS) as usize;
                let frac = (fixed_pos & FRAC_MASK) as f32 / FIXED_ONE as f32;

                if int_pos + 1 >= ring_len {
                    break;
                }
                if int_pos >= write_samples {
                    break;
                }

                let s0 = ring_buffer[int_pos];
                let s1 = ring_buffer[int_pos + 1];
                let mono = (s0 + (s1 - s0) * frac) * voice_volume;
                chunk[0] += mono * left_gain;
                chunk[1] += mono * right_gain;

                fixed_pos += step_fixed;
            }
        }

        self.read_fixed.store(fixed_pos, Ordering::Relaxed);
    }
}

pub struct StreamingVoiceHandle {
    pub id: VoiceId,
    pub(crate) mixer: Arc<crate::mixer::AudioMixer>,
}

impl StreamingVoiceHandle {
    pub fn stop(&self) {
        self.mixer.stop(self.id);
    }

    pub fn set_volume(&self, volume: f32) {
        self.mixer.set_streaming_voice_volume(self.id, volume);
    }

    pub fn set_volume_tweened(&self, volume: f32) {
        self.mixer
            .set_streaming_voice_volume_tweened(self.id, volume);
    }

    pub fn volume(&self) -> f32 {
        self.mixer.streaming_voice_volume(self.id)
    }

    pub fn set_pan(&self, pan: f32) {
        self.mixer.set_streaming_voice_pan(self.id, pan);
    }

    pub fn set_pan_tweened(&self, pan: f32) {
        self.mixer.set_streaming_voice_pan_tweened(self.id, pan);
    }

    pub fn set_pitch(&self, pitch: f32) {
        self.mixer.set_streaming_voice_pitch(self.id, pitch);
    }

    pub fn set_pitch_tweened(&self, pitch: f32) {
        self.mixer.set_streaming_voice_pitch_tweened(self.id, pitch);
    }

    pub fn set_tween_speed(&self, speed: f32) {
        self.mixer.set_streaming_voice_tween_speed(self.id, speed);
    }

    pub fn state(&self) -> VoiceState {
        self.mixer.streaming_voice_state(self.id)
    }
}
