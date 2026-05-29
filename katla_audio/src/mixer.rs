use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::buffer::AudioBuffer;
use crate::command_queue::{AudioCategoryValue, AudioCommand, CommandQueue};
use crate::effect::zone_reverb::ZoneReverbEffect;
use crate::effect::{AuxBus, EffectChain};
use crate::streaming_voice::StreamingVoice;
use crate::voice::{CategoryVolumes, Voice, VoiceId, VoiceState};

const MAX_VOICES: usize = 64;
const MAX_STREAMING_VOICES: usize = 8;

fn f32_to_bits(v: f32) -> u32 {
    v.to_bits()
}

fn bits_to_f32(v: u32) -> f32 {
    f32::from_bits(v)
}

#[derive(Clone, Copy)]
enum VoiceKind {
    Regular(usize),
    Streaming(usize),
}

struct MixerState {
    voices: Vec<Option<Voice>>,
    streaming_voices: Vec<Option<StreamingVoice>>,
    free_voice_slots: Vec<usize>,
    free_streaming_slots: Vec<usize>,
    voice_index: HashMap<VoiceId, VoiceKind>,
    master_effects: EffectChain,
    aux_buses: Vec<AuxBus>,
    sample_rate: u32,
    channels: u16,
}

impl MixerState {
    fn play_voice(
        &mut self,
        id: VoiceId,
        buffer: Arc<AudioBuffer>,
        looping: bool,
        category: AudioCategoryValue,
        category_volumes: Arc<CategoryVolumes>,
    ) -> bool {
        let slot = match self.free_voice_slots.pop() {
            Some(s) => s,
            None => {
                log::warn!("Voice pool full ({}), dropping play", MAX_VOICES);
                return false;
            }
        };
        if let Some(existing) = &mut self.voices[slot] {
            existing.reset(id, buffer, looping, category, category_volumes);
        } else {
            self.voices[slot] = Some(Voice::new(id, buffer, looping, category, category_volumes));
        }
        self.voice_index.insert(id, VoiceKind::Regular(slot));
        true
    }

    fn play_streaming_voice(
        &mut self,
        id: VoiceId,
        decoder: crate::streaming::StreamingDecoder,
        looping: bool,
        category: AudioCategoryValue,
        category_volumes: Arc<CategoryVolumes>,
    ) -> Result<bool, crate::error::AudioError> {
        let slot = match self.free_streaming_slots.pop() {
            Some(s) => s,
            None => {
                log::warn!(
                    "Streaming voice pool full ({}), dropping play",
                    MAX_STREAMING_VOICES
                );
                return Ok(false);
            }
        };
        if let Some(existing) = &mut self.streaming_voices[slot] {
            existing.reset(id, decoder, looping, category, category_volumes)?;
        } else {
            self.streaming_voices[slot] = Some(StreamingVoice::new(
                id,
                decoder,
                looping,
                category,
                category_volumes,
            )?);
        }
        self.voice_index.insert(id, VoiceKind::Streaming(slot));
        Ok(true)
    }

    fn stop(&mut self, id: VoiceId) {
        if let Some(kind) = self.voice_index.get(&id) {
            match kind {
                VoiceKind::Regular(slot) => {
                    if let Some(voice) = &self.voices[*slot] {
                        voice.begin_fade_out();
                    }
                }
                VoiceKind::Streaming(slot) => {
                    if let Some(voice) = &self.streaming_voices[*slot] {
                        voice.begin_fade_out();
                    }
                }
            }
        }
    }

    fn stop_all(&mut self) {
        for i in 0..self.voices.len() {
            self.voices[i] = None;
        }
        for i in 0..self.streaming_voices.len() {
            self.streaming_voices[i] = None;
        }
        self.free_voice_slots.clear();
        for i in (0..self.voices.len()).rev() {
            self.free_voice_slots.push(i);
        }
        self.free_streaming_slots.clear();
        for i in (0..self.streaming_voices.len()).rev() {
            self.free_streaming_slots.push(i);
        }
        self.voice_index.clear();
    }

    fn voice_slot(&self, id: VoiceId) -> Option<VoiceKind> {
        self.voice_index.get(&id).copied()
    }

