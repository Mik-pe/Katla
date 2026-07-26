from pathlib import Path


def replace_once(source: str, old: str, new: str, label: str) -> str:
    if old not in source:
        raise SystemExit(f"expected source fragment not found: {label}")
    return source.replace(old, new, 1)


argument_buffer_path = Path("katla_gfx/src/metal/argument_buffer.rs")
argument_buffer = argument_buffer_path.read_text()
if "use objc2::Message;" not in argument_buffer:
    argument_buffer = replace_once(
        argument_buffer,
        "use objc2::rc::Retained;\n",
        "use objc2::Message;\nuse objc2::rc::Retained;\n",
        "objc2 Message import",
    )
argument_buffer_path.write_text(argument_buffer)


renderer_path = Path("katla_gfx/src/metal/metal_renderer.rs")
renderer = renderer_path.read_text()
old_eager_init = '''        // Initialize the argument buffer after all default textures are registered.
        // Slot 0 = white (albedo/AO), slot 1 = flat normal, slot 2 = MR default
        if let Some(entry) = renderer.textures.get(default_tex.index()) {
            renderer
                .bindless_manager
                .init_argument_buffer(&renderer.context.device, &entry._view.inner);
        }
'''
new_lazy_init = '''        // Texture registration is valid before a shader layout exists. The argument
        // buffer itself is initialized lazily from the first compiled fragment
        // function so Metal, rather than Katla, owns the concrete layout ABI.
        if let Some(entry) = renderer.textures.get(default_tex.index()) {
            renderer
                .bindless_manager
                .set_default_texture(&entry._view.inner);
        }
'''
if old_eager_init in renderer:
    renderer = renderer.replace(old_eager_init, new_lazy_init, 1)
elif new_lazy_init not in renderer:
    raise SystemExit("eager Metal argument-buffer initialization did not match")

old_readback = '''        let (_readback_tex, readback_view) = renderer
            .context
            .create_texture_with_data(&readback_desc)
            .expect("Failed to create readback texture");
'''
new_readback = '''        let (readback_tex, readback_view) = renderer
            .context
            .create_texture_with_data(&readback_desc)
            .expect("Failed to create readback texture");
        // The no-UI headless schedule tonemaps directly to the current drawable.
        // Use the CPU-readable texture as that drawable so the test reads the
        // attachment that was actually rendered.
        renderer.set_headless_drawable(readback_tex.inner.clone());
'''
if old_readback in renderer:
    renderer = renderer.replace(old_readback, new_readback, 1)
elif new_readback not in renderer:
    raise SystemExit("headless readback texture setup did not match")

renderer = renderer.replace(
    "        let pixels = readback_texture_bgra8(&_readback_tex.inner, W, H);",
    "        let pixels = readback_texture_bgra8(&readback_tex.inner, W, H);",
    1,
)
renderer_path.write_text(renderer)


material_path = Path("katla_gfx/src/metal/material_api.rs")
material = material_path.read_text()
marker = '''        let fragment_fn = compiled.module.entry_points.get("fs_main");

        // For UI, also compile instanced entry points for a second pipeline
'''
replacement = '''        let fragment_fn = compiled.module.entry_points.get("fs_main");

        // Derive the bindless argument-buffer encoder from an actual compiled
        // shader function. This avoids the arbitrary-layout MTLDevice API that
        // raises an Objective-C exception on AppleParavirtDevice.
        if vertex_type != "compute" && !self.bindless_manager.is_initialized() {
            let fragment_function = fragment_fn.ok_or_else(|| {
                RendererError::InitializationFailed(
                    "The first Metal graphics material has no fragment function for bindless layout reflection"
                        .into(),
                )
            })?;
            self.bindless_manager
                .initialize_from_function(fragment_function.as_ref())?;
        }

        // For UI, also compile instanced entry points for a second pipeline
'''
if marker in material:
    material = material.replace(marker, replacement, 1)
elif replacement not in material:
    raise SystemExit("Metal material fragment-function block did not match")
material_path.write_text(material)
