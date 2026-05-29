use std::cell::UnsafeCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use crate::command_queue::AudioCategoryValue;
use crate::error::AudioError;
use crate::streaming::StreamingDecoder;
use crate::voice::{
    AuxBusId, CategoryVolumes, VoiceId, VoicePriority, VoiceState, catmull_rom, compute_pan_gains,
    fetch_sample,
};

const STREAM_RING_BUFFER_SAMPLES: usize = 44100 * 2 * 4;
const CHUNK_THRESHOLD: usize = 44100 * 2;
const SILENCE_THRESHOLD: f32 = 0.001;
const SILENCE_FRAMES_BEFORE_SKIP: u32 = 3;

const FRAC_BITS: u32 = 24;
const FRAC_MASK: u64 = (1u64 << FRAC_BITS) - 1;
const FIXED_ONE: u64 = 1u64 << FRAC_BITS;

const FADE_NONE: u8 = 0;
const FADE_IN: u8 = 1;
const FADE_OUT: u8 = 2;
const FADE_DURATION_MS: f32 = 3.0;

const LOOP_CROSSFADE_SAMPLES: usize = 256;

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
    fade_state: AtomicU8,
    fade_position: AtomicUsize,
    position_secs: AtomicU32,
    priority: VoicePriority,
    pub(crate) aux_sends: Vec<(AuxBusId, f32)>,
    loop_crossfade_tail: UnsafeCell<Vec<f32>>,
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
        priority: VoicePriority,
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
            fade_state: AtomicU8::new(FADE_IN),
            fade_position: AtomicUsize::new(0),
            position_secs: AtomicU32::new(0.0f32.to_bits()),
            priority,
            aux_sends: Vec::new(),
            loop_crossfade_tail: UnsafeCell::new(Vec::new()),
        })
    }

    pub fn id(&self) -> VoiceId {
        self.id
    }

    pub fn priority(&self) -> VoicePriority {
        self.priority
    }

    pub fn aux_send_level(&self, bus_id: AuxBusId) -> Option<f32> {
        self.aux_sends
            .iter()
            .find(|(id, _)| *id == bus_id)
            .map(|(_, level)| *level)
    }

    pub(crate) fn reset(
        &mut self,
        id: VoiceId,
        mut decoder: StreamingDecoder,
        looping: bool,
        category: AudioCategoryValue,
        category_volumes: Arc<CategoryVolumes>,
        priority: VoicePriority,
    ) -> Result<(), AudioError> {
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

        // SAFETY: Called from the main thread while the voice slot is inactive.
        let ring_buffer = unsafe { &mut *self.ring_buffer.get() };
        ring_buffer.fill(0.0f32);
        let copy_len = initial_chunk.samples.len().min(ring_buffer.len());
        ring_buffer[..copy_len].copy_from_slice(&initial_chunk.samples[..copy_len]);

        let initial_write = copy_len * FIXED_ONE as usize;

        self.id = id;
        *self.decoder.lock().unwrap() = decoder;
        self.write_pos.store(initial_write, Ordering::Relaxed);
        self.read_fixed.store(0, Ordering::Relaxed);
        self.ring_channels = initial_chunk.channels;
        self.ring_sample_rate = initial_chunk.sample_rate;
        self.volume.store(1.0f32.to_bits(), Ordering::Relaxed);
        self.pan.store(0.0f32.to_bits(), Ordering::Relaxed);
        self.pitch.store(1.0f32.to_bits(), Ordering::Relaxed);
        self.volume_target
            .store(1.0f32.to_bits(), Ordering::Relaxed);
        self.pan_target.store(0.0f32.to_bits(), Ordering::Relaxed);
        self.pitch_target.store(1.0f32.to_bits(), Ordering::Relaxed);
        self.tween_smoothing = 0.3;
        self.silent_frame_count = 0;
        self.looping = looping;
        self.finished.store(false, Ordering::Relaxed);
        self.category = category;
        self.category_volumes = category_volumes;
        self.fade_state.store(FADE_IN, Ordering::Relaxed);
        self.fade_position.store(0, Ordering::Relaxed);
        self.position_secs
            .store(0.0f32.to_bits(), Ordering::Relaxed);
        self.priority = priority;
        self.aux_sends.clear();
        unsafe { &mut *self.loop_crossfade_tail.get() }.clear();

        Ok(())
    }

    pub fn position(&self) -> f32 {
        f32::from_bits(self.position_secs.load(Ordering::Relaxed))
    }

    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Relaxed)
    }

    pub fn begin_fade_out(&self) {
        self.fade_state.store(FADE_OUT, Ordering::Relaxed);
        self.fade_position.store(0, Ordering::Relaxed);
    }

    pub fn seek(&self, position: Duration) {
        let mut decoder = match self.decoder.lock() {
            Ok(d) => d,
            Err(_) => return,
        };

        if let Err(e) = decoder.seek(position) {
            log::warn!("StreamingVoice seek failed: {e}");
            return;
        }

        // SAFETY: Called under MixerState's Mutex (via AudioMixer::seek_streaming_voice).
        let ring_buffer = unsafe { &mut *self.ring_buffer.get() };
        ring_buffer.fill(0.0f32);

        self.read_fixed.store(0, Ordering::Relaxed);

        let mut write_samples = 0usize;
        if let Some(chunk) = decoder.read_chunk() {
            let copy_len = chunk.samples.len().min(ring_buffer.len());
            ring_buffer[..copy_len].copy_from_slice(&chunk.samples[..copy_len]);
            write_samples = copy_len;
        }

        self.write_pos
            .store(write_samples * FIXED_ONE as usize, Ordering::Relaxed);
        self.finished.store(false, Ordering::Relaxed);
        self.fade_state.store(FADE_IN, Ordering::Relaxed);
        self.fade_position.store(0, Ordering::Relaxed);
        self.position_secs
            .store(position.as_secs_f32().to_bits(), Ordering::Relaxed);
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

        let looping = self.looping;
        let is_looping = decoder.is_exhausted() && looping;

        if decoder.is_exhausted() {
            if looping {
                if decoder.seek_to_start().is_err() {
                    return;
                }
            } else {
                return;
            }
        }

        if let Some(mut chunk) = decoder.read_chunk() {
            if is_looping {
                let tail = unsafe { &*self.loop_crossfade_tail.get() };
                let fade_len = tail.len().min(chunk.samples.len());
                for (i, chunk_sample) in chunk.samples.iter_mut().enumerate().take(fade_len) {
                    let t = i as f32 / fade_len as f32;
                    let angle = t * std::f32::consts::FRAC_PI_2;
                    *chunk_sample = tail[i] * angle.cos() + *chunk_sample * angle.sin();
                }
            }

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

            let tail_len = LOOP_CROSSFADE_SAMPLES.min(chunk.samples.len());
            let tail_buf = unsafe { &mut *self.loop_crossfade_tail.get() };
            tail_buf.resize(tail_len, 0.0);
            tail_buf.copy_from_slice(&chunk.samples[chunk.samples.len() - tail_len..]);
        }
    }

    pub fn mix_into(
        &mut self,
        output: &mut [f32],
        output_channels: usize,
        output_sample_rate: u32,
    ) {
        let voice_volume = self.volume() * self.category_volume();

        if voice_volume < SILENCE_THRESHOLD && self.fade_state.load(Ordering::Relaxed) == FADE_NONE
        {
            self.silent_frame_count += 1;
            if self.silent_frame_count >= SILENCE_FRAMES_BEFORE_SKIP {
                return;
            }
        } else {
            self.silent_frame_count = 0;
        }

        let fade_state = self.fade_state.load(Ordering::Relaxed);
        let fade_pos_start = self.fade_position.load(Ordering::Relaxed);
        let fade_length = (FADE_DURATION_MS * output_sample_rate as f32 / 1000.0) as usize;
        let mut frames_mixed = 0usize;

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

                let fade_gain =
                    streaming_fade_gain(fade_state, fade_pos_start + frames_mixed, fade_length);
                let vol = voice_volume * fade_gain;

                let ip = int_pos as isize;
                let lp0 = fetch_sample(ring_buffer, ip - 2, ring_len);
                let rp0 = fetch_sample(ring_buffer, ip - 1, ring_len);
                let lp1 = ring_buffer[int_pos];
                let rp1 = ring_buffer[int_pos + 1];
                let lp2 = fetch_sample(ring_buffer, ip + 2, ring_len);
                let rp2 = fetch_sample(ring_buffer, ip + 3, ring_len);
                let lp3 = fetch_sample(ring_buffer, ip + 4, ring_len);
                let rp3 = fetch_sample(ring_buffer, ip + 5, ring_len);
                let l = catmull_rom(lp0, lp1, lp2, lp3, frac);
                let r = catmull_rom(rp0, rp1, rp2, rp3, frac);
                chunk[0] += l * vol * left_gain;
                chunk[1] += r * vol * right_gain;

                frames_mixed += 1;
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

                let fade_gain =
                    streaming_fade_gain(fade_state, fade_pos_start + frames_mixed, fade_length);
                let vol = voice_volume * fade_gain;

                let ip = int_pos as isize;
                let sp0 = fetch_sample(ring_buffer, ip - 1, ring_len);
                let sp1 = ring_buffer[int_pos];
                let sp2 = fetch_sample(ring_buffer, ip + 1, ring_len);
                let sp3 = fetch_sample(ring_buffer, ip + 2, ring_len);
                let mono = catmull_rom(sp0, sp1, sp2, sp3, frac) * vol;
                chunk[0] += mono * left_gain;
                chunk[1] += mono * right_gain;

                frames_mixed += 1;
                fixed_pos += step_fixed;
            }
        }

        self.read_fixed.store(fixed_pos, Ordering::Relaxed);

        let elapsed = frames_mixed as f32 / output_sample_rate as f32;
        let prev = f32::from_bits(self.position_secs.load(Ordering::Relaxed));
        self.position_secs
            .store((prev + elapsed).to_bits(), Ordering::Relaxed);

        if fade_state != FADE_NONE {
            let new_pos = fade_pos_start + frames_mixed;
            self.fade_position.store(new_pos, Ordering::Relaxed);
            if new_pos >= fade_length {
                if fade_state == FADE_IN {
                    self.fade_state.store(FADE_NONE, Ordering::Relaxed);
                } else if fade_state == FADE_OUT {
                    self.finished.store(true, Ordering::Relaxed);
                }
            }
        }
    }
}

