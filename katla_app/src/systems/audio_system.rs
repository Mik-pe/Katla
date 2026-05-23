use std::collections::HashMap;
use std::sync::Arc;

use katla_audio::{AudioBuffer, AudioEngine, VoiceHandle, VoiceState};
use katla_ecs::World;

use crate::components::AudioEmitter;

pub struct AudioSystem {
    engine: AudioEngine,
    buffers: HashMap<String, Arc<AudioBuffer>>,
    active_voices: HashMap<katla_ecs::EntityId, VoiceHandle>,
    started: bool,
}

impl AudioSystem {
    pub fn new() -> Result<Self, String> {
        let engine = AudioEngine::new()?;
        Ok(AudioSystem {
            engine,
            buffers: HashMap::new(),
            active_voices: HashMap::new(),
            started: false,
        })
    }

    pub fn engine(&self) -> &AudioEngine {
        &self.engine
    }

    pub fn engine_mut(&mut self) -> &mut AudioEngine {
        &mut self.engine
    }

    pub fn load_buffer(&mut self, path: &str, _data: Vec<u8>) -> Result<(), String> {
        let buffer = katla_audio::load_audio(std::path::Path::new(path))?;
        self.buffers.insert(path.to_string(), Arc::new(buffer));
        Ok(())
    }

    fn get_or_load_buffer(&mut self, path: &str) -> Option<Arc<AudioBuffer>> {
        if let Some(buf) = self.buffers.get(path) {
            return Some(buf.clone());
        }

        match katla_audio::load_audio(std::path::Path::new(path)) {
            Ok(buffer) => {
                let arc = Arc::new(buffer);
                self.buffers.insert(path.to_string(), arc.clone());
                Some(arc)
            }
            Err(e) => {
                log::warn!("Failed to load audio '{}': {e}", path);
                None
            }
        }
    }

    pub fn update(&mut self, world: &mut World) {
        if !self.started {
            if let Err(e) = self.engine.resume() {
                log::warn!("Failed to start audio stream: {e}");
            }
            self.started = true;
        }

        for (entity, emitter) in world.query::<&AudioEmitter>() {
            if !emitter.playing {
                continue;
            }

            let already_playing = self
                .active_voices
                .get(&entity)
                .is_some_and(|h| h.state() == VoiceState::Playing);

            if already_playing {
                continue;
            }

            let Some(buffer) = self.get_or_load_buffer(&emitter.source_path) else {
                continue;
            };

            let handle = if emitter.looping {
                self.engine.play_looping(&buffer)
            } else {
                self.engine.play(&buffer)
            };

            handle.set_volume(emitter.volume);
            self.active_voices.insert(entity, handle);
        }

        let mut stopped_entities = Vec::new();
        for (entity, emitter) in world.query::<&mut AudioEmitter>() {
            if let Some(handle) = self.active_voices.get(&entity)
                && handle.state() == VoiceState::Stopped
            {
                emitter.playing = false;
                stopped_entities.push(entity);
            }
        }

        for entity in &stopped_entities {
            self.active_voices.remove(entity);
        }

        for entity in stopped_entities {
            if world
                .get_component::<AudioEmitter>(entity)
                .is_none_or(|e| !e.playing)
            {
                self.active_voices.remove(&entity);
            }
        }
    }

    pub fn stop_entity(&mut self, entity: katla_ecs::EntityId) {
        if let Some(handle) = self.active_voices.remove(&entity) {
            handle.stop();
        }
    }

    pub fn stop_all(&mut self) {
        self.engine.stop_all();
        self.active_voices.clear();
    }
}
