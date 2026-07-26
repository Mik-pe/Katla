from pathlib import Path


def replace_once(source: str, old: str, new: str, label: str) -> str:
    if old not in source:
        raise SystemExit(f"expected source fragment not found: {label}")
    return source.replace(old, new, 1)


# Selection adds gizmo/debug draws. Those draws must exist before Metal uploads
# object uniforms, otherwise their instance indices reference unwritten slots.
renderer_path = Path("katla_app/src/application/renderer.rs")
renderer = renderer_path.read_text()
old_order = '''        let mut draw_list = frame.take_draw_list();
        self.last_draw_call_count = draw_list.len();
        draw_list.sort_by_material();

        if let Err(e) = self.renderer.execute_draw_calls(&draw_list) {
            log::error!("Failed to execute draw calls: {}", e);
            return;
        }

        log::debug!(
            "About to submit {} draw calls to Metal renderer",
            draw_list.len()
        );

        let (shadow_draw_list, outline_draw_list) = self.prepare_draw_lists(&mut draw_list);
'''
new_order = '''        let mut draw_list = frame.take_draw_list();
        self.last_draw_call_count = draw_list.len();
        draw_list.sort_by_material();

        // Selection and editor overlays append gizmo/debug draws with fresh instance
        // indices. Prepare them before uploading object uniforms so every submitted
        // draw references initialized GPU data.
        let (shadow_draw_list, outline_draw_list) = self.prepare_draw_lists(&mut draw_list);

        if let Err(e) = self.renderer.execute_draw_calls(&draw_list) {
            log::error!("Failed to execute draw calls: {}", e);
            return;
        }

        log::debug!(
            "About to submit {} draw calls to Metal renderer",
            draw_list.len()
        );
'''
if old_order in renderer:
    renderer = renderer.replace(old_order, new_order, 1)
elif new_order not in renderer:
    raise SystemExit("Metal draw preparation/upload order did not match")
renderer_path.write_text(renderer)


# Reject invalid object-buffer offsets before any Metal encoder can bind them.
metal_renderer_path = Path("katla_gfx/src/metal/metal_renderer.rs")
metal_renderer = metal_renderer_path.read_text()
helper_marker = '''pub(crate) const OBJECT_UNIFORM_SIZE: u64 = 16 * 4 + 4 * 4 + 4 * 4 + 4 * 4;
pub(crate) const FRAMES_IN_FLIGHT: usize = 2;
'''
helper = '''pub(crate) const OBJECT_UNIFORM_SIZE: u64 = 16 * 4 + 4 * 4 + 4 * 4 + 4 * 4;
pub(crate) const FRAMES_IN_FLIGHT: usize = 2;

fn validate_object_buffer_capacity(
    draw_list: &DrawList,
    buffer_size: usize,
) -> Result<(), RendererError> {
    let Some(max_instance_index) = draw_list.draws.iter().map(|draw| draw.instance_index).max()
    else {
        return Ok(());
    };

    let required_size = (max_instance_index as usize)
        .checked_add(1)
        .and_then(|count| count.checked_mul(OBJECT_UNIFORM_SIZE as usize))
        .ok_or_else(|| {
            RendererError::InvalidOperation(format!(
                "Object uniform size overflow for instance index {max_instance_index}"
            ))
        })?;

    if required_size > buffer_size {
        let capacity = buffer_size / OBJECT_UNIFORM_SIZE as usize;
        return Err(RendererError::InvalidOperation(format!(
            "Draw list requires object instance index {max_instance_index}, but the Metal object buffer only has {capacity} slots"
        )));
    }

    Ok(())
}

#[cfg(test)]
mod object_buffer_capacity_tests {
    use super::*;
    use crate::renderer::types::DrawCall;

    fn draw_list(indices: &[u32]) -> DrawList {
        let mut draws = DrawList::new();
        for &index in indices {
            draws.push(
                DrawCall::new(MeshHandle::NONE, MaterialHandle::NONE)
                    .with_instance_index(index),
            );
        }
        draws
    }

    #[test]
    fn empty_draw_list_needs_no_object_storage() {
        assert!(validate_object_buffer_capacity(&DrawList::new(), 0).is_ok());
    }

    #[test]
    fn highest_instance_index_must_fit_the_uploaded_buffer() {
        let two_slots = OBJECT_UNIFORM_SIZE as usize * 2;
        assert!(validate_object_buffer_capacity(&draw_list(&[0, 1]), two_slots).is_ok());

        let error = validate_object_buffer_capacity(&draw_list(&[0, 2]), two_slots)
            .expect_err("instance index 2 must not fit a two-slot object buffer");
        assert!(error.to_string().contains("instance index 2"));
        assert!(error.to_string().contains("2 slots"));
    }
}
'''
if "fn validate_object_buffer_capacity(" not in metal_renderer:
    metal_renderer = replace_once(
        metal_renderer,
        helper_marker,
        helper,
        "object-buffer validation helper insertion",
    )

