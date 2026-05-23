use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

use crate::buffer::AudioBuffer;

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
    position: AtomicUsize,
    volume: AtomicU32,
    looping: bool,
    finished: AtomicBool,
}

impl Voice {
    pub fn new(id: VoiceId, buffer: Arc<AudioBuffer>, looping: bool) -> Self {
        Voice {
            id,
            buffer,
            position: AtomicUsize::new(0),
            volume: AtomicU32::new(1.0f32.to_bits()),
            looping,
            finished: AtomicBool::new(false),
        }
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

        let pos = self.position.load(Ordering::Relaxed);
        let mut samples_written = 0;

        if src_channels == output_channels {
            let mut src_idx = pos;
            for chunk in output.chunks_exact_mut(src_channels) {
                if src_idx + src_channels > total_src_samples {
                    break;
                }
                for ch in 0..src_channels {
                    chunk[ch] += src_samples[src_idx + ch] * volume;
                }
                src_idx += src_channels;
                samples_written += 1;
            }
        } else if src_channels == 1 && output_channels == 2 {
            let mut src_frame = pos;
            for chunk in output.chunks_exact_mut(2) {
                if src_frame >= total_src_samples {
                    break;
                }
                let mono = src_samples[src_frame] * volume;
                chunk[0] += mono;
                chunk[1] += mono;
                src_frame += 1;
                samples_written += 1;
            }
        } else if src_channels == 2 && output_channels == 1 {
            let mut src_idx = pos;
            for out in output.iter_mut() {
                if src_idx + 1 >= total_src_samples {
                    break;
                }
                *out += (src_samples[src_idx] + src_samples[src_idx + 1]) * 0.5 * volume;
                src_idx += 2;
                samples_written += 1;
            }
        }

        let new_pos = pos + samples_written * src_channels;
        if new_pos >= total_src_samples {
            if self.looping {
                self.position
                    .store(new_pos % total_src_samples, Ordering::Relaxed);
            } else {
                self.position.store(total_src_samples, Ordering::Relaxed);
                self.finished.store(true, Ordering::Relaxed);
            }
        } else {
            self.position.store(new_pos, Ordering::Relaxed);
        }
    }
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

    pub fn state(&self) -> VoiceState {
        self.mixer.voice_state(self.id)
    }
}
