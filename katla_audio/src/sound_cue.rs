use std::cell::Cell;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::buffer::AudioBuffer;
use crate::engine::{AudioCategory, AudioEngine};
use crate::voice::VoiceHandle;

struct Xorshift64 {
    state: Cell<u64>,
}

impl Xorshift64 {
    fn new() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x12345678_9ABCDEF0);
        Xorshift64 {
            state: Cell::new(if seed == 0 { 1 } else { seed }),
        }
    }

    fn next_u32(&self) -> u32 {
        let mut x = self.state.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state.set(x);
        x as u32
    }
}

fn random_f32(rng: &Xorshift64) -> f32 {
    (rng.next_u32() as f32) / (u32::MAX as f32) * 2.0 - 1.0
}

fn random_usize(rng: &Xorshift64, max: usize) -> usize {
    (rng.next_u32() as usize) % max
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CuePlayMode {
    Random,
    Sequential,
    Shuffle,
}

pub struct SoundCue {
    buffers: Vec<Arc<AudioBuffer>>,
    play_mode: CuePlayMode,
    pitch_variation: f32,
    volume_variation: f32,
    category: AudioCategory,
    next_index: usize,
    rng: Xorshift64,
}

impl SoundCue {
    pub fn new(category: AudioCategory) -> Self {
        SoundCue {
            buffers: Vec::new(),
            play_mode: CuePlayMode::Random,
            pitch_variation: 0.0,
            volume_variation: 0.0,
            category,
            next_index: 0,
            rng: Xorshift64::new(),
        }
    }

    pub fn with_buffer(mut self, buffer: Arc<AudioBuffer>) -> Self {
        self.buffers.push(buffer);
        self
    }

    pub fn with_buffers(mut self, buffers: Vec<Arc<AudioBuffer>>) -> Self {
        self.buffers = buffers;
        self
    }

    pub fn with_play_mode(mut self, mode: CuePlayMode) -> Self {
        self.play_mode = mode;
        self
    }

    pub fn with_pitch_variation(mut self, semitones: f32) -> Self {
        self.pitch_variation = semitones;
        self
    }

    pub fn with_volume_variation(mut self, db: f32) -> Self {
        self.volume_variation = db;
        self
    }

    pub fn play(&mut self, engine: &AudioEngine) -> Option<VoiceHandle> {
        if self.buffers.is_empty() {
            return None;
        }

        let index = self.select_buffer()?;
        let buffer = self.buffers.get(index)?.clone();
        let handle = engine.play_with_category(&buffer, self.category);

        if self.pitch_variation != 0.0 {
            let semitones = random_f32(&self.rng) * self.pitch_variation;
            let pitch = 2.0f32.powf(semitones / 12.0);
            handle.set_pitch(pitch);
        }

        if self.volume_variation != 0.0 {
            let db = random_f32(&self.rng) * self.volume_variation;
            let volume = 10.0f32.powf(db / 20.0);
            handle.set_volume(volume);
        }

        Some(handle)
    }

    fn select_buffer(&mut self) -> Option<usize> {
        let len = self.buffers.len();
        if len == 0 {
            return None;
        }
        if len == 1 {
            return Some(0);
        }

        match self.play_mode {
            CuePlayMode::Random => Some(random_usize(&self.rng, len)),
            CuePlayMode::Sequential => {
                let idx = self.next_index % len;
                self.next_index = (self.next_index + 1) % len;
                Some(idx)
            }
            CuePlayMode::Shuffle => {
                if self.next_index == 0 {
                    self.next_index = len;
                }
                let swap_from = self.next_index - 1;
                let swap_to = random_usize(&self.rng, self.next_index);
                self.buffers.swap(swap_from, swap_to);
                self.next_index -= 1;
                if self.next_index == 0 {
                    self.next_index = len;
                }
                Some(swap_from)
            }
        }
    }
}
