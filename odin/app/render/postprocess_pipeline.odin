//! Canonical selected WGSL entries own the display transform's native artifacts.
package render

import gfx "../../gfx"
import shader "../../gfx/shader"
import adapter "../../gfx/shader_adapter"
import "core:mem"

POSTPROCESS_SOURCE :: #load("shaders/postprocess.wgsl",string)
Postprocess_Shader :: struct { compiled:shader.Compiled, mapping:adapter.Graphics, colors:[]gfx.Color_Target, allocator:mem.Allocator }
postprocess_shader_compile :: proc(compiler:^shader.Compiler,format:gfx.Texture_Format,allocator:mem.Allocator=context.allocator)->(Postprocess_Shader,shader.Error) {
    if format!=.RGBA8_Unorm && format!=.BGRA8_Unorm { return {},.Invalid_Request }
    result:=Postprocess_Shader{allocator=allocator}
    success:=false; defer { if !success { postprocess_shader_destroy(&result) } }
    error:shader.Error
    result.compiled,error=shader.compile(compiler,POSTPROCESS_SOURCE,{{"vs_display",.Vertex},{"fs_display",.Fragment}},allocator=allocator)
    if error!=.None { return {},error }
    result.colors=make([]gfx.Color_Target,1,allocator); result.colors[0]={format=format,write_mask={.Red,.Green,.Blue,.Alpha}}
    map_error:adapter.Error
    result.mapping,map_error=adapter.graphics(&result.compiled,"vs_display","fs_display",{colors=result.colors,topology=.Triangle_List,cull=.None},allocator)
    if map_error!=.None { return {},.Reflection }
    success=true; return result,.None
}
postprocess_shader_destroy :: proc(value:^Postprocess_Shader) { adapter.graphics_destroy(&value.mapping); shader.compiled_destroy(&value.compiled); delete(value.colors,value.allocator); value^={} }
