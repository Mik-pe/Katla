use super::*;
use crate::error::RendererError;

impl GlobalParticleSystem {
    pub fn create_emitter(
        &mut self,
        config: EmitterConfig,
    ) -> Result<EmitterHandle, RendererError> {
        if self.emitter_pool.emitters.len() >= MAX_EMITTERS as usize {
            log::warn!(
                "Cannot create emitter: maximum emitter count ({}) reached",
                MAX_EMITTERS
            );
            return Err(RendererError::ResourceCreationFailed(format!(
                "Maximum emitter count ({}) reached",
                MAX_EMITTERS
            )));
        }

        let handle = self.emitter_pool.insert(config);
        self.recompute_estimated_max_alive();
        log::debug!(
            "Created particle emitter {} (generation {}) at position {:?}",
            handle.index(),
            handle.generation(),
            config.position
        );
        Ok(handle)
    }

    pub fn update_emitter(&mut self, handle: EmitterHandle, config: EmitterConfig) {
        if self.emitter_pool.update(handle, config) {
            self.recompute_estimated_max_alive();
        } else {
            warn!("Invalid emitter handle: {:?}", handle);
        }
    }

    pub fn burst(&mut self, handle: EmitterHandle, count: u32) -> Result<(), RendererError> {
        if self.emitter_pool.burst(handle, count) {
            log::debug!("Burst {} particles from emitter {}", count, handle.index());
            Ok(())
        } else {
            Err(RendererError::InvalidOperation(format!(
                "Invalid emitter handle: {:?}",
                handle
            )))
        }
    }

    pub fn destroy_emitter(&mut self, handle: EmitterHandle, kill_all: bool) {
        if self.emitter_pool.remove(handle, kill_all) {
            self.recompute_estimated_max_alive();
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
