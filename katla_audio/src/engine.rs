use std::path::Path;
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::buffer::AudioBuffer;
use crate::command_queue::AudioCategoryValue;
use crate::effect::{AudioEffect, AuxBus};
use crate::error::AudioError;
use crate::mixer::AudioMixer;
use crate::streaming::StreamingDecoder;
use crate::streaming_voice::StreamingVoiceHandle;
use crate::voice::{VoiceHandle, VoiceId, VoiceState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioCategory {
    Master,
    Sfx,
    Music,
    Ambient,
}

impl AudioCategory {
    pub fn to_value(self) -> Option<AudioCategoryValue> {
        match self {
            AudioCategory::Master => None,
            AudioCategory::Sfx => Some(AudioCategoryValue::Sfx),
            AudioCategory::Music => Some(AudioCategoryValue::Music),
            AudioCategory::Ambient => Some(AudioCategoryValue::Ambient),
        }
    }
}

pub struct AudioEngine {
    mixer: Arc<AudioMixer>,
    #[allow(dead_code)]
    stream: cpal::Stream,
}

impl AudioEngine {
    pub fn new() -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(AudioError::DeviceNotFound(
                "No audio output device found".into(),
            ))?;

        let supported_config = device
            .supported_output_configs()
            .map_err(|e| {
                AudioError::DeviceNotFound(format!("Failed to query output configs: {e}"))
            })?
            .find(|c| c.channels() <= 2 && c.sample_format() == cpal::SampleFormat::F32)
            .or_else(|| {
                device
                    .supported_output_configs()
                    .ok()?
                    .find(|c| c.channels() <= 2)
            })
            .ok_or(AudioError::DeviceNotFound(
                "No suitable audio output config found".into(),
            ))?;

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
            .map_err(|e| AudioError::StreamError(format!("Failed to build audio stream: {e}")))?;

        stream
            .pause()
            .map_err(|e| AudioError::StreamError(format!("Failed to pause stream: {e}")))?;

        Ok(AudioEngine { mixer, stream })
    }

    pub fn play(&self, buffer: &Arc<AudioBuffer>) -> VoiceHandle {
        self.play_with_category(buffer, AudioCategory::Sfx)
    }

    pub fn play_with_category(
        &self,
        buffer: &Arc<AudioBuffer>,
        category: AudioCategory,
    ) -> VoiceHandle {
        let cat_val = category.to_value().unwrap_or(AudioCategoryValue::Sfx);
        let voice_id = self.mixer.play(buffer.clone(), cat_val);
        self.handle(voice_id)
    }

    pub fn play_looping(&self, buffer: &Arc<AudioBuffer>) -> VoiceHandle {
        self.play_looping_with_category(buffer, AudioCategory::Sfx)
    }

    pub fn play_looping_with_category(
        &self,
        buffer: &Arc<AudioBuffer>,
        category: AudioCategory,
    ) -> VoiceHandle {
        let cat_val = category.to_value().unwrap_or(AudioCategoryValue::Sfx);
        let voice_id = self.mixer.play_looping(buffer.clone(), cat_val);
        self.handle(voice_id)
    }

    pub fn handle(&self, id: VoiceId) -> VoiceHandle {
        VoiceHandle {
            id,
            mixer: self.mixer.clone(),
        }
    }

    pub fn resume(&self) -> Result<(), AudioError> {
        self.stream
            .play()
            .map_err(|e| AudioError::StreamError(format!("Failed to resume audio stream: {e}")))
    }

    pub fn pause(&self) -> Result<(), AudioError> {
        self.stream
            .pause()
            .map_err(|e| AudioError::StreamError(format!("Failed to pause audio stream: {e}")))
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

    pub fn set_category_volume(&self, category: AudioCategory, volume: f32) {
        if let Some(cat_val) = category.to_value() {
            self.mixer.set_category_volume(cat_val, volume);
        } else {
            self.mixer.set_master_volume(volume);
        }
    }

    pub fn category_volume(&self, category: AudioCategory) -> f32 {
        if let Some(cat_val) = category.to_value() {
            self.mixer.category_volume(cat_val)
        } else {
            self.mixer.master_volume()
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

    pub fn add_master_effect(&self, effect: Box<dyn AudioEffect + Send>) {
        self.mixer.add_master_effect(effect);
    }

    pub fn add_aux_bus(&self, bus: AuxBus) {
        self.mixer.add_aux_bus(bus);
    }

    pub fn play_streaming(&self, path: &Path) -> Result<StreamingVoiceHandle, AudioError> {
        self.play_streaming_with_category(path, AudioCategory::Music)
    }

    pub fn play_streaming_with_category(
        &self,
        path: &Path,
        category: AudioCategory,
    ) -> Result<StreamingVoiceHandle, AudioError> {
        let decoder = StreamingDecoder::open(path)?;
        let cat_val = category.to_value().unwrap_or(AudioCategoryValue::Music);
        let voice_id = self.mixer.play_streaming(decoder, false, cat_val)?;
        Ok(StreamingVoiceHandle {
            id: voice_id,
            mixer: self.mixer.clone(),
        })
    }

    pub fn play_streaming_looping(&self, path: &Path) -> Result<StreamingVoiceHandle, AudioError> {
        self.play_streaming_looping_with_category(path, AudioCategory::Music)
    }

    pub fn play_streaming_looping_with_category(
        &self,
        path: &Path,
        category: AudioCategory,
    ) -> Result<StreamingVoiceHandle, AudioError> {
        let decoder = StreamingDecoder::open(path)?;
        let cat_val = category.to_value().unwrap_or(AudioCategoryValue::Music);
        let voice_id = self.mixer.play_streaming(decoder, true, cat_val)?;
        Ok(StreamingVoiceHandle {
            id: voice_id,
            mixer: self.mixer.clone(),
        })
    }
}
