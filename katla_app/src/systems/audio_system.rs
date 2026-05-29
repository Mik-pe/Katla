use std::collections::HashMap;
use std::sync::Arc;

use katla_audio::{AudioBuffer, AudioEngine, SoundCue, VoiceHandle, VoiceState};
use katla_ecs::World;
use katla_math::Vec3;

use crate::components::{
    AudioEmitter, AudioListener, DistanceModel, ReverbZone, TransformComponent,
};

/// Maximum occlusion factor (0.0 = not occluded, 1.0 = fully occluded).
const MAX_OCCLUSION: f32 = 0.85;

/// How much of the ray distance must be left after a hit to count as occlusion.
/// A hit very close to the listener barely occludes.
const OCCLUSION_MIN_RATIO: f32 = 0.1;

pub struct AudioSystem {
    engine: AudioEngine,
    buffers: HashMap<String, Arc<AudioBuffer>>,
    cues: HashMap<String, SoundCue>,
    active_voices: HashMap<katla_ecs::EntityId, VoiceHandle>,
    prev_listener_pos: Option<Vec3>,
    prev_emitter_positions: HashMap<katla_ecs::EntityId, Vec3>,
    started: bool,
}

impl AudioSystem {
    pub fn new() -> Result<Self, katla_audio::AudioError> {
        let engine = AudioEngine::new()?;
        engine.create_zone_reverb_bus();
        Ok(AudioSystem {
            engine,
            buffers: HashMap::new(),
            cues: HashMap::new(),
            active_voices: HashMap::new(),
            prev_listener_pos: None,
            prev_emitter_positions: HashMap::new(),
            started: false,
        })
    }

    pub fn engine(&self) -> &AudioEngine {
        &self.engine
    }

    pub fn engine_mut(&mut self) -> &mut AudioEngine {
        &mut self.engine
    }

    pub fn register_cue(&mut self, name: impl Into<String>, cue: SoundCue) {
        self.cues.insert(name.into(), cue);
    }

