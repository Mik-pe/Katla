//! Ordinary UI pipelines consume a shaped vertex stream and immutable texture snapshots.
package render

import gfx "../../gfx"
import shader "../../gfx/shader"
import adapter "../../gfx/shader_adapter"
import "core:mem"

UI_SHADER_SOURCE :: #load("shaders/ui.wgsl",string)
UI_TRANSFER_SOURCE :: #load("shaders/ui_transfer.wgsl",string)
UI_Frame_Data :: struct { logical_size:[2]f32, texture_index:u32, clip_y:f32, decode_sample:u32, _align:[3]u32, _pad:[3]u32, _end:u32 }
UI_Shader :: struct { compiled,transfer:shader.Compiled, mapped,final_mapped,decode_mapped:adapter.Graphics, color,final_color,decode_color:[]gfx.Color_Target, allocator:mem.Allocator }
/// Compiles selected UI entries once; no shader compilation belongs on a frame path.
ui_shader_compile :: proc(compiler:^shader.Compiler,format:gfx.Texture_Format,allocator:=context.allocator)->(UI_Shader,shader.Error) {
    if format!=.RGBA8_Unorm && format!=.BGRA8_Unorm { return {},.Reflection }
    compiled,error:=shader.compile(compiler,UI_SHADER_SOURCE,{{"vs_ui",.Vertex},{"fs_ui",.Fragment}},allocator=allocator)
    if error!=.None { shader.compiled_destroy(&compiled); return {},error }
    result:=UI_Shader{compiled=compiled,allocator=allocator,color=make([]gfx.Color_Target,1,allocator)}
    result.color[0]={format=.RGBA16_Float,write_mask={.Red,.Green,.Blue,.Alpha},blend_enabled=true,source_color=.Source_Alpha,destination_color=.One_Minus_Source_Alpha,source_alpha=.One,destination_alpha=.One_Minus_Source_Alpha}
    mapped,adapter_error:=adapter.graphics(&result.compiled,"vs_ui","fs_ui",{colors=result.color,topology=.Triangle_List,cull=.None},allocator)
    if adapter_error!=.None { ui_shader_destroy(&result); return {},.Reflection }
    result.mapped=mapped
    result.transfer,error=shader.compile(compiler,UI_TRANSFER_SOURCE,{{"vs_transfer",.Vertex},{"fs_encode",.Fragment},{"fs_decode",.Fragment}},allocator=allocator)
    if error!=.None { ui_shader_destroy(&result); return {},error }
    result.final_color=make([]gfx.Color_Target,1,allocator); result.final_color[0]={format=format,write_mask={.Red,.Green,.Blue,.Alpha}}
    result.final_mapped,adapter_error=adapter.graphics(&result.transfer,"vs_transfer","fs_encode",{colors=result.final_color,topology=.Triangle_List,cull=.None},allocator)
    if adapter_error!=.None { ui_shader_destroy(&result); return {},.Reflection }
    result.decode_color=make([]gfx.Color_Target,1,allocator); result.decode_color[0]={format=.RGBA16_Float,write_mask={.Red,.Green,.Blue,.Alpha}}
    result.decode_mapped,adapter_error=adapter.graphics(&result.transfer,"vs_transfer","fs_decode",{colors=result.decode_color,topology=.Triangle_List,cull=.None},allocator)
    if adapter_error!=.None { ui_shader_destroy(&result); return {},.Reflection }
    return result,.None
}
/// Native creation retains its own reflected artifacts before this temporary compilation is released.
ui_shader_destroy :: proc(input:^UI_Shader) { adapter.graphics_destroy(&input.mapped); adapter.graphics_destroy(&input.final_mapped); adapter.graphics_destroy(&input.decode_mapped); shader.compiled_destroy(&input.compiled); shader.compiled_destroy(&input.transfer); delete(input.color,input.allocator); delete(input.final_color,input.allocator); delete(input.decode_color,input.allocator); input^={} }
