use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::buffer::AudioBuffer;
use crate::mixer::AudioMixer;
use crate::voice::{VoiceHandle, VoiceId, VoiceState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioCategory {
    Master,
    Sfx,
    Music,
    Ambient,
}

pub struct AudioEngine {
    mixer: Arc<AudioMixer>,
    #[allow(dead_code)]
    stream: cpal::Stream,
    category_volumes: [f32; 3],
}

impl AudioEngine {
    pub fn new() -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or("No audio output device found")?;

        let supported_config = device
            .supported_output_configs()
            .map_err(|e| format!("Failed to query output configs: {e}"))?
            .find(|c| c.channels() <= 2 && c.sample_format() == cpal::SampleFormat::F32)
            .or_else(|| {
                device
                    .supported_output_configs()
                    .ok()?
                    .find(|c| c.channels() <= 2)
            })
            .ok_or("No suitable audio output config found")?;

        let config = supported_config.with_max_sample_rate().config();

        let sample_rate = config.sample_rate;
        let channels = config.channels;

        let mixer = Arc::new(AudioMixer::new(sample_rate, channels));

        let mixer_clone = mixer.clone();
        let stream = device
            .build_output_stream(
                &config,
                move |output: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    mixer_clone.render(output);
                },
                |err| {
                    log::error!("Audio stream error: {err}");
                },
                None,
            )
            .map_err(|e| format!("Failed to build audio stream: {e}"))?;

        stream
            .pause()
            .map_err(|e| format!("Failed to pause stream: {e}"))?;

        Ok(AudioEngine {
            mixer,
            stream,
            category_volumes: [1.0; 3],
        })
    }

    pub fn play(&self, buffer: &Arc<AudioBuffer>) -> VoiceHandle {
        let voice_id = self.mixer.play(buffer.clone());
        self.handle(voice_id)
    }

    pub fn play_looping(&self, buffer: &Arc<AudioBuffer>) -> VoiceHandle {
        let voice_id = self.mixer.play_looping(buffer.clone());
        self.handle(voice_id)
    }

    pub fn handle(&self, id: VoiceId) -> VoiceHandle {
        VoiceHandle {
            id,
            mixer: self.mixer.clone(),
        }
    }

    pub fn resume(&self) -> Result<(), String> {
        self.stream
            .play()
            .map_err(|e| format!("Failed to resume audio stream: {e}"))
    }

    pub fn pause(&self) -> Result<(), String> {
        self.stream
            .pause()
            .map_err(|e| format!("Failed to pause audio stream: {e}"))
    }

    pub fn stop_all(&self) {
        self.mixer.stop_all();
    }

    pub fn active_voice_count(&self) -> usize {
        self.mixer.active_voice_count()
    }

    pub fn set_master_volume(&self, volume: f32) {
        self.mixer.set_master_volume(volume);
    }

    pub fn master_volume(&self) -> f32 {
        self.mixer.master_volume()
    }

    pub fn set_category_volume(&mut self, category: AudioCategory, volume: f32) {
        let v = volume.clamp(0.0, 1.0);
        match category {
            AudioCategory::Master => self.mixer.set_master_volume(v),
            AudioCategory::Sfx => self.category_volumes[0] = v,
            AudioCategory::Music => self.category_volumes[1] = v,
            AudioCategory::Ambient => self.category_volumes[2] = v,
        }
    }

    pub fn category_volume(&self, category: AudioCategory) -> f32 {
        match category {
            AudioCategory::Master => self.mixer.master_volume(),
            AudioCategory::Sfx => self.category_volumes[0],
            AudioCategory::Music => self.category_volumes[1],
            AudioCategory::Ambient => self.category_volumes[2],
        }
    }

    pub fn voice_state(&self, id: VoiceId) -> VoiceState {
        self.mixer.voice_state(id)
    }

    pub fn sample_rate(&self) -> u32 {
        self.mixer.sample_rate()
    }

    pub fn channels(&self) -> u16 {
        self.mixer.channels()
    }
}