old_buffer_setup = '''        let object_buf = self.current_object_storage_buffer().unwrap();
        let buf_size = object_buf.size() as usize;
        let ptr = object_buf.map();

        for draw in &draw_list.draws {
'''
new_buffer_setup = '''        let object_buf = self.current_object_storage_buffer().unwrap();
        let buf_size = object_buf.size() as usize;
        validate_object_buffer_capacity(draw_list, buf_size)?;
        let ptr = object_buf.map();

        for draw in &draw_list.draws {
'''
if old_buffer_setup in metal_renderer:
    metal_renderer = metal_renderer.replace(old_buffer_setup, new_buffer_setup, 1)
elif new_buffer_setup not in metal_renderer:
    raise SystemExit("Metal object-buffer upload setup did not match")

old_per_draw_check = '''            let offset = draw.instance_index as usize * OBJECT_UNIFORM_SIZE as usize;
            if offset + OBJECT_UNIFORM_SIZE as usize > buf_size {
                log::warn!(
                    "Draw call instance_index {} exceeds object storage buffer capacity, skipping",
                    draw.instance_index
                );
                continue;
            }
            let dst = unsafe { ptr.add(offset) };
'''
new_per_draw_check = '''            let offset = draw.instance_index as usize * OBJECT_UNIFORM_SIZE as usize;
            debug_assert!(offset + OBJECT_UNIFORM_SIZE as usize <= buf_size);
            let dst = unsafe { ptr.add(offset) };
'''
if old_per_draw_check in metal_renderer:
    metal_renderer = metal_renderer.replace(old_per_draw_check, new_per_draw_check, 1)
elif new_per_draw_check not in metal_renderer:
    raise SystemExit("Metal per-draw object-buffer check did not match")
metal_renderer_path.write_text(metal_renderer)


# Restore a single coherent editor viewport path: tonemap into the graph-owned
# viewport_0 texture, then let the UI composite that texture into the drawable.
frame_render_path = Path("katla_gfx/src/metal/frame_render.rs")
frame_render = frame_render_path.read_text()
viewport_helper_marker = '''const CANVAS_CLEAR_COLOR: (f64, f64, f64, f64) = (0.013, 0.013, 0.013, 1.0);
'''
viewport_helper = '''const CANVAS_CLEAR_COLOR: (f64, f64, f64, f64) = (0.013, 0.013, 0.013, 1.0);

fn tonemap_viewport(
    drawable_height: f32,
    panel_x: f32,
    panel_y: f32,
    panel_width: f32,
    panel_height: f32,
    offscreen: bool,
) -> (f32, f32, f32, f32) {
    if offscreen {
        (0.0, 0.0, panel_width, panel_height)
    } else {
        (
            panel_x,
            drawable_height - (panel_y + panel_height),
            panel_width,
            panel_height,
        )
    }
}
'''
if "fn tonemap_viewport(" not in frame_render:
    frame_render = replace_once(
        frame_render,
        viewport_helper_marker,
        viewport_helper,
        "tonemap viewport helper insertion",
    )

