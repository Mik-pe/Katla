use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, RwLock};

use crate::buffer::AudioBuffer;
use crate::voice::{Voice, VoiceId, VoiceState};

fn f32_to_bits(v: f32) -> u32 {
    v.to_bits()
}

fn bits_to_f32(v: u32) -> f32 {
    f32::from_bits(v)
}

pub struct AudioMixer {
    voices: RwLock<Vec<Option<Voice>>>,
    next_id: AtomicU32,
    master_volume: AtomicU32,
    sample_rate: u32,
    channels: u16,
}

impl AudioMixer {
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        AudioMixer {
            voices: RwLock::new(Vec::new()),
            next_id: AtomicU32::new(1),
            master_volume: AtomicU32::new(f32_to_bits(1.0)),
            sample_rate,
            channels,
        }
    }

    fn allocate_id(&self) -> VoiceId {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        VoiceId(id)
    }

    fn find_free_slot(voices: &[Option<Voice>]) -> Option<usize> {
        voices.iter().position(|v| v.is_none())
    }

    pub fn play(&self, buffer: Arc<AudioBuffer>) -> VoiceId {
        self.play_internal(buffer, false)
    }

    pub fn play_looping(&self, buffer: Arc<AudioBuffer>) -> VoiceId {
        self.play_internal(buffer, true)
    }

    fn play_internal(&self, buffer: Arc<AudioBuffer>, looping: bool) -> VoiceId {
        let id = self.allocate_id();
        let voice = Voice::new(id, buffer, looping);

        let mut voices = self.voices.write().unwrap();
        if let Some(slot) = Self::find_free_slot(&voices) {
            voices[slot] = Some(voice);
        } else {
            voices.push(Some(voice));
        }

        id
    }

    pub fn stop(&self, id: VoiceId) {
        let mut voices = self.voices.write().unwrap();
        for voice in voices.iter_mut() {
            if let Some(v) = voice
                && v.id() == id
            {
                voice.take();
                break;
            }
        }
    }

    pub fn stop_all(&self) {
        let mut voices = self.voices.write().unwrap();
        voices.clear();
    }

    pub fn set_voice_volume(&self, id: VoiceId, volume: f32) {
        let voices = self.voices.read().unwrap();
        for voice in voices.iter().flatten() {
            if voice.id() == id {
                voice.set_volume(volume);
                break;
            }
        }
    }

    pub fn voice_volume(&self, id: VoiceId) -> f32 {
        let voices = self.voices.read().unwrap();
        for voice in voices.iter().flatten() {
            if voice.id() == id {
                return voice.volume();
            }
        }
        0.0
    }

    pub fn voice_state(&self, id: VoiceId) -> VoiceState {
        let voices = self.voices.read().unwrap();
        for voice in voices.iter().flatten() {
            if voice.id() == id {
                return if voice.is_finished() {
                    VoiceState::Stopped
                } else {
                    VoiceState::Playing
                };
            }
        }
        VoiceState::Stopped
    }

    pub fn set_master_volume(&self, volume: f32) {
        self.master_volume
            .store(f32_to_bits(volume.clamp(0.0, 1.0)), Ordering::Relaxed);
    }

    pub fn master_volume(&self) -> f32 {
        bits_to_f32(self.master_volume.load(Ordering::Relaxed))
    }

    pub fn active_voice_count(&self) -> usize {
        let voices = self.voices.read().unwrap();
        voices.iter().filter(|v| v.is_some()).count()
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    pub fn render(&self, output: &mut [f32]) {
        output.fill(0.0f32);

        {
            let voices = self.voices.read().unwrap();
            for voice in voices.iter().flatten() {
                if !voice.is_finished() {
                    voice.mix_into(output, self.channels as usize, self.sample_rate);
                }
            }
        }

        let master = self.master_volume();
        if master != 1.0 {
            for sample in output.iter_mut() {
                *sample *= master;
            }
        }

        for sample in output.iter_mut() {
            *sample = sample.clamp(-1.0, 1.0);
        }

        {
            let mut voices = self.voices.write().unwrap();
            for i in 0..voices.len() {
                if let Some(v) = &voices[i]
                    && v.is_finished()
                    && !v.is_looping()
                {
                    voices[i] = None;
                }
            }
        }
    }
}
