use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::buffer::AudioBuffer;
use crate::command_queue::AudioCategoryValue;
use crate::effect::{AudioEffect, AuxBus};
use crate::error::AudioError;
use crate::mixer::AudioMixer;
use crate::streaming::StreamingDecoder;
use crate::streaming_voice::StreamingVoiceHandle;
use crate::voice::{AuxBusId, VoiceHandle, VoiceId, VoicePriority, VoiceState};

const MAX_RECOVERY_ATTEMPTS: u32 = 3;

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
    stream: Option<cpal::Stream>,
    stream_error_flag: Arc<AtomicBool>,
    recovery_count: u32,
    was_playing: bool,
}

impl AudioEngine {
    pub fn new() -> Result<Self, AudioError> {
        let stream_error_flag = Arc::new(AtomicBool::new(false));
        let flag_clone = stream_error_flag.clone();

        let (mixer, stream) = Self::build_stream(move || flag_clone.clone())?;

        stream
            .pause()
            .map_err(|e| AudioError::StreamError(format!("Failed to pause stream: {e}")))?;

        Ok(AudioEngine {
            mixer,
            stream: Some(stream),
            stream_error_flag,
            recovery_count: 0,
            was_playing: false,
        })
    }

    fn build_stream(
        error_flag_factory: impl FnOnce() -> Arc<AtomicBool>,
    ) -> Result<(Arc<AudioMixer>, cpal::Stream), AudioError> {
        let host = cpal::default_host();

        let device = host.default_output_device().ok_or_else(|| {
            let host_id = host.id().name();
            AudioError::DeviceNotFound(format!(
                "No default output device available on host '{host_id}'. \
                 Check that an audio device is connected and enabled in system settings."
            ))
        })?;

        let device_name = device
            .description()
            .map(|d| d.name().to_string())
            .unwrap_or_else(|_| "<unknown>".into());

        let supported_config = device
            .supported_output_configs()
            .map_err(|e| {
                let msg = e.to_string();
                if msg.to_lowercase().contains("permission")
                    || msg.to_lowercase().contains("access")
                    || msg.to_lowercase().contains("denied")
                {
                    AudioError::DeviceAccessDenied(format!(
                        "Permission denied querying configs for device '{device_name}': {e}. \
                         Grant microphone/audio access in System Settings > Privacy & Security."
                    ))
                } else {
                    AudioError::DeviceNotFound(format!(
                        "Failed to query output configs for device '{device_name}': {e}"
                    ))
                }
            })?
            .find(|c| c.channels() <= 2 && c.sample_format() == cpal::SampleFormat::F32)
            .or_else(|| {
                device
                    .supported_output_configs()
                    .ok()?
                    .find(|c| c.channels() <= 2)
            })
            .ok_or_else(|| {
                AudioError::FormatUnsupported(format!(
                    "Device '{device_name}' does not support stereo F32 or I16 output. \
                     Audio engine requires <= 2 channel output."
                ))
            })?;

        let config = supported_config.with_max_sample_rate().config();
        let sample_rate = config.sample_rate;
        let channels = config.channels;

        let mixer = Arc::new(AudioMixer::new(sample_rate, channels));

        let mixer_clone = mixer.clone();
        let error_flag = error_flag_factory();
        let stream = device
            .build_output_stream(
                &config,
                move |output: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    mixer_clone.render(output);
                },
                move |err| {
                    log::error!("Audio stream error: {err}");
                    error_flag.store(true, Ordering::Relaxed);
                },
                None,
            )
            .map_err(|e| {
                let kind = e.to_string();
                if kind.to_lowercase().contains("permission")
                    || kind.to_lowercase().contains("access")
                    || kind.to_lowercase().contains("denied")
                {
                    AudioError::DeviceAccessDenied(format!(
                        "Permission denied creating stream on device '{device_name}': {e}. \
                         Grant audio access in System Settings > Privacy & Security."
                    ))
                } else {
                    AudioError::StreamError(format!(
                        "Failed to build audio stream on device '{device_name}' \
                         ({sample_rate}Hz, {channels}ch): {e}"
                    ))
                }
            })?;

