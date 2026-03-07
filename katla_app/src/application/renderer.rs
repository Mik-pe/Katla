//! Frame rendering implementation.
//!
//! This module implements frame rendering using the FrameGraph API
//! and the new FrameContext for automatic instance allocation.

use super::Application;
use crate::rendering::FrameContext;
use katla_gfx::renderer::FrameUniforms;

impl Application {
    /// Render a single frame using the frame graph.
    ///
    /// Uses FrameContext for draw submission with automatic instance allocation.
    pub fn render_frame(&mut self) {
        // Create frame context for this frame
        let mut frame = FrameContext::new();

        // Get camera view/projection matrices
        let camera = self.camera.borrow();
        let view_mat = camera.get_view_mat(&self.world);
        let proj_mat = camera.get_proj_mat(&self.world);
        let camera_entity = camera.entity;
        drop(camera); // Release transform borrow

        // Get camera position from transform component
        use crate::components::TransformComponent;
        let cam_pos = if let Some(transform) = self
            .world
            .get_component::<TransformComponent>(camera_entity)
        {
            [
                transform.transform.position.x(),
                transform.transform.position.y(),
                transform.transform.position.z(),
                1.0,
            ]
        } else {
            [0.0, 0.0, 0.0, 1.0]
        };

        // Compute inverse view-projection for sky rendering
        let inv_view_proj = (proj_mat.clone() * view_mat.clone()).inverse();

        // Set frame uniforms (uses katla_gfx public type directly)
        let frame_uniforms = FrameUniforms {
            view_matrix: view_mat.to_array(),
            proj_matrix: proj_mat.to_array(),
            inv_view_proj_matrix: inv_view_proj.to_array(),
            camera_position: cam_pos,
            // Default lighting (sunlight)
            light_direction: [0.3, 1.0, 0.2, 0.0],
            light_color: [1.0, 0.98, 0.95, 0.0],
            light_intensity: 1.0,
        };
        frame.set_frame_uniforms(frame_uniforms);

        // Collect draw calls from ECS world using FrameContext
        self.collect_draws_with_context(&mut frame);

        // Apply frame uniforms to renderer
        self.renderer
            .set_frame_uniforms(frame.frame_uniforms().clone());

        // Execute draw calls (writes per-object data to storage buffer)
        self.renderer.execute_draw_calls(frame.draw_list());

        // Render using the frame graph
        let draw_list = frame.take_draw_list();

        self.renderer.render(&mut self.frame_graph, |frame| {
            // Submit draw list to the geometry pass
            if !draw_list.is_empty() {
                frame.submit("geometry", &draw_list);
            }
        });
    }

    /// Collect drawable components from the ECS world and submit to FrameContext.
    ///
    /// This automatically allocates instance indices and builds the draw list.
    fn collect_draws_with_context(&self, frame: &mut FrameContext) {
        use crate::components::{DrawableComponent, TransformComponent};

        let entity_count = self.world.entity_ids().count();
        let mut drawable_count = 0;

        for entity_id in self.world.entity_ids() {
            // Get drawable and transform components
            let drawable = match self.world.get_component::<DrawableComponent>(entity_id) {
                Some(d) => d,
                None => continue,
            };
            let transform = match self.world.get_component::<TransformComponent>(entity_id) {
                Some(t) => t,
                None => continue,
            };

            // Get mesh and material handles
            let mesh_handle = match drawable.mesh_handle {
                Some(h) => h,
                None => continue,
            };
            let material_handle = match drawable.material_handle {
                Some(h) => h,
                None => continue,
            };

            // Submit draw via FrameContext (instance allocation is automatic)
            frame
                .draw(mesh_handle, material_handle)
                .with_transform(transform.transform.make_mat4().to_array())
                .submit();

            drawable_count += 1;
        }

        log::debug!(
            "Submitted {} draw calls from {} entities",
            drawable_count,
            entity_count
        );
    }
}