    fn set_voice_volume(&self, id: VoiceId, volume: f32) {
        if let Some(kind) = self.voice_slot(id) {
            match kind {
                VoiceKind::Regular(slot) => {
                    if let Some(voice) = &self.voices[slot] {
                        voice.set_volume(volume);
                    }
                }
                VoiceKind::Streaming(slot) => {
                    if let Some(voice) = &self.streaming_voices[slot] {
                        voice.set_volume(volume);
                    }
                }
            }
        }
    }

    fn set_voice_volume_tweened(&self, id: VoiceId, volume: f32) {
        if let Some(kind) = self.voice_slot(id) {
            match kind {
                VoiceKind::Regular(slot) => {
                    if let Some(voice) = &self.voices[slot] {
                        voice.set_volume_tweened(volume);
                    }
                }
                VoiceKind::Streaming(slot) => {
                    if let Some(voice) = &self.streaming_voices[slot] {
                        voice.set_volume_tweened(volume);
                    }
                }
            }
        }
    }

    fn set_voice_pan(&self, id: VoiceId, pan: f32) {
        if let Some(kind) = self.voice_slot(id) {
            match kind {
                VoiceKind::Regular(slot) => {
                    if let Some(voice) = &self.voices[slot] {
                        voice.set_pan(pan);
                    }
                }
                VoiceKind::Streaming(slot) => {
                    if let Some(voice) = &self.streaming_voices[slot] {
                        voice.set_pan(pan);
                    }
                }
            }
        }
    }

    fn set_voice_pan_tweened(&self, id: VoiceId, pan: f32) {
        if let Some(kind) = self.voice_slot(id) {
            match kind {
                VoiceKind::Regular(slot) => {
                    if let Some(voice) = &self.voices[slot] {
                        voice.set_pan_tweened(pan);
                    }
                }
                VoiceKind::Streaming(slot) => {
                    if let Some(voice) = &self.streaming_voices[slot] {
                        voice.set_pan_tweened(pan);
                    }
                }
            }
        }
    }

    fn set_voice_pitch(&self, id: VoiceId, pitch: f32) {
        if let Some(kind) = self.voice_slot(id) {
            match kind {
                VoiceKind::Regular(slot) => {
                    if let Some(voice) = &self.voices[slot] {
                        voice.set_pitch(pitch);
                    }
                }
                VoiceKind::Streaming(slot) => {
                    if let Some(voice) = &self.streaming_voices[slot] {
                        voice.set_pitch(pitch);
                    }
                }
            }
        }
    }

    fn set_voice_pitch_tweened(&self, id: VoiceId, pitch: f32) {
        if let Some(kind) = self.voice_slot(id) {
            match kind {
                VoiceKind::Regular(slot) => {
                    if let Some(voice) = &self.voices[slot] {
                        voice.set_pitch_tweened(pitch);
                    }
                }
                VoiceKind::Streaming(slot) => {
                    if let Some(voice) = &self.streaming_voices[slot] {
                        voice.set_pitch_tweened(pitch);
                    }
                }
            }
        }
    }

    fn set_voice_occlusion(&self, id: VoiceId, occlusion: f32) {
        if let Some(kind) = self.voice_slot(id) {
            match kind {
                VoiceKind::Regular(slot) => {
                    if let Some(voice) = &self.voices[slot] {
                        voice.set_occlusion(occlusion);
                    }
                }
                VoiceKind::Streaming(_) => {
                    let _ = (id, occlusion);
                }
            }
        }
    }

    fn set_tween_speed(&mut self, id: VoiceId, speed: f32) {
        if let Some(kind) = self.voice_slot(id) {
            match kind {
                VoiceKind::Regular(slot) => {
                    if let Some(voice) = &mut self.voices[slot] {
                        voice.set_tween_speed(speed);
                    }
                }
                VoiceKind::Streaming(slot) => {
                    if let Some(voice) = &mut self.streaming_voices[slot] {
                        voice.set_tween_speed(speed);
                    }
                }
            }
        }
    }

    fn voice_volume(&self, id: VoiceId) -> f32 {
        if let Some(kind) = self.voice_slot(id) {
            match kind {
                VoiceKind::Regular(slot) => {
                    if let Some(voice) = &self.voices[slot] {
                        return voice.volume();
                    }
                }
                VoiceKind::Streaming(slot) => {
                    if let Some(voice) = &self.streaming_voices[slot] {
                        return voice.volume();
                    }
                }
            }
        }
        0.0
    }

