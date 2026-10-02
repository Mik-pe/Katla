//! Application-level particle drive.
//!
//! Emitter updates prepare the acquired slot's data and dispatch dimensions.
//! The compiled graph executes the simulation and orders its draw consumers.

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
                match renderer.step_particle_system(delta_time) {
                    Ok((emit, simulate)) => {
                        self.frame_graph.set_particle_emit_workgroup_count(emit);
                        self.frame_graph
                            .set_particle_simulate_workgroup_count(simulate);
                    }
                    Err(e) => log::error!("Particle simulation step failed: {}", e),
                }
            }
        }
    }
}
