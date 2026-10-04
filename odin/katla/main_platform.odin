#+build linux, windows
//! Portable editor startup selects the Vulkan backend and the pinned native window dependency.
package main

import render "../app/render"
import window "../app/window"
import shader "../gfx/shader"
import vulkan "../gfx/vulkan"
import "core:fmt"
import "core:os"

main :: proc() {
    config,valid:=config_parse(os.args)
    defer config_destroy(&config)
    if config.help { fmt.println("Katla Odin editor: --shader-compiler EXE --font-library LIB --window-library LIB --vulkan-loader LIB [--scene PROJECT.katla] [--frames N] [--screenshot PNG]");return }
    if !valid || config.backend!="vulkan" || (!config.headless && config.window_library=="") { fmt.eprintln("Portable editor requires Vulkan and explicit shader/font/Vulkan paths; windowed mode also requires --window-library");os.exit(2) }
    if !config.headless { window.window_configure(config.window_library,config.vulkan_loader) }
    compiler:shader.Compiler;if shader.compiler_init(&compiler,config.shader_compiler)!=.None { fmt.eprintln("Cannot initialize offline shader compiler");os.exit(1) };defer shader.compiler_destroy(&compiler)
    fonts:render.UI_Font_System;if render.ui_font_init(&fonts,config.font_library,config.resource_root)!=.None { fmt.eprintln("Cannot initialize native font library");os.exit(1) };defer render.ui_font_destroy(&fonts)
    renderer:vulkan.Renderer;if vulkan.renderer_init(&renderer,validation=true,loader_path=config.vulkan_loader)!=.None { fmt.eprintln("Cannot initialize Vulkan renderer");os.exit(1) };defer vulkan.renderer_destroy(&renderer)
    api:=Backend_API(vulkan.Renderer){gpu={vulkan.create_graphics_pipeline,vulkan.destroy_graphics_pipeline,vulkan.create_buffer_with_data,vulkan.destroy_buffer,vulkan.write_buffer,vulkan.create_texture,vulkan.destroy_texture,vulkan.acquire,vulkan.abort,vulkan.submit,vulkan.wait,vulkan.release_graph_exports,vulkan.create_pipeline,vulkan.destroy_pipeline,vulkan.create_sampler,vulkan.destroy_sampler},ui={vulkan.create_graphics_pipeline,vulkan.destroy_graphics_pipeline,vulkan.create_buffer_with_data,vulkan.destroy_buffer,vulkan.create_texture_with_data,vulkan.destroy_texture,vulkan.create_sampler,vulkan.destroy_sampler,vulkan.create_texture},picking={vulkan.graph_texture_source,vulkan.queue_texture_readback,vulkan.poll_texture_readback,vulkan.destroy_readback},particles={vulkan.create_pipeline,vulkan.destroy_pipeline,vulkan.create_graphics_pipeline,vulkan.destroy_graphics_pipeline,vulkan.create_buffer_with_data,vulkan.destroy_buffer,vulkan.write_buffer,vulkan.read_buffer},attach=vulkan.attach_surface,resize=vulkan.resize_surface,detach=vulkan.detach_surface,acquire_surface=vulkan.acquire_surface,abort_surface=vulkan.abort_surface,present=vulkan.present_surface}
    if !run_editor(&renderer,api,config,&compiler,&fonts,true) || vulkan.validation_error_count(&renderer)!=0 { os.exit(1) }
}
