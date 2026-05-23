use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use crate::buffer::AudioBuffer;

const FRAC_BITS: u32 = 24;
const FRAC_MASK: u64 = (1u64 << FRAC_BITS) - 1;
const FIXED_ONE: u64 = 1u64 << FRAC_BITS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VoiceId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceState {
    Playing,
    Stopped,
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
    looping: bool,
    finished: AtomicBool,
}

impl Voice {
    pub fn new(id: VoiceId, buffer: Arc<AudioBuffer>, looping: bool) -> Self {
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
            looping,
            finished: AtomicBool::new(false),
        }
    }

    pub fn with_loop_region(mut self, start_sample: usize, end_sample: usize) -> Self {
        self.loop_start = start_sample as u64 * FIXED_ONE;
        self.loop_end = end_sample as u64 * FIXED_ONE;
        self
    }

    pub fn id(&self) -> VoiceId {
        self.id
    }

    pub fn set_volume(&self, volume: f32) {
        self.volume
            .store(volume.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }

    pub fn set_pan(&self, pan: f32) {
        self.pan
            .store(pan.clamp(-1.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn pan(&self) -> f32 {
        f32::from_bits(self.pan.load(Ordering::Relaxed))
    }

    pub fn set_pitch(&self, pitch: f32) {
        self.pitch
            .store(pitch.clamp(0.1, 4.0).to_bits(), Ordering::Relaxed);
    }

    pub fn pitch(&self) -> f32 {
        f32::from_bits(self.pitch.load(Ordering::Relaxed))
    }

    pub fn is_looping(&self) -> bool {
        self.looping
    }

    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Relaxed)
    }

    pub fn mix_into(&self, output: &mut [f32], output_channels: usize, _output_sample_rate: u32) {
        let src_channels = self.buffer.channels as usize;
        let src_samples = &self.buffer.samples;
        let total_src_samples = src_samples.len();
        let volume = self.volume();

        if total_src_samples == 0 || output_channels == 0 {
            return;
        }

        let pan = self.pan();
        let (left_gain, right_gain) = compute_pan_gains(pan);

        let pitch = self.pitch();
        let step_fixed = (pitch * FIXED_ONE as f32) as u64;
        let total_fixed = total_src_samples as u64 * FIXED_ONE;
        let loop_end = if self.looping {
            self.loop_end.min(total_fixed)
        } else {
            total_fixed
        };

        let mut fixed_pos = self.fixed_position.load(Ordering::Relaxed);

        if src_channels == output_channels {
            for chunk in output.chunks_exact_mut(src_channels) {
                let int_pos = (fixed_pos >> FRAC_BITS) as usize;
                let frac = (fixed_pos & FRAC_MASK) as f32 / FIXED_ONE as f32;

                if int_pos + src_channels > total_src_samples {
                    break;
                }

                if src_channels == 2 {
                    let l0 = src_samples[int_pos];
                    let r0 = src_samples[int_pos + 1];
                    let l1 = if int_pos + 3 < total_src_samples {
                        src_samples[int_pos + 2]
                    } else {
                        l0
                    };
                    let r1 = if int_pos + 3 < total_src_samples {
                        src_samples[int_pos + 3]
                    } else {
                        r0
                    };
                    let l = l0 + (l1 - l0) * frac;
                    let r = r0 + (r1 - r0) * frac;
                    chunk[0] += l * volume * left_gain;
                    chunk[1] += r * volume * right_gain;
                } else {
                    for ch in 0..src_channels {
                        let s0 = src_samples[int_pos + ch];
                        let s1 = if int_pos + src_channels + ch < total_src_samples {
                            src_samples[int_pos + src_channels + ch]
                        } else {
                            s0
                        };
                        chunk[ch] += (s0 + (s1 - s0) * frac) * volume;
                    }
                }

                fixed_pos += step_fixed * src_channels as u64;
            }
        } else if src_channels == 1 && output_channels == 2 {
            for chunk in output.chunks_exact_mut(2) {
                let int_pos = (fixed_pos >> FRAC_BITS) as usize;
                let frac = (fixed_pos & FRAC_MASK) as f32 / FIXED_ONE as f32;

                if int_pos >= total_src_samples {
                    break;
                }

                let s0 = src_samples[int_pos];
                let s1 = if int_pos + 1 < total_src_samples {
                    src_samples[int_pos + 1]
                } else {
                    s0
                };
                let mono = (s0 + (s1 - s0) * frac) * volume;
                chunk[0] += mono * left_gain;
                chunk[1] += mono * right_gain;

                fixed_pos += step_fixed;
            }
        } else if src_channels == 2 && output_channels == 1 {
            for out in output.iter_mut() {
                let int_pos = (fixed_pos >> FRAC_BITS) as usize;
                let frac = (fixed_pos & FRAC_MASK) as f32 / FIXED_ONE as f32;

                if int_pos + 1 >= total_src_samples {
                    break;
                }

                let l0 = src_samples[int_pos];
                let r0 = src_samples[int_pos + 1];
                let l1 = if int_pos + 3 < total_src_samples {
                    src_samples[int_pos + 2]
                } else {
                    l0
                };
                let r1 = if int_pos + 3 < total_src_samples {
                    src_samples[int_pos + 3]
                } else {
                    r0
                };
                let l = l0 + (l1 - l0) * frac;
                let r = r0 + (r1 - r0) * frac;
                *out += (l * left_gain + r * right_gain) * 0.5 * volume;

                fixed_pos += step_fixed * 2;
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

pub fn compute_pan_gains(pan: f32) -> (f32, f32) {
    if pan == 0.0 {
        return (1.0, 1.0);
    }
    let angle = (pan + 1.0) * 0.25 * std::f32::consts::PI;
    (angle.cos(), angle.sin())
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

    pub fn volume(&self) -> f32 {
        self.mixer.voice_volume(self.id)
    }

    pub fn set_pan(&self, pan: f32) {
        self.mixer.set_voice_pan(self.id, pan);
    }

    pub fn set_pitch(&self, pitch: f32) {
        self.mixer.set_voice_pitch(self.id, pitch);
    }

    pub fn state(&self) -> VoiceState {
        self.mixer.voice_state(self.id)
    }
}