old_viewport_setup = '''        let vp_x = vp.map_or(0.0, |r| r.min[0]);
        let vp_y = vp.map_or(0.0, |r| r.min[1]);
        let vp_w = vp.map_or(width, |r| r.width());
        let vp_h = vp.map_or(height, |r| r.height());
'''
new_viewport_setup = '''        let vp_x = vp.map_or(0.0, |r| r.min[0]);
        let vp_y = vp.map_or(0.0, |r| r.min[1]);
        let vp_w = vp.map_or(width, |r| r.width());
        let vp_h = vp.map_or(height, |r| r.height());
        let ui_will_composite_viewport = schedule.contains(PassKind::Ui)
            && self
                .pending_ui_draw_list
                .as_ref()
                .is_some_and(|draw_list| !draw_list.is_empty())
            && self.tonemap_output_view.is_some();
'''
if old_viewport_setup in frame_render:
    frame_render = frame_render.replace(old_viewport_setup, new_viewport_setup, 1)
elif new_viewport_setup not in frame_render:
    raise SystemExit("Metal viewport setup did not match")

old_tonemap_target = '''            // Clear the entire drawable to panel background color before the tonemap
            // pass loads it and renders the 3D scene into the viewport panel rect.
            {
                let clear_desc = objc2_metal::MTLRenderPassDescriptor::new();
                let color_attach =
                    unsafe { clear_desc.colorAttachments().objectAtIndexedSubscript(0) };
                color_attach.setTexture(Some(&drawable_view.inner));
                color_attach.setLoadAction(objc2_metal::MTLLoadAction::Clear);
                color_attach.setStoreAction(objc2_metal::MTLStoreAction::Store);
                color_attach.setClearColor(objc2_metal::MTLClearColor {
                    red: CANVAS_CLEAR_COLOR.0,
                    green: CANVAS_CLEAR_COLOR.1,
                    blue: CANVAS_CLEAR_COLOR.2,
                    alpha: CANVAS_CLEAR_COLOR.3,
                });
                let clear_encoder = cmd_buffer
                    .inner
                    .renderCommandEncoderWithDescriptor(&clear_desc)
                    .expect("Failed to create clear encoder");
                let label = objc2_foundation::NSString::from_str("clear_drawable");
                clear_encoder.setLabel(Some(&label));
                clear_encoder.endEncoding();
            }

            // Tonemap renders directly into the drawable, constrained to the
            // Tonemap renders directly into the drawable, constrained to the
            // viewport panel rect. Rendering to a separate panel-sized
            // intermediate (viewport_0) and blitting produced a vertical
            // duplication of the scene, so we render in place instead.
            drawable_written = true;
            let tonemap_target = drawable_view.clone();
            // Metal viewport originY is measured from the bottom of the
            // attachment; the panel rect uses top-down coordinates.
            let mtl_vp_y = height - (vp_y + vp_h);

            let tonemap_pass_info = RenderPassInfo {
                color_attachments: vec![ColorAttachmentInfo {
                    view: tonemap_target,
                    load_op: LoadOp::Load,
                    store_op: StoreOp::Store,
                    clear_value: ClearValue::OPAQUE_BLACK,
                }],
                depth_attachment: None,
            };

            let mut encoder = cmd_buffer.begin_render_pass(tonemap_pass_info);
            encoder.set_viewport(vp_x, mtl_vp_y, vp_w, vp_h, 0.0, 1.0);
            encoder.set_scissor(vp_x as u32, mtl_vp_y as u32, vp_w as u32, vp_h as u32);
'''
new_tonemap_target = '''            let (tonemap_target, tonemap_load_op) = if ui_will_composite_viewport {
                // The editor UI samples viewport_0. Render in the panel-sized
                // texture's local coordinate system so the image fills the whole
                // viewport widget instead of occupying a drawable-relative quadrant.
                (
                    self.tonemap_output_view
                        .as_ref()
                        .expect("viewport_0 view checked above")
                        .clone(),
                    LoadOp::Clear,
                )
            } else {
                // Non-editor/headless fallback when there is no UI composition pass.
                drawable_written = true;
                (drawable_view.clone(), LoadOp::Clear)
            };
            let (tonemap_x, tonemap_y, tonemap_w, tonemap_h) = tonemap_viewport(
                height,
                vp_x,
                vp_y,
                vp_w,
                vp_h,
                ui_will_composite_viewport,
            );

            let tonemap_pass_info = RenderPassInfo {
                color_attachments: vec![ColorAttachmentInfo {
                    view: tonemap_target,
                    load_op: tonemap_load_op,
                    store_op: StoreOp::Store,
                    clear_value: ClearValue::color(
                        CANVAS_CLEAR_COLOR.0 as f32,
                        CANVAS_CLEAR_COLOR.1 as f32,
                        CANVAS_CLEAR_COLOR.2 as f32,
                        CANVAS_CLEAR_COLOR.3 as f32,
                    ),
                }],
                depth_attachment: None,
            };

            let mut encoder = cmd_buffer.begin_render_pass(tonemap_pass_info);
            encoder.set_viewport(tonemap_x, tonemap_y, tonemap_w, tonemap_h, 0.0, 1.0);
            encoder.set_scissor(
                tonemap_x as u32,
                tonemap_y as u32,
                tonemap_w as u32,
                tonemap_h as u32,
            );
'''
if old_tonemap_target in frame_render:
    frame_render = frame_render.replace(old_tonemap_target, new_tonemap_target, 1)