    fn voice_position(&self, id: VoiceId) -> f32 {
        if let Some(VoiceKind::Regular(slot)) = self.voice_slot(id)
            && let Some(voice) = &self.voices[slot]
        {
            return voice.position();
        }
        0.0
    }

    fn streaming_voice_position(&self, id: VoiceId) -> f32 {
        if let Some(VoiceKind::Streaming(slot)) = self.voice_slot(id)
            && let Some(voice) = &self.streaming_voices[slot]
        {
            return voice.position();
        }
        0.0
    }

    fn seek_streaming_voice(&self, id: VoiceId, position: Duration) {
        if let Some(VoiceKind::Streaming(slot)) = self.voice_slot(id)
            && let Some(voice) = &self.streaming_voices[slot]
        {
            voice.seek(position);
        }
    }

    fn voice_state(&self, id: VoiceId) -> VoiceState {
        if let Some(kind) = self.voice_slot(id) {
            let finished = match kind {
                VoiceKind::Regular(slot) => self
                    .voices
                    .get(slot)
                    .and_then(|v| v.as_ref())
                    .map(|v| v.is_finished())
                    .unwrap_or(true),
                VoiceKind::Streaming(slot) => self
                    .streaming_voices
                    .get(slot)
                    .and_then(|v| v.as_ref())
                    .map(|v| v.is_finished())
                    .unwrap_or(true),
            };
            return if finished {
                VoiceState::Stopped
            } else {
                VoiceState::Playing
            };
        }
        VoiceState::Stopped
    }

    fn active_voice_count(&self) -> usize {
        self.voice_index.len()
    }
}

pub struct AudioMixer {
    state: Mutex<MixerState>,
    command_queue: Arc<CommandQueue>,
    next_id: AtomicU32,
    master_volume: AtomicU32,
    category_volumes: Arc<CategoryVolumes>,
    zone_reverb_decay: Arc<AtomicU32>,
    zone_reverb_wet: Arc<AtomicU32>,
    zone_reverb_dampening: Arc<AtomicU32>,
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
        let zone_reverb_decay = Arc::new(AtomicU32::new(0.0f32.to_bits()));
        let zone_reverb_wet = Arc::new(AtomicU32::new(0.0f32.to_bits()));
        let zone_reverb_dampening = Arc::new(AtomicU32::new(0.2f32.to_bits()));
        AudioMixer {
            state: Mutex::new(MixerState {
                voices: (0..MAX_VOICES).map(|_| None).collect(),
                streaming_voices: (0..MAX_STREAMING_VOICES).map(|_| None).collect(),
                free_voice_slots: (0..MAX_VOICES).rev().collect(),
                free_streaming_slots: (0..MAX_STREAMING_VOICES).rev().collect(),
                voice_index: HashMap::with_capacity(MAX_VOICES + MAX_STREAMING_VOICES),
                master_effects: EffectChain::new(),
                aux_buses: Vec::new(),
                sample_rate,
                channels,
            }),
            command_queue: Arc::new(CommandQueue::new()),
            next_id: AtomicU32::new(1),
            master_volume: AtomicU32::new(f32_to_bits(1.0)),
            category_volumes,
            zone_reverb_decay,
            zone_reverb_wet,
            zone_reverb_dampening,
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
        let mut state = self.state.lock().expect("AudioMixer state lock poisoned");
        state.play_voice(id, buffer, looping, category, self.category_volumes.clone());
        id
    }

    pub fn stop(&self, id: VoiceId) {
        let _ = self.command_queue.push(AudioCommand::Stop(id));
    }

    pub fn stop_all(&self) {
        let _ = self.command_queue.push(AudioCommand::StopAll);
    }

    pub fn set_voice_volume(&self, id: VoiceId, volume: f32) {
        let state = self.state.lock().expect("AudioMixer state lock poisoned");
        state.set_voice_volume(id, volume);
    }

    pub fn set_voice_volume_tweened(&self, id: VoiceId, volume: f32) {
        let state = self.state.lock().expect("AudioMixer state lock poisoned");
        state.set_voice_volume_tweened(id, volume);
    }