        Ok((mixer, stream))
    }

    pub fn play(&self, buffer: &Arc<AudioBuffer>) -> VoiceHandle {
        self.play_with_category(buffer, AudioCategory::Sfx)
    }

    pub fn play_with_category(
        &self,
        buffer: &Arc<AudioBuffer>,
        category: AudioCategory,
    ) -> VoiceHandle {
        self.play_with_category_and_priority(buffer, category, VoicePriority::default())
    }

    pub fn play_with_priority(
        &self,
        buffer: &Arc<AudioBuffer>,
        priority: VoicePriority,
    ) -> VoiceHandle {
        self.play_with_category_and_priority(buffer, AudioCategory::Sfx, priority)
    }

    pub fn play_with_category_and_priority(
        &self,
        buffer: &Arc<AudioBuffer>,
        category: AudioCategory,
        priority: VoicePriority,
    ) -> VoiceHandle {
        let cat_val = category.to_value().unwrap_or(AudioCategoryValue::Sfx);
        let voice_id = self.mixer.play(buffer.clone(), cat_val, priority);
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
        self.play_looping_with_category_and_priority(buffer, category, VoicePriority::default())
    }

    pub fn play_looping_with_priority(
        &self,
        buffer: &Arc<AudioBuffer>,
        priority: VoicePriority,
    ) -> VoiceHandle {
        self.play_looping_with_category_and_priority(buffer, AudioCategory::Sfx, priority)
    }

    pub fn play_looping_with_category_and_priority(
        &self,
        buffer: &Arc<AudioBuffer>,
        category: AudioCategory,
        priority: VoicePriority,
    ) -> VoiceHandle {
        let cat_val = category.to_value().unwrap_or(AudioCategoryValue::Sfx);
        let voice_id = self.mixer.play_looping(buffer.clone(), cat_val, priority);
        self.handle(voice_id)
    }

    pub fn handle(&self, id: VoiceId) -> VoiceHandle {
        VoiceHandle {
            id,
            mixer: self.mixer.clone(),
        }
    }

    pub fn resume(&self) -> Result<(), AudioError> {
        if let Some(ref stream) = self.stream {
            stream.play().map_err(|e| {
                AudioError::StreamError(format!("Failed to resume audio stream: {e}"))
            })?;
        }
        Ok(())
    }

    pub fn pause(&self) -> Result<(), AudioError> {
        if let Some(ref stream) = self.stream {
            stream.pause().map_err(|e| {
                AudioError::StreamError(format!("Failed to pause audio stream: {e}"))
            })?;
        }
        Ok(())
    }

    pub fn try_recover_stream(&mut self) -> Result<bool, AudioError> {
        if !self.stream_error_flag.load(Ordering::Relaxed) {
            return Ok(true);
        }

        if self.recovery_count >= MAX_RECOVERY_ATTEMPTS {
            log::error!(
                "Stream recovery abandoned after {} attempts",
                MAX_RECOVERY_ATTEMPTS
            );
            return Ok(false);
        }

        self.recovery_count += 1;
        log::warn!(
            "Attempting stream recovery (attempt {}/{})",
            self.recovery_count,
            MAX_RECOVERY_ATTEMPTS
        );

        let flag_clone = self.stream_error_flag.clone();
        match Self::build_stream(move || flag_clone) {
            Ok((_mixer, new_stream)) => {
                self.stream_error_flag.store(false, Ordering::Relaxed);
                self.stream = Some(new_stream);

                if self.was_playing
                    && let Some(ref stream) = self.stream
                    && let Err(e) = stream.play()
                {
                    log::error!("Failed to resume recovered stream: {e}");
                    return Ok(false);
                }

                self.recovery_count = 0;
                log::info!("Stream recovery succeeded");
                Ok(true)
            }
            Err(e) => {
                log::error!("Stream recovery failed: {e}");
                Ok(false)
            }
        }
    }

    pub fn check_and_recover(&mut self) -> Result<bool, AudioError> {
        let recovered = self.try_recover_stream()?;
        if recovered {
            self.was_playing = self.stream.is_some();
        }
        Ok(recovered)
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

    pub fn add_aux_bus(&self, bus: AuxBus) -> AuxBusId {
        self.mixer.add_aux_bus(bus)
    }

    pub fn create_zone_reverb_bus(&self) -> AuxBusId {
        self.mixer.create_zone_reverb_bus()
    }

    pub fn set_zone_reverb(&self, decay: f32, wet: f32, dampening: f32) {
        self.mixer.set_zone_reverb(decay, wet, dampening);
    }

    pub fn play_streaming(&self, path: &Path) -> Result<StreamingVoiceHandle, AudioError> {
        self.play_streaming_with_category(path, AudioCategory::Music)
    }

    pub fn play_streaming_with_category(
        &self,
        path: &Path,
        category: AudioCategory,
    ) -> Result<StreamingVoiceHandle, AudioError> {
        self.play_streaming_with_category_and_priority(path, category, VoicePriority::default())
    }

    pub fn play_streaming_with_priority(
        &self,
        path: &Path,
        priority: VoicePriority,
    ) -> Result<StreamingVoiceHandle, AudioError> {
        self.play_streaming_with_category_and_priority(path, AudioCategory::Music, priority)
    }

    pub fn play_streaming_with_category_and_priority(
        &self,
        path: &Path,
        category: AudioCategory,
        priority: VoicePriority,
    ) -> Result<StreamingVoiceHandle, AudioError> {
        let decoder = StreamingDecoder::open(path)?;
        let cat_val = category.to_value().unwrap_or(AudioCategoryValue::Music);
        let voice_id = self
            .mixer
            .play_streaming(decoder, false, cat_val, priority)?;
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
        self.play_streaming_looping_with_category_and_priority(
            path,
            category,
            VoicePriority::default(),
        )
    }

    pub fn play_streaming_looping_with_priority(
        &self,
        path: &Path,
        priority: VoicePriority,
    ) -> Result<StreamingVoiceHandle, AudioError> {
        self.play_streaming_looping_with_category_and_priority(path, AudioCategory::Music, priority)
    }

    pub fn play_streaming_looping_with_category_and_priority(
        &self,
        path: &Path,
        category: AudioCategory,
        priority: VoicePriority,
    ) -> Result<StreamingVoiceHandle, AudioError> {
        let decoder = StreamingDecoder::open(path)?;
        let cat_val = category.to_value().unwrap_or(AudioCategoryValue::Music);
        let voice_id = self
            .mixer
            .play_streaming(decoder, true, cat_val, priority)?;
        Ok(StreamingVoiceHandle {
            id: voice_id,
            mixer: self.mixer.clone(),
        })
    }
}