#[inline]
fn streaming_fade_gain(fade_state: u8, pos: usize, len: usize) -> f32 {
    if len == 0 || fade_state == FADE_NONE {
        return 1.0;
    }
    match fade_state {
        FADE_IN => (pos as f32 / len as f32).min(1.0),
        FADE_OUT => (1.0 - pos as f32 / len as f32).max(0.0),
        _ => 1.0,
    }
}

/// Main-thread handle to a streaming voice (long audio, e.g. music).
///
/// Behaves like [`VoiceHandle`] but adds [`seek()`](StreamingVoiceHandle::seek) for
/// seeking within the stream. Same thread safety considerations apply.
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

    pub fn position(&self) -> f32 {
        self.mixer.streaming_voice_position(self.id)
    }

    pub fn seek(&self, position: Duration) {
        self.mixer.seek_streaming_voice(self.id, position);
    }

    pub fn set_tween_speed(&self, speed: f32) {
        self.mixer.set_streaming_voice_tween_speed(self.id, speed);
    }

    pub fn state(&self) -> VoiceState {
        self.mixer.streaming_voice_state(self.id)
    }

    pub fn set_aux_sends(&self, sends: Vec<(AuxBusId, f32)>) {
        self.mixer.set_streaming_voice_aux_sends(self.id, sends);
    }
}
