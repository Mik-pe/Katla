//! Canonical WGSL particle stages retain the same serialized emitter and particle ABI.
package render

import gfx "../../gfx"
import shader "../../gfx/shader"
import adapter "../../gfx/shader_adapter"
import "core:strings"
import "core:mem"

PARTICLE_COMMON :: #load("../../../resources/shaders/particles/common.wgsl",string)
PARTICLE_EMIT :: #load("shaders/particles_emit.wgsl",string)
PARTICLE_SIMULATE :: #load("../../../resources/shaders/particles/particle_simulate.wgsl",string)
PARTICLE_DRAW :: #load("../../../resources/shaders/particles/particle_draw_command.wgsl",string)
PARTICLE_DISPATCH :: #load("shaders/particles_dispatch.wgsl",string)
PARTICLE_RENDER :: #load("shaders/particles_render.wgsl",string)
/// Native preparation borrows these immutable artifacts only during pipeline creation.
Particle_Shader :: struct {
    compiled:[4]shader.Compiled,
    compute:[4]adapter.Compute,
    rendering:shader.Compiled,
    graphics:adapter.Graphics,
    colors:[]gfx.Color_Target,
    allocator:mem.Allocator,
}
/// Compiles emission, simulation, command generation and actual billboard stages.
particle_shader_compile :: proc(compiler:^shader.Compiler,format:gfx.Texture_Format,allocator:=context.allocator)->(Particle_Shader,shader.Error) {
    result:=Particle_Shader{allocator=allocator}
    success:=false; defer { if !success { particle_shader_destroy(&result) } }
    for source,i in ([4]string{PARTICLE_EMIT,PARTICLE_SIMULATE,PARTICLE_DRAW,PARTICLE_DISPATCH}) {
        expanded,owned:=strings.replace_all(source,"#include \"common.wgsl\"",PARTICLE_COMMON,allocator)
        defer { if owned { delete(expanded,allocator) } }
        error:shader.Error
        result.compiled[i],error=shader.compile(compiler,expanded,{{"cs_main",.Compute}},allocator=allocator)
        if error!=.None { return {},error }
        map_error:adapter.Error
        result.compute[i],map_error=adapter.compute(&result.compiled[i],"cs_main",allocator)
        if map_error!=.None { return {},.Reflection }
    }
    expanded,owned:=strings.replace_all(PARTICLE_RENDER,"#include \"common.wgsl\"",PARTICLE_COMMON,allocator); defer { if owned { delete(expanded,allocator) } }
    error:shader.Error
    result.rendering,error=shader.compile(compiler,expanded,{{"vs_main",.Vertex},{"fs_main",.Fragment}},allocator=allocator)
    if error!=.None { return {},error }
    result.colors=make([]gfx.Color_Target,1,allocator)
    result.colors[0]={format=format,write_mask={.Red,.Green,.Blue,.Alpha},blend_enabled=true,source_color=.Source_Alpha,destination_color=.One_Minus_Source_Alpha,source_alpha=.One,destination_alpha=.One_Minus_Source_Alpha}
    map_error:adapter.Error
    result.graphics,map_error=adapter.graphics(&result.rendering,"vs_main","fs_main",{colors=result.colors,depth={enabled=true,test=true,write=false,compare=.Less_Equal,format=.D32_Float},topology=.Triangle_List,cull=.None},allocator)
    if map_error!=.None { return {},.Reflection }
    success=true; return result,.None
}
/// Releases compiled source, target binaries and mapping arrays with their captured allocators.
particle_shader_destroy :: proc(value:^Particle_Shader) {
    for &mapping in value.compute { adapter.compute_destroy(&mapping) }
    for &compiled in value.compiled { shader.compiled_destroy(&compiled) }
    delete(value.colors,value.allocator); adapter.graphics_destroy(&value.graphics)
    shader.compiled_destroy(&value.rendering); value^={}
}
