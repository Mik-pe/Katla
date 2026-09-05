//! Application-level particle drive.
//!
//! `step_particle_simulation` mirrors the Vulkan frame path's particle work
//! for every backend: the ECS emitter sync (`ParticleSystem::update`) has
//! already run by the time this is called (frame_loop), so this only performs
//! the per-frame CPU state update and dispatch sizing. The Metal renderer
//! dispatches the staged workgroups inline at the top of its own `render()`.

#[cfg(target_os = "macos")]
use super::Application;

#[cfg(target_os = "macos")]
impl Application {
    pub(crate) fn step_particle_simulation(&mut self, delta_time: f32) {
        match &mut self.renderer {
            katla_gfx::AnyRenderer::Vulkan(renderer) => {
                // The Vulkan frame graph drives the particle compute passes;
                // workgroup counts are set in the non-macos render_frame.
                let _ = (renderer, delta_time);
            }
            #[cfg(target_os = "macos")]
            katla_gfx::AnyRenderer::Metal(renderer) => {
                let driver = renderer.particle_emitter_driver_mut();
                if let Some(driver) = driver {
                    self.particle_system
                        .update(&mut self.world, driver, delta_time);
                }
                if let Err(e) = renderer.step_particle_system(delta_time) {
                    log::error!("Particle simulation step failed: {}", e);
                }
            }
        }
    }
}
