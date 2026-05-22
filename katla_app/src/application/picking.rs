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
        let picked_result = match &mut self.renderer {
            katla_gfx::AnyRenderer::Vulkan(r) => r.check_picking_readback().ok().flatten(),
            #[cfg(target_os = "macos")]
            katla_gfx::AnyRenderer::Metal(r) => r.check_picking_readback(),
        };

        if let Some((_frame, instance_index)) = picked_result {
            if instance_index == 0 {
                if self.editor.editor_ui.selected_entity.is_some() {
                    info!("Clicked empty space, clearing selection");
                    self.editor.editor_ui.selected_entity = None;
                }
            } else {
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
                log::debug!("Discarding stale pending pick from frame {}", pick_frame);
                return;
            }

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

            match &mut self.renderer {
                katla_gfx::AnyRenderer::Vulkan(r) => {
                    let frame_idx = r.current_frame();
                    if let Some(transient) = self
                        .frame_graph
                        .as_vulkan()
                        .transient_texture("object_id", frame_idx)
                    {
                        let image = transient.image;
                        let current_layout = transient.current_layout();
                        match r.queue_picking_readback(
                            self.frame_count,
                            image,
                            current_layout,
                            physical_x,
                            physical_y,
                        ) {
                            Ok(()) => {
                                log::debug!(
                                    "Queued Vulkan picking readback at ({}, {}) for frame {}",
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
                #[cfg(target_os = "macos")]
                katla_gfx::AnyRenderer::Metal(r) => {
                    match r.queue_picking_readback(self.frame_count, physical_x, physical_y) {
                        Ok(()) => {
                            log::debug!(
                                "Queued Metal picking readback at ({}, {}) for frame {}",
                                physical_x,
                                physical_y,
                                self.frame_count
                            );
                        }
                        Err(e) => {
                            log::warn!("Failed to queue Metal picking readback: {}", e);
                        }
                    }
                }
            }
        }
    }
}
