use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use crate::buffer::AudioBuffer;
use crate::command_queue::{AudioCategoryValue, AudioCommand, CommandQueue};
use crate::effect::{AuxBus, EffectChain};
use crate::voice::{CategoryVolumes, Voice, VoiceId, VoiceState};

fn f32_to_bits(v: f32) -> u32 {
    v.to_bits()
}

fn bits_to_f32(v: u32) -> f32 {
    f32::from_bits(v)
}

struct MixerState {
    voices: Vec<Option<Voice>>,
    master_effects: EffectChain,
    aux_buses: Vec<AuxBus>,
    sample_rate: u32,
    channels: u16,
}

impl MixerState {
    fn find_free_slot(voices: &[Option<Voice>]) -> Option<usize> {
        voices.iter().position(|v| v.is_none())
    }

    fn add_voice(&mut self, voice: Voice) {
        if let Some(slot) = Self::find_free_slot(&self.voices) {
            self.voices[slot] = Some(voice);
        } else {
            self.voices.push(Some(voice));
        }
    }

    fn stop(&mut self, id: VoiceId) {
        for voice in self.voices.iter_mut() {
            if let Some(v) = voice
                && v.id() == id
            {
                *voice = None;
                break;
            }
        }
    }

    fn stop_all(&mut self) {
        self.voices.clear();
    }

    fn set_voice_volume(&self, id: VoiceId, volume: f32) {
        for voice in self.voices.iter().flatten() {
            if voice.id() == id {
                voice.set_volume(volume);
                break;
            }
        }
    }

    fn set_voice_volume_tweened(&self, id: VoiceId, volume: f32) {
        for voice in self.voices.iter().flatten() {
            if voice.id() == id {
                voice.set_volume_tweened(volume);
                break;
            }
        }
    }

    fn set_voice_pan(&self, id: VoiceId, pan: f32) {
        for voice in self.voices.iter().flatten() {
            if voice.id() == id {
                voice.set_pan(pan);
                break;
            }
        }
    }

    fn set_voice_pan_tweened(&self, id: VoiceId, pan: f32) {
        for voice in self.voices.iter().flatten() {
            if voice.id() == id {
                voice.set_pan_tweened(pan);
                break;
            }
        }
    }

    fn set_voice_pitch(&self, id: VoiceId, pitch: f32) {
        for voice in self.voices.iter().flatten() {
            if voice.id() == id {
                voice.set_pitch(pitch);
                break;
            }
        }
    }

    fn set_voice_pitch_tweened(&self, id: VoiceId, pitch: f32) {
        for voice in self.voices.iter().flatten() {
            if voice.id() == id {
                voice.set_pitch_tweened(pitch);
                break;
            }
        }
    }

    fn voice_volume(&self, id: VoiceId) -> f32 {
        for voice in self.voices.iter().flatten() {
            if voice.id() == id {
                return voice.volume();
            }
        }
        0.0
    }

