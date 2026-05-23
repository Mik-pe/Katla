use std::collections::HashMap;
use std::sync::Arc;

use katla_audio::{AudioBuffer, AudioEngine, VoiceHandle, VoiceState};
use katla_ecs::World;
use katla_math::Vec3;

use crate::components::{AudioEmitter, AudioListener, DistanceModel, TransformComponent};

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

    pub fn get_or_load_buffer(&mut self, path: &str) -> Option<Arc<AudioBuffer>> {
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

    fn find_listener(world: &World) -> (Vec3, Vec3) {
        let default = (Vec3::ZERO, -Vec3::Z_AXIS);
        for (entity, _listener) in world.query_ref::<&AudioListener>() {
            if let Some(transform) = world.get_component::<TransformComponent>(entity) {
                return (transform.transform.position, transform.transform.forward());
            }
        }
        default
    }

    pub fn update(&mut self, world: &mut World) {
        if !self.started {
            if let Err(e) = self.engine.resume() {
                log::warn!("Failed to start audio stream: {e}");
            }
            self.started = true;
        }

        let (listener_pos, listener_forward) = Self::find_listener(world);

        // Start new voices for emitters that aren't yet playing
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

        // Update spatial volume/pan and detect stopped voices
        let mut stopped_entities = Vec::new();
        let mut spatial_updates: Vec<(katla_ecs::EntityId, f32, f32)> = Vec::new();
        for (entity, emitter) in world.query_ref::<&AudioEmitter>() {
            let Some(handle) = self.active_voices.get(&entity) else {
                continue;
            };

            if handle.state() == VoiceState::Stopped {
                stopped_entities.push(entity);
                continue;
            }

            if emitter.spatial {
                if let Some(transform) = world.get_component::<TransformComponent>(entity) {
                    let emitter_pos = transform.transform.position;
                    let (spatial_volume, pan) = compute_spatialization(
                        emitter_pos,
                        listener_pos,
                        listener_forward,
                        emitter.min_distance,
                        emitter.max_distance,
                        emitter.rolloff_factor,
                        emitter.distance_model,
                    );
                    spatial_updates.push((entity, emitter.volume * spatial_volume, pan));
                }
            } else {
                handle.set_volume(emitter.volume);
            }
        }

        // Apply spatial updates outside the query borrow
        for (entity, volume, pan) in &spatial_updates {
            if let Some(handle) = self.active_voices.get(entity) {
                handle.set_volume(*volume);
                handle.set_pan(*pan);
            }
        }

        for entity in &stopped_entities {
            if let Some(handle) = self.active_voices.remove(entity) {
                handle.stop();
            }
        }

        // Mark emitter as no longer playing for stopped entities
        for entity in stopped_entities {
            if let Some(emitter) = world.get_component_mut::<AudioEmitter>(entity) {
                emitter.playing = false;
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

fn compute_spatialization(
    emitter_pos: Vec3,
    listener_pos: Vec3,
    listener_forward: Vec3,
    min_distance: f32,
    max_distance: f32,
    rolloff_factor: f32,
    distance_model: DistanceModel,
) -> (f32, f32) {
    let to_emitter = emitter_pos - listener_pos;
    let distance = to_emitter.length();

    let attenuation = match distance_model {
        DistanceModel::InverseClamped => {
            let d = distance.max(min_distance).min(max_distance);
            min_distance / (min_distance + rolloff_factor * (d - min_distance))
        }
        DistanceModel::Linear => {
            if distance <= min_distance {
                1.0
            } else if distance >= max_distance {
                0.0
            } else {
                1.0 - rolloff_factor * (distance - min_distance) / (max_distance - min_distance)
            }
        }
        DistanceModel::Exponential => {
            let d = distance.max(min_distance);
            (d / min_distance).powf(-rolloff_factor)
        }
    };

    let spatial_volume = attenuation.clamp(0.0, 1.0);

    let pan = if distance > 0.001 {
        let direction = to_emitter.normalize();
        let right = listener_forward.cross(Vec3::Y_AXIS);
        let right_len = right.length();
        if right_len > 0.001 {
            right.normalize().dot(direction).clamp(-1.0, 1.0)
        } else {
            0.0
        }
    } else {
        0.0
    };

    (spatial_volume, pan)
}
