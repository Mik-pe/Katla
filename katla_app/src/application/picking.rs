#[cfg(feature = "editor")]
use log::info;

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
        if !self.frame_graph_runtime.uses_katla_scene() || self.pass_ids.picking.is_none() {
            self.editor.pending_pick = None;
            return;
        }

        let object_id_resource = self.frame_graph_bindings.resources.object_id.clone();
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
            // The picking texture is panel-sized (matches the scene render
            // targets), so map panel-local coords into panel-physical pixels.
            let pick_w = self.panel_rt_size.width.max(1);
            let pick_h = self.panel_rt_size.height.max(1);
            let physical_x = ((rel_x / panel_width) * pick_w as f32) as u32;
            let physical_y = ((rel_y / panel_height) * pick_h as f32) as u32;

            // Metal's viewport maps clip Y = +1 → pixel Y = 0 (top), which inverts Y
            // compared to the tonemapped display. Flip the readback Y so that
            // screen-top (rel_y=0) reads the pixel corresponding to what the user sees.
            #[cfg(target_os = "macos")]
            let physical_y = if matches!(self.renderer, katla_gfx::AnyRenderer::Metal(_)) {
                pick_h.saturating_sub(1 + physical_y)
            } else {
                physical_y
            };

            if physical_x >= pick_w || physical_y >= pick_h {
                log::debug!(
                    "Picking coords ({}, {}) out of render target bounds ({}x{}), skipping",
                    physical_x,
                    physical_y,
                    pick_w,
                    pick_h
                );
                return;
            }

            match &mut self.renderer {
                katla_gfx::AnyRenderer::Vulkan(r) => {
                    let Some(object_id_resource) = object_id_resource.as_deref() else {
                        log::debug!(
                            "Skipping Vulkan picking because no object-ID resource is bound"
                        );
                        return;
                    };
                    let frame_idx = r.current_frame();
                    if let Some(transient) = self
                        .frame_graph
                        .as_vulkan()
                        .transient_texture(object_id_resource, frame_idx)
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
                        log::warn!(
                            "Bound object-ID resource '{}' was not available for picking readback",
                            object_id_resource
                        );
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
