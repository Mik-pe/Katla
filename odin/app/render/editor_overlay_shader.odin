//! Canonical offline compilation creates genuine HDR depth-tested and always-visible overlay pipelines.
package render

import gfx "../../gfx"
import shader "../../gfx/shader"
import adapter "../../gfx/shader_adapter"
import "core:mem"

OVERLAY_SOURCE :: #load("shaders/editor_overlay.wgsl",string)
Overlay_Shader :: struct { compiled:shader.Compiled,mappings:[2]adapter.Graphics,colors:[]gfx.Color_Target,allocator:mem.Allocator }
/// Both overlay phases share selected entry ABI and differ only in their actual depth comparison.
overlay_shader_compile :: proc(compiler:^shader.Compiler,allocator:=context.allocator)->(Overlay_Shader,shader.Error) {
    value:=Overlay_Shader{allocator=allocator}; success:=false; defer { if !success { overlay_shader_destroy(&value) } }
    error:shader.Error
    value.compiled,error=shader.compile(compiler,OVERLAY_SOURCE,{{"vs_overlay",.Vertex},{"fs_overlay",.Fragment}},allocator=allocator); if error!=.None { return {},error }
    value.colors=make([]gfx.Color_Target,1,allocator)
    value.colors[0]={format=.RGBA16_Float,write_mask={.Red,.Green,.Blue,.Alpha},blend_enabled=true,source_color=.Source_Alpha,destination_color=.One_Minus_Source_Alpha,source_alpha=.One,destination_alpha=.One_Minus_Source_Alpha}
    for &mapping,i in value.mappings {
        compare:=gfx.Compare_Op.Less_Equal if i==0 else gfx.Compare_Op.Always
        descriptor:=adapter.Graphics_State{colors=value.colors,depth={enabled=true,test=true,write=false,compare=compare,format=.D32_Float_S8_Uint},topology=.Triangle_List,cull=.None}
        map_error:adapter.Error
        mapping,map_error=adapter.graphics(&value.compiled,"vs_overlay","fs_overlay",descriptor,allocator); if map_error!=.None { return {},.Reflection }
    }
    success=true; return value,.None
}
/// Native owners retain selected artifacts independently of temporary compiler ownership.
overlay_shader_destroy :: proc(value:^Overlay_Shader) { for &mapping in value.mappings { adapter.graphics_destroy(&mapping) }; shader.compiled_destroy(&value.compiled); delete(value.colors,value.allocator); value^={} }
