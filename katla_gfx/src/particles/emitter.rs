use super::*;

use types::EmitterState;

impl GlobalParticleSystem {
    pub fn create_emitter(&mut self, config: EmitterConfig) -> Result<EmitterHandle, String> {
        if self.emitter_pool.emitters.len() >= MAX_EMITTERS as usize {
            log::warn!(
                "Cannot create emitter: maximum emitter count ({}) reached",
                MAX_EMITTERS
            );
            return Err(format!("Maximum emitter count ({}) reached", MAX_EMITTERS));
        }

        let index = self
            .emitter_pool
            .free_slots
            .pop()
            .unwrap_or(self.emitter_pool.next_slot);
        if index >= self.emitter_pool.next_slot {
            self.emitter_pool.next_slot = index + 1;
        }

        if self.emitter_pool.emitters.len() <= index as usize {
            self.emitter_pool
                .emitters
                .resize(index as usize + 1, EmitterConfig::default());
        }
        if self.emitter_pool.emitter_states.len() <= index as usize {
            self.emitter_pool
                .emitter_states
                .resize(index as usize + 1, EmitterState::default());
        }

        self.emitter_pool.emitters[index as usize] = config;
        self.recompute_estimated_max_alive();

        self.emitter_pool.emitter_states[index as usize] = EmitterState::default();

        log::debug!(
            "Created particle emitter {} at position {:?}",
            index,
            config.position
        );

        Ok(EmitterHandle::new(index))
    }

    pub fn update_emitter(&mut self, handle: EmitterHandle, config: EmitterConfig) {
        if handle.index() < self.emitter_pool.emitters.len() as u32 {
            self.emitter_pool.emitters[handle.index() as usize] = config;
            self.recompute_estimated_max_alive();
        } else {
            warn!("Invalid emitter handle: {:?}", handle);
        }
    }

    pub fn burst(&mut self, handle: EmitterHandle, count: u32) -> Result<(), String> {
        if handle.index() < self.emitter_pool.emitter_states.len() as u32 {
            self.emitter_pool.emitter_states[handle.index() as usize].burst_count = count;
            log::debug!("Burst {} particles from emitter {}", count, handle.index());
            Ok(())
        } else {
            Err(format!("Invalid emitter handle: {:?}", handle))
        }
    }

    pub fn destroy_emitter(&mut self, handle: EmitterHandle, kill_all: bool) {
        if handle.index() < self.emitter_pool.emitters.len() as u32 {
            self.emitter_pool.emitters[handle.index() as usize] = EmitterConfig {
                emit_rate: 0.0,
                kill_all: if kill_all { 1 } else { 0 },
                ..Default::default()
            };
            if handle.index() < self.emitter_pool.emitter_states.len() as u32 {
                self.emitter_pool.emitter_states[handle.index() as usize] = EmitterState::default();
            }
            self.emitter_pool.free_slots.push(handle.index());
            log::info!(
                "Destroyed particle emitter {} (kill_all={})",
                handle.index(),
                kill_all
            );
        }
    }

    pub fn calculate_emit_count(&mut self, delta_time: f32) -> u32 {
        let mut total_emit = 0u32;

        for (emitter, state) in self
            .emitter_pool
            .emitters
            .iter()
            .zip(self.emitter_pool.emitter_states.iter_mut())
        {
            if emitter.emit_rate > 0.0 {
                state.emit_accumulator += emitter.emit_rate * delta_time;

                let to_emit = state.emit_accumulator as u32;
                state.emit_accumulator -= to_emit as f32;

                total_emit += to_emit;
            }
        }

        total_emit
    }

    pub(super) fn recompute_estimated_max_alive(&mut self) {
        self.estimated_max_alive = self
            .emitter_pool
            .emitters
            .iter()
            .filter(|e| e.emit_rate > 0.0)
            .map(|e| {
                let max_alive = e.emit_rate * e.base_lifetime * (1.0 + e.lifetime_variation);
                max_alive.ceil() as u32
            })
            .sum::<u32>()
            .min(self.max_particles);
    }
}
