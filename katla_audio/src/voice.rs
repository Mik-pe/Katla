use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, AtomicUsize, Ordering};

use crate::buffer::AudioBuffer;
use crate::command_queue::AudioCategoryValue;

const FRAC_BITS: u32 = 24;
const FRAC_MASK: u64 = (1u64 << FRAC_BITS) - 1;
const FIXED_ONE: u64 = 1u64 << FRAC_BITS;

const FADE_NONE: u8 = 0;
const FADE_IN: u8 = 1;
const FADE_OUT: u8 = 2;
const FADE_DURATION_MS: f32 = 3.0;

/// Per-voice one-pole low-pass filter for occlusion.
/// When occlusion > 0, the cutoff frequency is reduced, simulating
/// sound passing through walls/obstacles.
struct OcclusionFilter {
    state: [std::cell::Cell<f32>; 2],
    coefficient: std::cell::Cell<f32>,
}

impl OcclusionFilter {
    fn new() -> Self {
        OcclusionFilter {
            state: [std::cell::Cell::new(0.0), std::cell::Cell::new(0.0)],
            coefficient: std::cell::Cell::new(1.0),
        }
    }

    fn set_occlusion(&self, occlusion: f32, sample_rate: f32) {
        let occlusion = occlusion.clamp(0.0, 1.0);
        let min_cutoff = 200.0f32;
        let max_cutoff = sample_rate * 0.5;
        let cutoff = max_cutoff * (1.0 - occlusion) + min_cutoff * occlusion;
        let rc = 1.0 / (2.0 * std::f32::consts::PI * cutoff);
        let dt = 1.0 / sample_rate;
        self.coefficient.set(dt / (rc + dt));
    }

