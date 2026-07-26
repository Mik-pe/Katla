from pathlib import Path


def replace_once(source: str, old: str, new: str, label: str) -> str:
    if old not in source:
        raise SystemExit(f"expected source fragment not found: {label}")
    return source.replace(old, new, 1)


renderer_path = Path("katla_app/src/application/renderer.rs")
renderer = renderer_path.read_text()
if "use katla_gfx::UIDrawList;" not in renderer:
    renderer = replace_once(
        renderer,
        "use katla_gfx::GpuRenderer;\n",
        "use katla_gfx::GpuRenderer;\nuse katla_gfx::UIDrawList;\n",
        "UIDrawList import",
    )
renderer = renderer.replace(
    '            info!("=== Resize complete ===");',
    '            log::info!("=== Resize complete ===");',
    1,
)
renderer_path.write_text(renderer)


builder_path = Path("katla_app/src/application/builder.rs")
builder = builder_path.read_text()
metal_marker = "    /// Build the semantic frame graph used by the Metal backend.\n"
if "    #[cfg(target_os = \"macos\")]\n" + metal_marker not in builder:
    builder = replace_once(
        builder,
        metal_marker,
        "    #[cfg(target_os = \"macos\")]\n" + metal_marker,
        "Metal frame graph cfg gate",
    )
builder_path.write_text(builder)


gizmo_path = Path("katla_app/src/application/gizmo.rs")
gizmo = gizmo_path.read_text()
gizmo = gizmo.replace(
    "use katla_gfx::GpuRenderer;\n",
    "#[cfg(target_os = \"macos\")]\nuse katla_gfx::GpuRenderer;\n",
    1,
)
gizmo_path.write_text(gizmo)


editor_path = Path("katla_app/src/application/editor/mod.rs")
editor = editor_path.read_text()
old_atlas = '''        let atlas_handle = app.renderer.create_ui_font_atlas(width, height, &data);

        if let Some(bindless_slot) = match &mut app.renderer {
            katla_gfx::AnyRenderer::Vulkan(r) => r.ui_renderer.font_atlas_bindless_slot(),
            #[cfg(target_os = "macos")]
            katla_gfx::AnyRenderer::Metal(_) => app.renderer.get_bindless_slot(atlas_handle),
        } {
'''
new_atlas = '''        let atlas_handle = app.renderer.create_ui_font_atlas(width, height, &data);

        if let Some(bindless_slot) = app.renderer.get_bindless_slot(atlas_handle) {
'''
if old_atlas in editor:
    editor = editor.replace(old_atlas, new_atlas, 1)
elif new_atlas not in editor:
    raise SystemExit("font atlas bindless lookup did not match")
editor = editor.replace(
    "                if let katla_gfx::AnyRenderer::Vulkan(vulkan_renderer) = &mut app.renderer {",
    "                if let Some(vulkan_renderer) = app.renderer.as_vulkan() {",
    1,
)
editor_path.write_text(editor)


frame_loop_path = Path("katla_app/src/application/frame_loop.rs")
frame_loop = frame_loop_path.read_text()
old_readback = '''        // Wait for any pending async readback to complete before destroying resources
        // This must happen BEFORE wait_for_device() to ensure readback finishes
        if let katla_gfx::AnyRenderer::Vulkan(vulkan_renderer) = &mut self.renderer {
            match vulkan_renderer.wait_for_pending_readback() {
'''
new_readback = '''        // Wait for any pending async readback to complete before destroying resources
        // This must happen BEFORE wait_for_device() to ensure readback finishes
        let swapchain_extent = self.renderer.swapchain_extent();
        if let Some(vulkan_renderer) = self.renderer.as_vulkan() {
            match vulkan_renderer.wait_for_pending_readback() {
'''
if old_readback in frame_loop:
    frame_loop = frame_loop.replace(old_readback, new_readback, 1)
elif new_readback not in frame_loop:
    raise SystemExit("pending Vulkan readback dispatch did not match")
frame_loop = frame_loop.replace(
    "                    let extent = self.renderer.swapchain_extent();\n                    let width = extent.width as usize;\n                    let height = extent.height as usize;",
    "                    let width = swapchain_extent.width as usize;\n                    let height = swapchain_extent.height as usize;",
    1,
)
frame_loop_path.write_text(frame_loop)


headless_path = Path("katla_app/src/application/headless.rs")
headless = headless_path.read_text()
old_setup = '''        let screenshot_path = self
            .info
            .screenshot_path
            .clone()
            .unwrap_or_else(|| "/tmp/katla_screenshot.png".to_string());

        let mut ui_test = self
            .info
            .ui_test_path
            .as_ref()
            .map(|dir| crate::application::ui_test::UiTestRunner::new(dir.clone()));
'''
new_setup = '''        #[cfg(target_os = "macos")]
        let screenshot_path = self
            .info
            .screenshot_path
            .clone()
            .unwrap_or_else(|| "/tmp/katla_screenshot.png".to_string());

        #[cfg(all(target_os = "macos", feature = "editor"))]
        let mut ui_test = self
            .info
            .ui_test_path
            .as_ref()
            .map(|dir| crate::application::ui_test::UiTestRunner::new(dir.clone()));
        #[cfg(not(all(target_os = "macos", feature = "editor")))]
        let ui_test = self
            .info
            .ui_test_path
            .as_ref()
            .map(|dir| crate::application::ui_test::UiTestRunner::new(dir.clone()));
'''
if old_setup in headless:
    headless = headless.replace(old_setup, new_setup, 1)
elif new_setup not in headless:
    raise SystemExit("headless platform-local setup did not match")
loop_marker = "        for frame in 0..max_frames {\n"
loop_replacement = '''        for frame in 0..max_frames {
            #[cfg(not(all(target_os = "macos", feature = "editor")))]
            let _ = frame;
'''
if loop_replacement not in headless:
    headless = replace_once(
        headless,
        loop_marker,
        loop_replacement,
        "headless frame platform use",
    )
headless = headless.replace(
    '''
            #[cfg(all(target_os = "macos", not(feature = "editor")))]
            let _ = &mut ui_test;
''',
    "\n",
    1,
)
headless_path.write_text(headless)


picking_path = Path("katla_app/src/application/picking.rs")
picking = picking_path.read_text()
old_y = '''            let physical_x = ((rel_x / panel_width) * pick_w as f32) as u32;
            let mut physical_y = ((rel_y / panel_height) * pick_h as f32) as u32;

            // Metal's viewport maps clip Y = +1 → pixel Y = 0 (top), which inverts Y
            // compared to the tonemapped display. Flip the readback Y so that
            // screen-top (rel_y=0) reads the pixel corresponding to what the user sees.
            #[cfg(target_os = "macos")]
            if matches!(self.renderer, katla_gfx::AnyRenderer::Metal(_)) {
                physical_y = pick_h.saturating_sub(1 + physical_y);
            }
'''
new_y = '''            let physical_x = ((rel_x / panel_width) * pick_w as f32) as u32;
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
'''
if old_y in picking:
    picking = picking.replace(old_y, new_y, 1)
elif new_y not in picking:
    raise SystemExit("platform-local picking Y conversion did not match")
picking_path.write_text(picking)