    pub fn play_cue(&mut self, name: &str) -> Option<VoiceHandle> {
        let cue = self.cues.get_mut(name)?;
        cue.play(&self.engine)
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

    fn find_listener(world: &World) -> (Vec3, Vec3, Vec3) {
        let default = (Vec3::ZERO, -Vec3::Z_AXIS, Vec3::Y_AXIS);
        for (entity, _listener) in world.query_ref::<&AudioListener>() {
            if let Some(transform) = world.get_component::<TransformComponent>(entity) {
                return (
                    transform.transform.position,
                    transform.transform.forward(),
                    transform.transform.up(),
                );
            }
        }
        default
    }

    pub fn update(&mut self, world: &mut World, dt: f32) {
        if !self.started {
            if let Err(e) = self.engine.resume() {
                log::warn!("Failed to start audio stream: {e}");
            }
            self.started = true;
        }

        let (listener_pos, listener_forward, listener_up) = Self::find_listener(world);
        let listener_vel = self
            .prev_listener_pos
            .map_or(Vec3::ZERO, |prev| (listener_pos - prev) / dt.max(0.001));
        self.prev_listener_pos = Some(listener_pos);

        // Update reverb zones — blend parameters from all zones containing the listener
        self.update_reverb_zones(world, listener_pos);

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
        let mut spatial_updates: Vec<(katla_ecs::EntityId, f32, f32, f32, f32)> = Vec::new();
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
                        listener_up,
                        emitter.min_distance,
                        emitter.max_distance,
                        emitter.rolloff_factor,
                        emitter.distance_model,
                    );

                    let emitter_vel = self
                        .prev_emitter_positions
                        .get(&entity)
                        .map_or(Vec3::ZERO, |prev| (emitter_pos - *prev) / dt.max(0.001));
                    let doppler_pitch =
                        compute_doppler(emitter_pos, listener_pos, emitter_vel, listener_vel);
                    self.prev_emitter_positions.insert(entity, emitter_pos);

                    // Compute occlusion via physics raycast
                    let occlusion = compute_occlusion(world, emitter_pos, listener_pos);

                    spatial_updates.push((
                        entity,
                        emitter.volume * spatial_volume,
                        pan,
                        doppler_pitch,
                        occlusion,
                    ));
                }
            } else {
                handle.set_volume(emitter.volume);
            }
        }

        for (entity, volume, pan, doppler_pitch, occlusion) in &spatial_updates {
            if let Some(handle) = self.active_voices.get(entity) {
                handle.set_volume_tweened(*volume);
                handle.set_pan_tweened(*pan);
                handle.set_pitch(*doppler_pitch);
                handle.set_occlusion(*occlusion);
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

    fn update_reverb_zones(&self, world: &World, listener_pos: Vec3) {
        let mut total_decay = 0.0f32;
        let mut total_wet = 0.0f32;
        let mut total_dampening = 0.0f32;
        let mut count = 0usize;

        let lp = [listener_pos.x(), listener_pos.y(), listener_pos.z()];

        for (entity, zone) in world.query_ref::<&ReverbZone>() {
            if let Some(transform) = world.get_component::<TransformComponent>(entity) {
                let pos = [
                    transform.transform.position.x(),
                    transform.transform.position.y(),
                    transform.transform.position.z(),
                ];
                if zone.contains(&pos, &lp) {
                    total_decay += zone.decay;
                    total_wet += zone.wet;
                    total_dampening += zone.dampening;
                    count += 1;
                }
            }
        }

        if count > 0 {
            let inv = 1.0 / count as f32;
            self.engine
                .set_zone_reverb(total_decay * inv, total_wet * inv, total_dampening * inv);
        } else {
            self.engine.set_zone_reverb(0.0, 0.0, 0.2);
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

    pub fn process_script_audio_commands(
        &mut self,
        commands: &mut Vec<katla_script::ScriptCommand>,
    ) {
        for cmd in commands.drain(..) {
            match cmd {
                katla_script::ScriptCommand::PlaySound {
                    path,
                    volume,
                    looping,
                } => {
                    if let Some(buffer) = self.get_or_load_buffer(&path) {
                        let handle = if looping {
                            self.engine.play_looping(&buffer)
                        } else {
                            self.engine.play(&buffer)
                        };
                        handle.set_volume(volume);
                    }
                }
                katla_script::ScriptCommand::PlaySoundAt {
                    path,
                    position: _,
                    volume,
                    looping,
                } => {
                    if let Some(buffer) = self.get_or_load_buffer(&path) {
                        let handle = if looping {
                            self.engine.play_looping(&buffer)
                        } else {
                            self.engine.play(&buffer)
                        };
                        handle.set_volume(volume);
                        // TODO: spatial positioning will be applied once
                        // play_sound_at creates a tracked AudioEmitter
                    }
                }
                katla_script::ScriptCommand::PlaySoundCue { cue_name } => {
                    self.play_cue(&cue_name);
                }
                _ => {}
            }
        }
    }
}

fn compute_spatialization(
    emitter_pos: Vec3,
    listener_pos: Vec3,
    listener_forward: Vec3,
    listener_up: Vec3,
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
        let right = listener_forward.cross(listener_up);
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

fn compute_doppler(
    emitter_pos: Vec3,
    listener_pos: Vec3,
    emitter_vel: Vec3,
    listener_vel: Vec3,
) -> f32 {
    const SPEED_OF_SOUND: f32 = 343.0;

    let to_listener = listener_pos - emitter_pos;
    let dist = to_listener.length();
    if dist < 0.001 {
        return 1.0;
    }

    let direction = to_listener / dist;
    let emitter_vel_radial = emitter_vel.dot(direction);
    let listener_vel_radial = listener_vel.dot(direction);

    let denominator = SPEED_OF_SOUND - listener_vel_radial + emitter_vel_radial;
    if denominator.abs() < 0.001 {
        return 1.0;
    }

    (SPEED_OF_SOUND / denominator).clamp(0.5, 2.0)
}

fn compute_occlusion(world: &World, emitter_pos: Vec3, listener_pos: Vec3) -> f32 {
    let physics = match world.get_resource::<katla_physics::PhysicsWorld>() {
        Some(p) => p,
        None => return 0.0,
    };

    let to_listener = listener_pos - emitter_pos;
    let distance = to_listener.length();
    if distance < 0.001 {
        return 0.0;
    }

    let direction = to_listener / distance;

    // Cast a ray slightly short of the listener to avoid self-intersection
    let max_distance = distance * 0.99;

    if let Some(hit) = physics.raycast(emitter_pos, direction, max_distance) {
        // Occlusion is based on how far along the ray the hit is.
        // A hit close to the emitter = heavily occluded.
        // A hit close to the listener = barely occluded.
        let ratio = hit.distance / distance;

        (1.0 - ratio).clamp(OCCLUSION_MIN_RATIO, 1.0) * MAX_OCCLUSION
    } else {
        0.0
    }
}