    fn process_sample(&self, ch: usize, sample: f32) -> f32 {
        let s = self.state[ch].get() + self.coefficient.get() * (sample - self.state[ch].get());
        self.state[ch].set(s);
        s
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VoiceId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AuxBusId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceState {
    Playing,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum VoicePriority {
    Low,
    #[default]
    Medium,
    High,
}

pub struct CategoryVolumes(pub [AtomicU32; 3]);

impl CategoryVolumes {
    pub fn new() -> Self {
        CategoryVolumes([
            AtomicU32::new(1.0f32.to_bits()),
            AtomicU32::new(1.0f32.to_bits()),
            AtomicU32::new(1.0f32.to_bits()),
        ])
    }
}

pub struct Voice {
    id: VoiceId,
    buffer: Arc<AudioBuffer>,
    fixed_position: AtomicU64,
    loop_start: u64,
    loop_end: u64,
    volume: AtomicU32,
    pan: AtomicU32,
    pitch: AtomicU32,
    volume_target: AtomicU32,
    pan_target: AtomicU32,
    pitch_target: AtomicU32,
    occlusion: AtomicU32,
    tween_smoothing: f32,
    looping: bool,
    finished: AtomicBool,
    category: AudioCategoryValue,
    category_volumes: Arc<CategoryVolumes>,
    occlusion_filter: OcclusionFilter,
    fade_state: AtomicU8,
    fade_position: AtomicUsize,
    priority: VoicePriority,
    pub(crate) aux_sends: Vec<(AuxBusId, f32)>,
}

impl Voice {
    pub fn new(
        id: VoiceId,
        buffer: Arc<AudioBuffer>,
        looping: bool,
        category: AudioCategoryValue,
        category_volumes: Arc<CategoryVolumes>,
        priority: VoicePriority,
    ) -> Self {
        let total_fixed = buffer.samples.len() as u64 * FIXED_ONE;
        Voice {
            id,
            buffer,
            fixed_position: AtomicU64::new(0),
            loop_start: 0,
            loop_end: total_fixed,
            volume: AtomicU32::new(1.0f32.to_bits()),
            pan: AtomicU32::new(0.0f32.to_bits()),
            pitch: AtomicU32::new(1.0f32.to_bits()),
            volume_target: AtomicU32::new(1.0f32.to_bits()),
            pan_target: AtomicU32::new(0.0f32.to_bits()),
            pitch_target: AtomicU32::new(1.0f32.to_bits()),
            occlusion: AtomicU32::new(0.0f32.to_bits()),
            tween_smoothing: 0.3,
            looping,
            finished: AtomicBool::new(false),
            category,
            category_volumes,
            occlusion_filter: OcclusionFilter::new(),
            fade_state: AtomicU8::new(FADE_IN),
            fade_position: AtomicUsize::new(0),
            priority,
            aux_sends: Vec::new(),
        }
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
        buffer: Arc<AudioBuffer>,
        looping: bool,
        category: AudioCategoryValue,
        category_volumes: Arc<CategoryVolumes>,
        priority: VoicePriority,
    ) {
        let total_fixed = buffer.samples.len() as u64 * FIXED_ONE;
        self.id = id;
        self.buffer = buffer;
        self.fixed_position.store(0, Ordering::Relaxed);
        self.loop_start = 0;
        self.loop_end = total_fixed;
        self.volume.store(1.0f32.to_bits(), Ordering::Relaxed);
        self.pan.store(0.0f32.to_bits(), Ordering::Relaxed);
        self.pitch.store(1.0f32.to_bits(), Ordering::Relaxed);
        self.volume_target
            .store(1.0f32.to_bits(), Ordering::Relaxed);
        self.pan_target.store(0.0f32.to_bits(), Ordering::Relaxed);
        self.pitch_target.store(1.0f32.to_bits(), Ordering::Relaxed);
        self.occlusion.store(0.0f32.to_bits(), Ordering::Relaxed);
        self.tween_smoothing = 0.3;
        self.looping = looping;
        self.finished.store(false, Ordering::Relaxed);
        self.category = category;
        self.category_volumes = category_volumes;
        self.occlusion_filter = OcclusionFilter::new();
        self.fade_state.store(FADE_IN, Ordering::Relaxed);
        self.fade_position.store(0, Ordering::Relaxed);
        self.priority = priority;
        self.aux_sends.clear();
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

    pub fn set_tween_speed(&mut self, speed: f32) {
        self.tween_smoothing = speed.clamp(0.0, 1.0);
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

    pub fn pan(&self) -> f32 {
        f32::from_bits(self.pan.load(Ordering::Relaxed))
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

    pub fn pitch(&self) -> f32 {
        f32::from_bits(self.pitch.load(Ordering::Relaxed))
    }

    pub fn set_occlusion(&self, occlusion: f32) {
        self.occlusion
            .store(occlusion.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn occlusion(&self) -> f32 {
        f32::from_bits(self.occlusion.load(Ordering::Relaxed))
    }

    fn category_volume(&self) -> f32 {
        let idx = match self.category {
            AudioCategoryValue::Sfx => 0,
            AudioCategoryValue::Music => 1,
            AudioCategoryValue::Ambient => 2,
        };
        f32::from_bits(self.category_volumes.0[idx].load(Ordering::Relaxed))
    }

    pub fn tick_tweens(&self) {
        let s = self.tween_smoothing;

        let cur_vol = f32::from_bits(self.volume.load(Ordering::Relaxed));
        let tgt_vol = f32::from_bits(self.volume_target.load(Ordering::Relaxed));
        let new_vol = cur_vol + (tgt_vol - cur_vol) * s;
        self.volume.store(new_vol.to_bits(), Ordering::Relaxed);

        let cur_pan = f32::from_bits(self.pan.load(Ordering::Relaxed));
        let tgt_pan = f32::from_bits(self.pan_target.load(Ordering::Relaxed));
        let new_pan = cur_pan + (tgt_pan - cur_pan) * s;
        self.pan.store(new_pan.to_bits(), Ordering::Relaxed);

        let cur_pitch = f32::from_bits(self.pitch.load(Ordering::Relaxed));
        let tgt_pitch = f32::from_bits(self.pitch_target.load(Ordering::Relaxed));
        let new_pitch = cur_pitch + (tgt_pitch - cur_pitch) * s;
        self.pitch.store(new_pitch.to_bits(), Ordering::Relaxed);

        if self.occlusion() > 0.0 {
            self.occlusion_filter
                .set_occlusion(self.occlusion(), self.buffer.sample_rate as f32);
        }
    }

    pub fn position(&self) -> f32 {
        let fixed_pos = self.fixed_position.load(Ordering::Relaxed);
        let sample_index = (fixed_pos >> FRAC_BITS) as f32;
        sample_index / (self.buffer.sample_rate as f32 * self.buffer.channels as f32)
    }

    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Relaxed)
    }

    pub fn begin_fade_out(&self) {
        self.fade_state.store(FADE_OUT, Ordering::Relaxed);
        self.fade_position.store(0, Ordering::Relaxed);
    }

    pub fn mix_into(&self, output: &mut [f32], output_channels: usize, output_sample_rate: u32) {
        let src_channels = self.buffer.channels as usize;
        let src_samples = &self.buffer.samples;
        let total_src_samples = src_samples.len();
        let voice_volume = self.volume() * self.category_volume();

        if total_src_samples == 0 || output_channels == 0 {
            return;
        }

        let fade_state = self.fade_state.load(Ordering::Relaxed);
        let fade_pos_start = self.fade_position.load(Ordering::Relaxed);
        let fade_length = (FADE_DURATION_MS * output_sample_rate as f32 / 1000.0) as usize;
        let mut frames_mixed = 0usize;

        let pan = self.pan();
        let (left_gain, right_gain) = compute_pan_gains(pan);

        let pitch = self.pitch();
        let rate_ratio = self.buffer.sample_rate as f64 / output_sample_rate as f64;
        let step_fixed = (pitch as f64 * FIXED_ONE as f64 * rate_ratio).round() as u64;
        let total_fixed = total_src_samples as u64 * FIXED_ONE;
        let loop_end = if self.looping {
            self.loop_end.min(total_fixed)
        } else {
            total_fixed
        };

        if step_fixed == 0 {
            return;
        }

        let occluded = self.occlusion() > 0.0;
        if occluded {
            self.occlusion_filter
                .set_occlusion(self.occlusion(), output_sample_rate as f32);
        }

        let mut fixed_pos = self.fixed_position.load(Ordering::Relaxed);

        if src_channels == output_channels {
            for chunk in output.chunks_exact_mut(src_channels) {
                let int_pos = (fixed_pos >> FRAC_BITS) as usize;
                let frac = (fixed_pos & FRAC_MASK) as f32 / FIXED_ONE as f32;

                if int_pos + src_channels > total_src_samples {
                    break;
                }

                let fade_gain =
                    compute_fade_gain(fade_state, fade_pos_start + frames_mixed, fade_length);
                let vol = voice_volume * fade_gain;

                if src_channels == 2 {
                    let ip = int_pos as isize;
                    let lp0 = fetch_sample(src_samples, ip - 2, total_src_samples);
                    let rp0 = fetch_sample(src_samples, ip - 1, total_src_samples);
                    let lp1 = src_samples[int_pos];
                    let rp1 = src_samples[int_pos + 1];
                    let lp2 = fetch_sample(src_samples, ip + 2, total_src_samples);
                    let rp2 = fetch_sample(src_samples, ip + 3, total_src_samples);
                    let lp3 = fetch_sample(src_samples, ip + 4, total_src_samples);
                    let rp3 = fetch_sample(src_samples, ip + 5, total_src_samples);
                    let mut l = catmull_rom(lp0, lp1, lp2, lp3, frac);
                    let mut r = catmull_rom(rp0, rp1, rp2, rp3, frac);
                    if occluded {
                        l = self.occlusion_filter.process_sample(0, l);
                        r = self.occlusion_filter.process_sample(1, r);
                    }
                    chunk[0] += l * vol * left_gain;
                    chunk[1] += r * vol * right_gain;
                } else {
                    let stride = src_channels as isize;
                    for ch in 0..src_channels {
                        let base = int_pos as isize + ch as isize;
                        let sp0 = fetch_sample(src_samples, base - stride, total_src_samples);
                        let sp1 = src_samples[int_pos + ch];
                        let sp2 = fetch_sample(src_samples, base + stride, total_src_samples);
                        let sp3 = fetch_sample(src_samples, base + 2 * stride, total_src_samples);
                        let mut s = catmull_rom(sp0, sp1, sp2, sp3, frac);
                        if occluded {
                            s = self.occlusion_filter.process_sample(ch.min(1), s);
                        }
                        chunk[ch] += s * vol;
                    }
                }

                frames_mixed += 1;
                fixed_pos += step_fixed * src_channels as u64;
            }
        } else if src_channels == 1 && output_channels == 2 {
            for chunk in output.chunks_exact_mut(2) {
                let int_pos = (fixed_pos >> FRAC_BITS) as usize;
                let frac = (fixed_pos & FRAC_MASK) as f32 / FIXED_ONE as f32;

                if int_pos >= total_src_samples {
                    break;
                }

                let fade_gain =
                    compute_fade_gain(fade_state, fade_pos_start + frames_mixed, fade_length);
                let vol = voice_volume * fade_gain;

                let ip = int_pos as isize;
                let sp0 = fetch_sample(src_samples, ip - 1, total_src_samples);
                let sp1 = src_samples[int_pos];
                let sp2 = fetch_sample(src_samples, ip + 1, total_src_samples);
                let sp3 = fetch_sample(src_samples, ip + 2, total_src_samples);
                let mut mono = catmull_rom(sp0, sp1, sp2, sp3, frac);
                if occluded {
                    mono = self.occlusion_filter.process_sample(0, mono);
                }
                chunk[0] += mono * vol * left_gain;
                chunk[1] += mono * vol * right_gain;

                frames_mixed += 1;
                fixed_pos += step_fixed;
            }
        } else if src_channels == 2 && output_channels == 1 {
            for out in output.iter_mut() {
                let int_pos = (fixed_pos >> FRAC_BITS) as usize;
                let frac = (fixed_pos & FRAC_MASK) as f32 / FIXED_ONE as f32;

                if int_pos + 1 >= total_src_samples {
                    break;
                }

                let fade_gain =
                    compute_fade_gain(fade_state, fade_pos_start + frames_mixed, fade_length);
                let vol = voice_volume * fade_gain;

                let ip = int_pos as isize;
                let lp0 = fetch_sample(src_samples, ip - 2, total_src_samples);
                let rp0 = fetch_sample(src_samples, ip - 1, total_src_samples);
                let lp1 = src_samples[int_pos];
                let rp1 = src_samples[int_pos + 1];
                let lp2 = fetch_sample(src_samples, ip + 2, total_src_samples);
                let rp2 = fetch_sample(src_samples, ip + 3, total_src_samples);
                let lp3 = fetch_sample(src_samples, ip + 4, total_src_samples);
                let rp3 = fetch_sample(src_samples, ip + 5, total_src_samples);
                let mut l = catmull_rom(lp0, lp1, lp2, lp3, frac);
                let mut r = catmull_rom(rp0, rp1, rp2, rp3, frac);
                if occluded {
                    l = self.occlusion_filter.process_sample(0, l);
                    r = self.occlusion_filter.process_sample(1, r);
                }
                *out += (l * left_gain + r * right_gain) * 0.5 * vol;

                frames_mixed += 1;
                fixed_pos += step_fixed * 2;
            }
        }

        if fade_state != FADE_NONE {
            let new_pos = fade_pos_start + frames_mixed;
            self.fade_position.store(new_pos, Ordering::Relaxed);
            if new_pos >= fade_length {
                if fade_state == FADE_IN {
                    self.fade_state.store(FADE_NONE, Ordering::Relaxed);
                } else if fade_state == FADE_OUT {
                    self.finished.store(true, Ordering::Relaxed);
                    return;
                }
            }
        }

        if fixed_pos >= loop_end {
            if self.looping {
                let overshoot = fixed_pos - loop_end;
                self.fixed_position
                    .store(self.loop_start + overshoot, Ordering::Relaxed);
            } else {
                self.fixed_position.store(total_fixed, Ordering::Relaxed);
                self.finished.store(true, Ordering::Relaxed);
            }
        } else {
            self.fixed_position.store(fixed_pos, Ordering::Relaxed);
        }
    }
}

#[inline]
pub(crate) fn catmull_rom(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    let t2 = t * t;
    let t3 = t2 * t;
    0.5 * ((2.0 * p1)
        + (-p0 + p2) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)
}

#[inline]
pub(crate) fn fetch_sample(samples: &[f32], index: isize, len: usize) -> f32 {
    if index < 0 {
        samples[0]
    } else if index as usize >= len {
        samples[len - 1]
    } else {
        samples[index as usize]
    }
}

pub fn compute_pan_gains(pan: f32) -> (f32, f32) {
    let angle = (pan + 1.0) * 0.25 * std::f32::consts::PI;
    (angle.cos(), angle.sin())
}

#[inline]
fn compute_fade_gain(fade_state: u8, pos: usize, len: usize) -> f32 {
    if len == 0 || fade_state == FADE_NONE {
        return 1.0;
    }
    match fade_state {
        FADE_IN => (pos as f32 / len as f32).min(1.0),
        FADE_OUT => (1.0 - pos as f32 / len as f32).max(0.0),
        _ => 1.0,
    }
}

pub fn db_to_linear(db: f32) -> f32 {
    10.0f32.powf(db / 20.0)
}

pub fn linear_to_db(linear: f32) -> f32 {
    if linear <= 0.0 {
        return f32::NEG_INFINITY;
    }
    20.0 * linear.log10()
}

pub struct VoiceHandle {
    pub id: VoiceId,
    pub(crate) mixer: Arc<crate::mixer::AudioMixer>,
}

impl VoiceHandle {
    pub fn stop(&self) {
        self.mixer.stop(self.id);
    }

    pub fn set_volume(&self, volume: f32) {
        self.mixer.set_voice_volume(self.id, volume);
    }

    pub fn set_volume_tweened(&self, volume: f32) {
        self.mixer.set_voice_volume_tweened(self.id, volume);
    }

    pub fn volume(&self) -> f32 {
        self.mixer.voice_volume(self.id)
    }

    pub fn set_pan(&self, pan: f32) {
        self.mixer.set_voice_pan(self.id, pan);
    }

    pub fn set_pan_tweened(&self, pan: f32) {
        self.mixer.set_voice_pan_tweened(self.id, pan);
    }

    pub fn set_pitch(&self, pitch: f32) {
        self.mixer.set_voice_pitch(self.id, pitch);
    }

    pub fn set_pitch_tweened(&self, pitch: f32) {
        self.mixer.set_voice_pitch_tweened(self.id, pitch);
    }

    pub fn set_occlusion(&self, occlusion: f32) {
        self.mixer.set_voice_occlusion(self.id, occlusion);
    }

    pub fn set_tween_speed(&self, speed: f32) {
        self.mixer.set_voice_tween_speed(self.id, speed);
    }

    pub fn position(&self) -> f32 {
        self.mixer.voice_position(self.id)
    }

    pub fn state(&self) -> VoiceState {
        self.mixer.voice_state(self.id)
    }

    pub fn set_aux_sends(&self, sends: Vec<(AuxBusId, f32)>) {
        self.mixer.set_voice_aux_sends(self.id, sends);
    }
}