elif new_tonemap_target not in frame_render:
    raise SystemExit("Metal tonemap target block did not match")

old_ui_load = '''                drawable_written = true;
                // The scene is always written to the drawable before the UI pass
                // (either tonemap writes directly, or the blit copies viewport_0 → drawable).
                // Load the prior pass output so the UI composites on top of the 3D scene.
                let ui_load_op = if drawable_written {
                    LoadOp::Load
                } else {
                    LoadOp::Clear
                };
'''
new_ui_load = '''                // When tonemap wrote viewport_0, UI is the first drawable writer
                // and must clear the canvas. Direct-to-drawable fallbacks are loaded.
                let ui_load_op = if drawable_written {
                    LoadOp::Load
                } else {
                    LoadOp::Clear
                };
                drawable_written = true;
'''
if old_ui_load in frame_render:
    frame_render = frame_render.replace(old_ui_load, new_ui_load, 1)
elif new_ui_load not in frame_render:
    raise SystemExit("Metal UI load-op block did not match")

old_ui_clear = '''                        clear_value: ClearValue::OPAQUE_BLACK,
'''
new_ui_clear = '''                        clear_value: ClearValue::color(
                            CANVAS_CLEAR_COLOR.0 as f32,
                            CANVAS_CLEAR_COLOR.1 as f32,
                            CANVAS_CLEAR_COLOR.2 as f32,
                            CANVAS_CLEAR_COLOR.3 as f32,
                        ),
'''
# Replace only the UI attachment occurrence after the UI pass marker.
ui_marker = "// Pass 3: UI overlay"
ui_index = frame_render.index(ui_marker)
ui_tail = frame_render[ui_index:]
if old_ui_clear in ui_tail:
    ui_tail = ui_tail.replace(old_ui_clear, new_ui_clear, 1)
    frame_render = frame_render[:ui_index] + ui_tail
elif new_ui_clear not in ui_tail:
    raise SystemExit("Metal UI clear value did not match")

if "offscreen_tonemap_uses_texture_local_coordinates" not in frame_render:
    frame_render += r'''

#[cfg(test)]
mod tests {
    use super::tonemap_viewport;

    #[test]
    fn offscreen_tonemap_uses_texture_local_coordinates() {
        assert_eq!(
            tonemap_viewport(1200.0, 240.0, 80.0, 960.0, 720.0, true),
            (0.0, 0.0, 960.0, 720.0)
        );
    }

    #[test]
    fn direct_tonemap_converts_top_down_panel_origin_for_metal() {
        assert_eq!(
            tonemap_viewport(1200.0, 240.0, 80.0, 960.0, 720.0, false),
            (240.0, 400.0, 960.0, 720.0)
        );
    }
}
'''
frame_render_path.write_text(frame_render)


# Guard against regressing the selection crash by preserving preparation before upload.
renderer = renderer_path.read_text()
metal_impl = renderer.index('#[cfg(target_os = "macos")]\nimpl Application')
prepare = renderer.index("self.prepare_draw_lists(&mut draw_list)", metal_impl)
upload = renderer.index("self.renderer.execute_draw_calls(&draw_list)", metal_impl)
if prepare > upload:
    raise SystemExit("Metal object uniforms are still uploaded before editor draws are prepared")
