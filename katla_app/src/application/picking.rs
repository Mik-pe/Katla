#[cfg(feature = "editor")]
use log::info;

#[cfg(feature = "editor")]
use katla_gfx::GpuRenderer;

#[cfg(feature = "editor")]
use crate::application::Application;

#[cfg(feature = "editor")]
impl Application {
    /// Process GPU picking: queue a readback for a pending pick, or resolve a completed readback.
    ///
    /// Flow:
    /// 1. On left-click in viewport: `pending_pick` is set with viewport-relative logical coords
    /// 2. After render_frame: If `pending_pick` is set for this frame, queue the GPU readback
    ///    converting viewport-relative logical coords to full-render-target physical pixel coords
    /// 3. On subsequent frames: Check if the readback completed, resolve instance_index -> EntityId
    pub(crate) fn process_picking(&mut self) {
        // Check for completed readback from a previous frame
        if let Ok(Some((_frame, instance_index))) =
            self.renderer.unwrap_vulkan().check_picking_readback()
        {
            if instance_index == 0 {
                // Background/empty space was clicked — clear selection
                if self.editor.editor_ui.selected_entity.is_some() {
                    info!("Clicked empty space, clearing selection");
                    self.editor.editor_ui.selected_entity = None;
                }
            } else {
                // The shader encodes instance_index + 1, so subtract 1 to get the storage buffer index
                let storage_index = instance_index - 1;

                if let Some(&entity_id) = self.editor.entity_instance_map.get(&storage_index) {
                    info!(
                        "Picked entity {:?} (instance_index={}, storage_index={})",
                        entity_id, instance_index, storage_index
                    );
                    self.editor.editor_ui.selected_entity = Some(entity_id);
                } else {
                    log::debug!(
                        "Picked instance_index={} but no entity mapping found (storage_index={})",
                        instance_index,
                        storage_index
                    );
                    self.editor.editor_ui.selected_entity = None;
                }
            }
        }

        // Queue a new readback if a pick was triggered this frame
        if let Some((pick_frame, rel_x, rel_y)) = self.editor.pending_pick.take() {
            if pick_frame != self.frame_count {
                // Stale pick from a previous frame — discard
                log::debug!("Discarding stale pending pick from frame {}", pick_frame);
                return;
            }

            // Convert viewport-panel-relative logical coordinates to physical pixel coordinates
            // in the full render target (swapchain resolution).
            //
            // The object_id texture covers the full swapchain, but the UI maps it into the
            // viewport panel (a sub-region of the window). So we need to map panel-local
            // coords to full-texture coords:
            //   physical_x = (rel_x / panel_logical_width) * swapchain_physical_width
            let vp = &self.editor.editor_ui.last_viewport_bounds;
            let panel_width = vp.width().max(1.0);
            let panel_height = vp.height().max(1.0);
            let extent = self.renderer.swapchain_extent();
            let physical_x = ((rel_x / panel_width) * extent.width as f32) as u32;
            let physical_y = ((rel_y / panel_height) * extent.height as f32) as u32;

            if physical_x >= extent.width || physical_y >= extent.height {
                log::debug!(
                    "Picking coords ({}, {}) out of render target bounds ({}x{}), skipping",
                    physical_x,
                    physical_y,
                    extent.width,
                    extent.height
                );
                return;
            }

            // Get the object-ID texture image for the current frame
            let frame_idx = self.renderer.current_frame();
            if let Some(transient) = self
                .frame_graph
                .as_vulkan()
                .transient_texture("object_id", frame_idx)
            {
                let image = transient.image;
                let current_layout = transient.current_layout();
                match self.renderer.unwrap_vulkan().queue_picking_readback(
                    self.frame_count,
                    image,
                    current_layout,
                    physical_x,
                    physical_y,
                ) {
                    Ok(()) => {
                        log::debug!(
                            "Queued picking readback at physical ({}, {}) for frame {}",
                            physical_x,
                            physical_y,
                            self.frame_count
                        );
                    }
                    Err(e) => {
                        log::warn!("Failed to queue picking readback: {}", e);
                    }
                }
            } else {
                log::warn!("Object-ID transient texture not found for picking readback");
            }
        }
    }
}
