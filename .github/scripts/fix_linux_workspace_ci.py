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
normal_info = '''            dump_layout_path: self.dump_layout_path,
            screenshot_path: None,
            headless: false,
            ui_test_path: None,
'''
normal_info_cfg = '''            dump_layout_path: self.dump_layout_path,
            #[cfg(target_os = "macos")]
            screenshot_path: None,
            headless: false,
            #[cfg(target_os = "macos")]
            ui_test_path: None,
'''
if normal_info in builder:
    builder = builder.replace(normal_info, normal_info_cfg, 1)
elif normal_info_cfg not in builder:
    raise SystemExit("windowed ApplicationInfo platform fields did not match")
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
if not headless.startswith('#![cfg(target_os = "macos")]'):
    headless = '#![cfg(target_os = "macos")]\n\n' + headless
headless_path.write_text(headless)


ui_test_path = Path("katla_app/src/application/ui_test.rs")
ui_test = ui_test_path.read_text()
if not ui_test.startswith('#![cfg(target_os = "macos")]'):
    ui_test = '#![cfg(target_os = "macos")]\n\n' + ui_test
ui_test_path.write_text(ui_test)


application_mod_path = Path("katla_app/src/application/mod.rs")
application_mod = application_mod_path.read_text()
old_info_fields = '''    dump_layout_path: Option<DumpLayoutTarget>,
    screenshot_path: Option<String>, // Headless screenshot output path
    headless: bool,                  // Running without a window
    pub(crate) ui_test_path: Option<String>, // UI test mode: output directory for screenshots
'''
new_info_fields = '''    dump_layout_path: Option<DumpLayoutTarget>,
    #[cfg(target_os = "macos")]
    screenshot_path: Option<String>, // Headless screenshot output path
    headless: bool, // Running without a window
    #[cfg(target_os = "macos")]
    pub(crate) ui_test_path: Option<String>, // UI test mode: output directory for screenshots
'''
if old_info_fields in application_mod:
    application_mod = application_mod.replace(old_info_fields, new_info_fields, 1)
elif new_info_fields not in application_mod:
    raise SystemExit("ApplicationInfo platform fields did not match")
application_mod = application_mod.replace(
    "    #[expect(dead_code)]\n    pub(crate) point_lights_buffer: Vec<katla_gfx::PointLightGPU>,",
    "    pub(crate) point_lights_buffer: Vec<katla_gfx::PointLightGPU>,",
    1,
)
application_mod_path.write_text(application_mod)


editor_ui_path = Path("katla_app/src/ui/editor_ui/mod.rs")
editor_ui = editor_ui_path.read_text()
expand_marker = '''    /// Expand an entity in the hierarchy panel (show its children).
    pub fn expand_entity(&mut self, id: EntityId) {
'''
expand_gated = '''    /// Expand an entity in the hierarchy panel (show its children).
    #[cfg(target_os = "macos")]
    pub fn expand_entity(&mut self, id: EntityId) {
'''
if expand_gated not in editor_ui:
    editor_ui = replace_once(editor_ui, expand_marker, expand_gated, "UI-test hierarchy expansion cfg")
editor_ui_path.write_text(editor_ui)


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


game_main_path = Path("game/src/main.rs")
game_main = game_main_path.read_text()
headless_branch = '''    if args.headless || args.ui_test.is_some() {
        if let Some(ref dir) = args.ui_test {
            builder = builder.ui_test_path(dir.clone());
        }
        let screenshot_path = args
            .screenshot
            .unwrap_or_else(|| "/tmp/katla_screenshot.png".to_string());
        let max_frames = if args.single_frame || args.ui_test.is_some() {
            100
        } else {
            10
        };

        let result = builder.build_headless(max_frames, screenshot_path);
        match result {
            Ok(mut app) => {
                if let Err(e) = app.init() {
                    error!("Application init failed: {e}");
                    std::process::exit(1);
                }
                if let Err(e) = app.run_headless() {
                    error!("Headless render failed: {e}");
                    std::process::exit(1);
                }
            }
            Err(e) => {
                eprintln!("Failed to initialize headless application: {}", e);
                std::process::exit(1);
            }
        }
    } else {
'''
headless_branch_cfg = '''    if args.headless || args.ui_test.is_some() {
        #[cfg(not(target_os = "macos"))]
        {
            error!("Headless rendering is currently supported only on macOS with Metal");
            std::process::exit(2);
        }

        #[cfg(target_os = "macos")]
        {
            if let Some(ref dir) = args.ui_test {
                builder = builder.ui_test_path(dir.clone());
            }
            let screenshot_path = args
                .screenshot
                .unwrap_or_else(|| "/tmp/katla_screenshot.png".to_string());
            let max_frames = if args.single_frame || args.ui_test.is_some() {
                100
            } else {
                10
            };

            let result = builder.build_headless(max_frames, screenshot_path);
            match result {
                Ok(mut app) => {
                    if let Err(e) = app.init() {
                        error!("Application init failed: {e}");
                        std::process::exit(1);
                    }
                    if let Err(e) = app.run_headless() {
                        error!("Headless render failed: {e}");
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("Failed to initialize headless application: {}", e);
                    std::process::exit(1);
                }
            }
        }
    } else {
'''
if headless_branch in game_main:
    game_main = game_main.replace(headless_branch, headless_branch_cfg, 1)
elif headless_branch_cfg not in game_main:
    raise SystemExit("game headless platform branch did not match")
game_main_path.write_text(game_main)