    pub fn voice_volume(&self, id: VoiceId) -> f32 {
        let state = self.state.lock().expect("AudioMixer state lock poisoned");
        state.voice_volume(id)
    }

    pub fn set_voice_pan(&self, id: VoiceId, pan: f32) {
        let state = self.state.lock().expect("AudioMixer state lock poisoned");
        state.set_voice_pan(id, pan);
    }

    pub fn set_voice_pan_tweened(&self, id: VoiceId, pan: f32) {
        let state = self.state.lock().expect("AudioMixer state lock poisoned");
        state.set_voice_pan_tweened(id, pan);
    }

    pub fn set_voice_pitch(&self, id: VoiceId, pitch: f32) {
        let state = self.state.lock().expect("AudioMixer state lock poisoned");
        state.set_voice_pitch(id, pitch);
    }

    pub fn set_voice_pitch_tweened(&self, id: VoiceId, pitch: f32) {
        let state = self.state.lock().expect("AudioMixer state lock poisoned");
        state.set_voice_pitch_tweened(id, pitch);
    }

    pub fn set_voice_occlusion(&self, id: VoiceId, occlusion: f32) {
        let state = self.state.lock().expect("AudioMixer state lock poisoned");
        state.set_voice_occlusion(id, occlusion);
    }

    pub fn set_voice_tween_speed(&self, id: VoiceId, speed: f32) {
        let mut state = self.state.lock().expect("AudioMixer state lock poisoned");
        state.set_tween_speed(id, speed);
    }

    pub fn voice_state(&self, id: VoiceId) -> VoiceState {
        let state = self.state.lock().expect("AudioMixer state lock poisoned");
        state.voice_state(id)
    }

    pub fn voice_position(&self, id: VoiceId) -> f32 {
        let state = self.state.lock().expect("AudioMixer state lock poisoned");
        state.voice_position(id)
    }

    pub fn streaming_voice_position(&self, id: VoiceId) -> f32 {
        let state = self.state.lock().expect("AudioMixer state lock poisoned");
        state.streaming_voice_position(id)
    }

