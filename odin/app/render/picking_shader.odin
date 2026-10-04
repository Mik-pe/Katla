//! Picking shaders use ordinary buffers and an integer attachment owned by the application.
package render

import gfx "../../gfx"
import shader "../../gfx/shader"
import adapter "../../gfx/shader_adapter"
import "core:mem"

PICKING_SHADER_SOURCE :: #load("shaders/picking.wgsl",string)
PICKING_MASK_SHADER_SOURCE :: #load("shaders/picking_mask.wgsl",string)
Picking_Mask_Uniform :: struct { picking:Picking_Uniform,uv_offset,vertex_alpha_offset,object_alpha_offset,vertex_alpha_enabled:u32,cutoff:f32,padding:[3]u32 }
Picking_Uniform :: struct { vertex_stride,object_stride,position_offset,model_offset,object_index,encoded:u32, padding:[2]u32 }
Picking_Shader :: struct { compiled:shader.Compiled, mapped:adapter.Graphics, colors:[]gfx.Color_Target, allocator:mem.Allocator }
/// Compiles the integer picking attachment and independent depth testing once.
picking_shader_compile :: proc(compiler:^shader.Compiler,allocator:=context.allocator,masked:=false,cull:=gfx.Cull_Mode.None,front_counter_clockwise:=false)->(Picking_Shader,shader.Error) {
    compiled,error:=shader.compile(compiler,PICKING_MASK_SHADER_SOURCE if masked else PICKING_SHADER_SOURCE,{{"vs_pick",.Vertex},{"fs_pick",.Fragment}},allocator=allocator)
    if error!=.None { shader.compiled_destroy(&compiled); return {},error }
    result:=Picking_Shader{compiled=compiled,allocator=allocator,colors=make([]gfx.Color_Target,1,allocator)}
    result.colors[0]={format=.R32_Uint,write_mask={.Red}}
    mapped,adapter_error:=adapter.graphics(&result.compiled,"vs_pick","fs_pick",{colors=result.colors,depth={enabled=true,test=true,write=true,compare=.Less,format=.D32_Float},topology=.Triangle_List,cull=cull,front_counter_clockwise=front_counter_clockwise},allocator)
    if adapter_error!=.None { picking_shader_destroy(&result); return {},.Reflection }
    result.mapped=mapped; return result,.None
}
/// Releases temporary compilation after a native pipeline retained its selected entries.
picking_shader_destroy :: proc(input:^Picking_Shader) { adapter.graphics_destroy(&input.mapped); shader.compiled_destroy(&input.compiled); delete(input.colors,input.allocator); input^={} }
