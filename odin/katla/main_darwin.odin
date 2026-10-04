#+build darwin, arm64
//! Darwin selects Metal or Vulkan while the canonical editor owner loop is shared.
package main
import render "../app/render"
import shader "../gfx/shader"
import metal "../gfx/metal"
import vulkan "../gfx/vulkan"
import NS "core:sys/darwin/Foundation"
import "core:fmt"
import "core:os"

main :: proc() {
    config,valid:=config_parse(os.args)
    defer config_destroy(&config)
    if config.help { config_help(); return }
    if !valid { fmt.eprintln("Usage: katla --shader-compiler <executable> --font-library <library> [--backend metal|vulkan] [--vulkan-loader <library>] [--project <directory>] [--resources <directory>] [--scene <project.katla>] [--preferences <directory>] [--luau-library <library>] [--box-library <library>]"); os.exit(2) }
    if config.gpu_validation { if os.set_env("MTL_DEBUG_LAYER","1")!=nil || os.set_env("METAL_DEVICE_WRAPPER_TYPE","1")!=nil { fmt.eprintln("Cannot enable Metal validation"); os.exit(1) } }
    pool:=NS.AutoreleasePool.alloc()->init(); defer pool->drain()
    compiler:shader.Compiler; if shader.compiler_init(&compiler,config.shader_compiler)!=.None { fmt.eprintln("Cannot initialize offline shader compiler"); os.exit(1) }; defer shader.compiler_destroy(&compiler)
    fonts:render.UI_Font_System; if render.ui_font_init(&fonts,config.font_library,config.resource_root)!=.None { fmt.eprintln("Cannot initialize native font library"); os.exit(1) }; defer render.ui_font_destroy(&fonts)
    success:=false
    if config.backend=="metal" {
        renderer:metal.Renderer; if metal.renderer_init(&renderer)!=.None { os.exit(1) }; defer metal.renderer_destroy(&renderer)
        api:=Backend_API(metal.Renderer){gpu={metal.create_graphics_pipeline,metal.destroy_graphics_pipeline,metal.create_buffer_with_data,metal.destroy_buffer,metal.write_buffer,metal.create_texture,metal.destroy_texture,metal.acquire,metal.abort,metal.submit,metal.wait,metal.release_graph_exports,metal.create_pipeline,metal.destroy_pipeline,metal.create_sampler,metal.destroy_sampler},ui={metal.create_graphics_pipeline,metal.destroy_graphics_pipeline,metal.create_buffer_with_data,metal.destroy_buffer,metal.create_texture_with_data,metal.destroy_texture,metal.create_sampler,metal.destroy_sampler,metal.create_texture},picking={metal.graph_texture_source,metal.queue_texture_readback,metal.poll_texture_readback,metal.destroy_readback},particles={metal.create_pipeline,metal.destroy_pipeline,metal.create_graphics_pipeline,metal.destroy_graphics_pipeline,metal.create_buffer_with_data,metal.destroy_buffer,metal.write_buffer,metal.read_buffer},attach=metal.attach_surface,resize=metal.resize_surface,detach=metal.detach_surface,acquire_surface=metal.acquire_surface,abort_surface=metal.abort_surface,present=metal.present_surface}
        success=run_editor(&renderer,api,config,&compiler,&fonts,false)
    } else {
        renderer:vulkan.Renderer; if vulkan.renderer_init(&renderer,validation=true,loader_path=config.vulkan_loader)!=.None { os.exit(1) }; defer vulkan.renderer_destroy(&renderer)
        api:=Backend_API(vulkan.Renderer){gpu={vulkan.create_graphics_pipeline,vulkan.destroy_graphics_pipeline,vulkan.create_buffer_with_data,vulkan.destroy_buffer,vulkan.write_buffer,vulkan.create_texture,vulkan.destroy_texture,vulkan.acquire,vulkan.abort,vulkan.submit,vulkan.wait,vulkan.release_graph_exports,vulkan.create_pipeline,vulkan.destroy_pipeline,vulkan.create_sampler,vulkan.destroy_sampler},ui={vulkan.create_graphics_pipeline,vulkan.destroy_graphics_pipeline,vulkan.create_buffer_with_data,vulkan.destroy_buffer,vulkan.create_texture_with_data,vulkan.destroy_texture,vulkan.create_sampler,vulkan.destroy_sampler,vulkan.create_texture},picking={vulkan.graph_texture_source,vulkan.queue_texture_readback,vulkan.poll_texture_readback,vulkan.destroy_readback},particles={vulkan.create_pipeline,vulkan.destroy_pipeline,vulkan.create_graphics_pipeline,vulkan.destroy_graphics_pipeline,vulkan.create_buffer_with_data,vulkan.destroy_buffer,vulkan.write_buffer,vulkan.read_buffer},attach=vulkan.attach_surface,resize=vulkan.resize_surface,detach=vulkan.detach_surface,acquire_surface=vulkan.acquire_surface,abort_surface=vulkan.abort_surface,present=vulkan.present_surface}
        success=run_editor(&renderer,api,config,&compiler,&fonts,true)
        if vulkan.validation_error_count(&renderer)!=0 { success=false }
    }
    if !success { os.exit(1) }
}
