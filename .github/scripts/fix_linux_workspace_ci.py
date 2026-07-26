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


ui_mod_path = Path("katla_app/src/ui/mod.rs")
ui_mod = ui_mod_path.read_text()
ui_mod = ui_mod.replace(
    "    Panel, ParticleEmitterInfo, PerspectiveInfo, PhysicsMaterialInfo, PointLightInfo,\n",
    "    ParticleEmitterInfo, PerspectiveInfo, PhysicsMaterialInfo, PointLightInfo,\n",
    1,
)
panel_export = '''#[cfg(all(feature = "editor", target_os = "macos"))]
pub use editor_ui::Panel;
'''
if panel_export not in ui_mod:
    ui_mod = replace_once(
        ui_mod,
        "pub use katla_ui::ColorScheme;\n",
        panel_export + "pub use katla_ui::ColorScheme;\n",
        "macOS UI-test Panel export",
    )
ui_mod_path.write_text(ui_mod)


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


text_path = Path("katla_ui/src/text/mod.rs")
text = text_path.read_text()
text = text.replace(
    '''                    assert!(
                        alpha <= 255,
                        "Alpha value {} should be <= 255 (R8 range)",
                        alpha
                    );
''',
    "",
    1,
)
text = text.replace(
    '''        let atlas_data = sys.atlas_data();
        for &byte in atlas_data {
            assert!(byte <= 255, "Atlas should store single-byte alpha values");
        }
''',
    '''        let atlas_data = sys.atlas_data();
        assert!(!atlas_data.is_empty(), "Atlas should contain rasterized alpha data");
''',
    1,
)
text_path.write_text(text)


preset_test_path = Path("katla_gfx/tests/particle_preset_tests.rs")
preset_test = preset_test_path.read_text().replace("mod common;\n\n", "", 1)
preset_test_path.write_text(preset_test)


bindless_path = Path("katla_gfx/src/vulkan/bindless_texture.rs")
bindless = bindless_path.read_text()
lookup_method = '''    /// Get the bindless slot index for a texture handle.
    ///
    /// This is used internally by the renderer to map TextureHandle values
    /// to their bindless slot indices for shader binding.
    ///
    /// # Arguments
    /// * `image_view` - The Vulkan image view to look up
    ///
    /// # Returns
    /// The slot index if the texture is registered, None otherwise.
    ///
    /// # Note
    /// Currently unused but kept for future texture management features.
    #[cfg(test)]
    pub(crate) fn get_slot_for_image_view(&self, image_view: vk::ImageView) -> Option<u32> {
        self.slots
            .iter()
            .position(|&slot| slot == Some(image_view))
            .map(|i| i as u32)
    }

'''
bindless = bindless.replace(lookup_method, "", 1)
default_helpers = '''    /// Check if a slot is a default texture slot.
    ///
    /// # Arguments
    /// * `slot` - The slot index to check
    ///
    /// # Returns
    /// true if the slot is reserved for default textures (0-4).
    #[cfg(test)]
    pub(crate) fn is_default_slot(&self, slot: u32) -> bool {
        slot < DEFAULT_TEXTURE_COUNT
    }

    /// Get the number of slots reserved for default textures.
    #[cfg(test)]
    pub(crate) fn default_texture_count(&self) -> u32 {
        DEFAULT_TEXTURE_COUNT
    }
'''
bindless = bindless.replace(default_helpers, "", 1)
bindless_path.write_text(bindless)


validation_path = Path("katla_gfx/src/vulkan/context/validation.rs")
validation = validation_path.read_text()
validation = validation.replace(
    "use ash::{Entry, ext::debug_utils::Instance as DebugInstance, vk};",
    "use ash::{Entry, Instance, ext::debug_utils::Instance as DebugInstance, vk};",
    1,
)
old_debug_messenger = '''pub(super) fn create_debug_messenger(
    debug_utils_loader: &DebugInstance,
    with_validation_layers: bool,
    user_data: *mut std::ffi::c_void,
) -> Option<vk::DebugUtilsMessengerEXT> {
    if with_validation_layers {
        let create_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
            .message_severity(
                vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE
                    | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                    | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
            )
            .message_type(
                vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
            )
            .pfn_user_callback(Some(debug_callback))
            .user_data(user_data);

        Some(
            unsafe { debug_utils_loader.create_debug_utils_messenger(&create_info, None) }.unwrap(),
        )
    } else {
        None
    }
}
'''
new_debug_messenger = '''pub(super) fn create_debug_messenger(
    entry: &Entry,
    instance: &Instance,
    debug_utils_loader: &DebugInstance,
    with_validation_layers: bool,
    user_data: *mut std::ffi::c_void,
) -> Option<vk::DebugUtilsMessengerEXT> {
    if !with_validation_layers {
        return None;
    }

    let create_name = c"vkCreateDebugUtilsMessengerEXT";
    let destroy_name = c"vkDestroyDebugUtilsMessengerEXT";
    let functions_available = unsafe {
        entry
            .get_instance_proc_addr(instance.handle(), create_name.as_ptr())
            .is_some()
            && entry
                .get_instance_proc_addr(instance.handle(), destroy_name.as_ptr())
                .is_some()
    };
    if !functions_available {
        log::warn!(
            "VK_EXT_debug_utils was enabled but its messenger functions are unavailable; continuing validation without a debug callback"
        );
        return None;
    }

    let create_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
        .message_severity(
            vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE
                | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
        )
        .message_type(
            vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
        )
        .pfn_user_callback(Some(debug_callback))
        .user_data(user_data);

    match unsafe { debug_utils_loader.create_debug_utils_messenger(&create_info, None) } {
        Ok(messenger) => Some(messenger),
        Err(error) => {
            log::warn!("Failed to create Vulkan debug messenger: {error:?}");
            None
        }
    }
}
'''
if old_debug_messenger in validation:
    validation = validation.replace(old_debug_messenger, new_debug_messenger, 1)
elif new_debug_messenger not in validation:
    raise SystemExit("Vulkan debug messenger implementation did not match")
validation_path.write_text(validation)


context_path = Path("katla_gfx/src/vulkan/context/mod.rs")
context = context_path.read_text()
old_debug_call = '''        let debug_callback = validation::create_debug_messenger(
            &debug_utils_loader,
            validation_layers_active,
            user_data,
        );
'''
new_debug_call = '''        let debug_callback = validation::create_debug_messenger(
            &entry,
            &instance,
            &debug_utils_loader,
            validation_layers_active,
            user_data,
        );
'''
if context.count(old_debug_call) == 2:
    context = context.replace(old_debug_call, new_debug_call, 2)
elif context.count(new_debug_call) != 2:
    raise SystemExit("Vulkan debug messenger call sites did not match")
context_path.write_text(context)