    pub fn seek_streaming_voice(&self, id: VoiceId, position: Duration) {
        let state = self.state.lock().expect("AudioMixer state lock poisoned");
        state.seek_streaming_voice(id, position);
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
        let state = self.state.lock().expect("AudioMixer state lock poisoned");
        state.active_voice_count()
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    pub fn add_master_effect(&self, effect: Box<dyn crate::effect::AudioEffect + Send>) {
        let mut state = self.state.lock().expect("AudioMixer state lock poisoned");
        state.master_effects.add_effect(effect);
    }

    pub fn add_aux_bus(&self, bus: AuxBus) {
        let mut state = self.state.lock().expect("AudioMixer state lock poisoned");
        state.aux_buses.push(bus);
    }

    pub fn create_zone_reverb_bus(&self) {
        let effect = ZoneReverbEffect::new(
            self.sample_rate,
            self.zone_reverb_decay.clone(),
            self.zone_reverb_wet.clone(),
            self.zone_reverb_dampening.clone(),
        );
        let mut bus = AuxBus::new(1.0, 1.0);
        bus.add_effect(Box::new(effect));
        let mut state = self.state.lock().expect("AudioMixer state lock poisoned");
        state.aux_buses.push(bus);
    }

    pub fn set_zone_reverb(&self, decay: f32, wet: f32, dampening: f32) {
        self.zone_reverb_decay
            .store(decay.to_bits(), Ordering::Relaxed);
        self.zone_reverb_wet.store(wet.to_bits(), Ordering::Relaxed);
        self.zone_reverb_dampening
            .store(dampening.to_bits(), Ordering::Relaxed);
    }

    pub fn play_streaming(
        &self,
        decoder: crate::streaming::StreamingDecoder,
        looping: bool,
        category: AudioCategoryValue,
    ) -> Result<VoiceId, crate::error::AudioError> {
        let id = self.allocate_id();
        let mut state = self.state.lock().expect("AudioMixer state lock poisoned");
        state.play_streaming_voice(
            id,
            decoder,
            looping,
            category,
            self.category_volumes.clone(),
        )?;
        Ok(id)
    }

    pub fn set_streaming_voice_volume(&self, id: VoiceId, volume: f32) {
        let state = self.state.lock().expect("AudioMixer state lock poisoned");
        state.set_voice_volume(id, volume);
    }

    pub fn set_streaming_voice_volume_tweened(&self, id: VoiceId, volume: f32) {
        let state = self.state.lock().expect("AudioMixer state lock poisoned");
        state.set_voice_volume_tweened(id, volume);
    }

    pub fn streaming_voice_volume(&self, id: VoiceId) -> f32 {
        let state = self.state.lock().expect("AudioMixer state lock poisoned");
        state.voice_volume(id)
    }

    pub fn set_streaming_voice_pan(&self, id: VoiceId, pan: f32) {
        let state = self.state.lock().expect("AudioMixer state lock poisoned");
        state.set_voice_pan(id, pan);
    }

    pub fn set_streaming_voice_pan_tweened(&self, id: VoiceId, pan: f32) {
        let state = self.state.lock().expect("AudioMixer state lock poisoned");
        state.set_voice_pan_tweened(id, pan);
    }

    pub fn set_streaming_voice_pitch(&self, id: VoiceId, pitch: f32) {
        let state = self.state.lock().expect("AudioMixer state lock poisoned");
        state.set_voice_pitch(id, pitch);
    }

    pub fn set_streaming_voice_pitch_tweened(&self, id: VoiceId, pitch: f32) {
        let state = self.state.lock().expect("AudioMixer state lock poisoned");
        state.set_voice_pitch_tweened(id, pitch);
    }

    pub fn streaming_voice_state(&self, id: VoiceId) -> VoiceState {
        let state = self.state.lock().expect("AudioMixer state lock poisoned");
        state.voice_state(id)
    }

    pub fn set_streaming_voice_tween_speed(&self, id: VoiceId, speed: f32) {
        let mut state = self.state.lock().expect("AudioMixer state lock poisoned");
        state.set_tween_speed(id, speed);
    }

    fn process_commands(state: &mut MixerState, queue: &CommandQueue) {
        while let Some(cmd) = queue.pop() {
            match cmd {
                AudioCommand::Stop(id) => state.stop(id),
                AudioCommand::StopAll => state.stop_all(),
            }
        }
    }

    pub fn render(&self, output: &mut [f32]) {
        {
            let mut state = self.state.lock().expect("AudioMixer state lock poisoned");
            Self::process_commands(&mut state, &self.command_queue);

            output.fill(0.0f32);

            let channels = state.channels as usize;
            let sample_rate = state.sample_rate;

            for voice in state.voices.iter().flatten() {
                if !voice.is_finished() {
                    voice.tick_tweens();
                }
            }

            for voice in state.streaming_voices.iter().flatten() {
                if !voice.is_finished() {
                    voice.tick_tweens();
                }
            }

            for voice in state.voices.iter().flatten() {
                if !voice.is_finished() {
                    voice.mix_into(output, channels, sample_rate);
                }
            }

            for voice in state.streaming_voices.iter_mut().flatten() {
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

            let mut finished_voice_count = 0usize;
            let mut finished_voices: [(usize, VoiceId); MAX_VOICES] = [(0, VoiceId(0)); MAX_VOICES];
            for i in 0..state.voices.len() {
                if let Some(v) = &state.voices[i]
                    && v.is_finished()
                {
                    finished_voices[finished_voice_count] = (i, v.id());
                    finished_voice_count += 1;
                }
            }
            for &(i, id) in finished_voices.iter().take(finished_voice_count) {
                if state.voice_index.remove(&id).is_some() {
                    state.free_voice_slots.push(i);
                }
            }

            let mut finished_streaming_count = 0usize;
            let mut finished_streaming: [(usize, VoiceId); MAX_STREAMING_VOICES] =
                [(0, VoiceId(0)); MAX_STREAMING_VOICES];
            for i in 0..state.streaming_voices.len() {
                if let Some(v) = &state.streaming_voices[i]
                    && v.is_finished()
                {
                    finished_streaming[finished_streaming_count] = (i, v.id());
                    finished_streaming_count += 1;
                }
            }
            for &(i, id) in finished_streaming.iter().take(finished_streaming_count) {
                if state.voice_index.remove(&id).is_some() {
                    state.free_streaming_slots.push(i);
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