    fn voice_state(&self, id: VoiceId) -> VoiceState {
        for voice in self.voices.iter().flatten() {
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

    fn active_voice_count(&self) -> usize {
        self.voices.iter().filter(|v| v.is_some()).count()
    }
}

pub struct AudioMixer {
    state: Mutex<MixerState>,
    command_queue: Arc<CommandQueue>,
    next_id: AtomicU32,
    master_volume: AtomicU32,
    category_volumes: Arc<CategoryVolumes>,
    sample_rate: u32,
    channels: u16,
}

fn category_index(cat: AudioCategoryValue) -> usize {
    match cat {
        AudioCategoryValue::Sfx => 0,
        AudioCategoryValue::Music => 1,
        AudioCategoryValue::Ambient => 2,
    }
}

impl AudioMixer {
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        let category_volumes = Arc::new(CategoryVolumes::new());
        AudioMixer {
            state: Mutex::new(MixerState {
                voices: Vec::new(),
                master_effects: EffectChain::new(),
                aux_buses: Vec::new(),
                sample_rate,
                channels,
            }),
            command_queue: Arc::new(CommandQueue::new()),
            next_id: AtomicU32::new(1),
            master_volume: AtomicU32::new(f32_to_bits(1.0)),
            category_volumes,
            sample_rate,
            channels,
        }
    }

    fn allocate_id(&self) -> VoiceId {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        VoiceId(id)
    }

    pub fn category_volume(&self, category: AudioCategoryValue) -> f32 {
        bits_to_f32(self.category_volumes.0[category_index(category)].load(Ordering::Relaxed))
    }

    pub fn play(&self, buffer: Arc<AudioBuffer>, category: AudioCategoryValue) -> VoiceId {
        self.play_internal(buffer, false, category)
    }

    pub fn play_looping(&self, buffer: Arc<AudioBuffer>, category: AudioCategoryValue) -> VoiceId {
        self.play_internal(buffer, true, category)
    }

    fn play_internal(
        &self,
        buffer: Arc<AudioBuffer>,
        looping: bool,
        category: AudioCategoryValue,
    ) -> VoiceId {
        let id = self.allocate_id();
        let voice = Voice::new(id, buffer, looping, category, self.category_volumes.clone());

        let mut state = self.state.lock().unwrap();
        state.add_voice(voice);

        id
    }

    pub fn stop(&self, id: VoiceId) {
        let _ = self.command_queue.push(AudioCommand::Stop(id));
    }

    pub fn stop_all(&self) {
        let _ = self.command_queue.push(AudioCommand::StopAll);
    }

    pub fn set_voice_volume(&self, id: VoiceId, volume: f32) {
        let state = self.state.lock().unwrap();
        state.set_voice_volume(id, volume);
    }

    pub fn set_voice_volume_tweened(&self, id: VoiceId, volume: f32) {
        let state = self.state.lock().unwrap();
        state.set_voice_volume_tweened(id, volume);
    }

    pub fn voice_volume(&self, id: VoiceId) -> f32 {
        let state = self.state.lock().unwrap();
        state.voice_volume(id)
    }

    pub fn set_voice_pan(&self, id: VoiceId, pan: f32) {
        let state = self.state.lock().unwrap();
        state.set_voice_pan(id, pan);
    }

    pub fn set_voice_pan_tweened(&self, id: VoiceId, pan: f32) {
        let state = self.state.lock().unwrap();
        state.set_voice_pan_tweened(id, pan);
    }

    pub fn set_voice_pitch(&self, id: VoiceId, pitch: f32) {
        let state = self.state.lock().unwrap();
        state.set_voice_pitch(id, pitch);
    }

    pub fn set_voice_pitch_tweened(&self, id: VoiceId, pitch: f32) {
        let state = self.state.lock().unwrap();
        state.set_voice_pitch_tweened(id, pitch);
    }

    pub fn voice_state(&self, id: VoiceId) -> VoiceState {
        let state = self.state.lock().unwrap();
        state.voice_state(id)
    }

    pub fn set_master_volume(&self, volume: f32) {
        self.master_volume
            .store(f32_to_bits(volume.clamp(0.0, 1.0)), Ordering::Relaxed);
    }

    pub fn master_volume(&self) -> f32 {
        bits_to_f32(self.master_volume.load(Ordering::Relaxed))
    }

    pub fn set_category_volume(&self, category: AudioCategoryValue, volume: f32) {
        self.category_volumes.0[category_index(category)]
            .store(f32_to_bits(volume.clamp(0.0, 1.0)), Ordering::Relaxed);
    }

    pub fn active_voice_count(&self) -> usize {
        let state = self.state.lock().unwrap();
        state.active_voice_count()
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    pub fn add_master_effect(&self, effect: Box<dyn crate::effect::AudioEffect + Send>) {
        let mut state = self.state.lock().unwrap();
        state.master_effects.add_effect(effect);
    }

    pub fn add_aux_bus(&self, bus: AuxBus) {
        let mut state = self.state.lock().unwrap();
        state.aux_buses.push(bus);
    }

    fn process_commands(state: &mut MixerState, queue: &CommandQueue) {
        while let Some(cmd) = queue.pop() {
            match cmd {
                AudioCommand::Stop(id) => state.stop(id),
                AudioCommand::StopAll => state.stop_all(),
                AudioCommand::SetVolume(id, vol) => state.set_voice_volume(id, vol),
                AudioCommand::SetPan(id, pan) => state.set_voice_pan(id, pan),
                AudioCommand::SetPitch(id, pitch) => state.set_voice_pitch(id, pitch),
                AudioCommand::SetMasterVolume(_) | AudioCommand::SetCategoryVolume(_, _) => {
                    // These use atomics, no state mutation needed
                }
            }
        }
    }

    pub fn render(&self, output: &mut [f32]) {
        {
            let mut state = self.state.lock().unwrap();
            Self::process_commands(&mut state, &self.command_queue);

            output.fill(0.0f32);

            let channels = state.channels as usize;
            let sample_rate = state.sample_rate;

            for voice in state.voices.iter().flatten() {
                if !voice.is_finished() {
                    voice.tick_tweens();
                }
            }

            for voice in state.voices.iter().flatten() {
                if !voice.is_finished() {
                    voice.mix_into(output, channels, sample_rate);
                }
            }

            for bus in &mut state.aux_buses {
                bus.accumulate(output);
            }

            for bus in &mut state.aux_buses {
                bus.process_effects(channels);
                bus.mix_into(output);
            }

            state.master_effects.process(output, channels);

            for i in 0..state.voices.len() {
                if let Some(v) = &state.voices[i]
                    && v.is_finished()
                    && !v.is_looping()
                {
                    state.voices[i] = None;
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
    }
}
