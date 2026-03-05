//! Frame rendering implementation.
//!
//! This module implements frame rendering using the FrameGraph API.

use super::Application;
use katla_gfx::renderer::DrawList;

impl Application {
    /// Render a single frame using the frame graph.
    ///
    /// Uses the new FrameGraph API for frame submission.
    pub fn render_frame(&mut self) {
        // Get draw lists from the ECS world
        let draw_list = self.collect_draw_list();

        // Render using the frame graph
        self.renderer.render(&mut self.frame_graph, |frame| {
            // Submit draw list to the geometry pass
            if !draw_list.is_empty() {
                frame.submit("geometry", &draw_list);
            }
        });
    }

    /// Collect drawable components from the ECS world and build a DrawList.
    fn collect_draw_list(&self) -> DrawList {
        use crate::components::{DrawableComponent, TransformComponent};

        let mut draw_list = DrawList::new();
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

            // Create draw call with transform
            let draw_call = katla_gfx::renderer::DrawCall::new(mesh_handle, material_handle)
                .with_transform(transform.transform.make_mat4().to_array());

            draw_list.push(draw_call);
            drawable_count += 1;
        }

        if drawable_count > 0 {
            log::info!("Collected {} draw calls from {} entities", drawable_count, entity_count);
        }

        draw_list
    }
}
