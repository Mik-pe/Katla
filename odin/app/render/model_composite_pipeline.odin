//! Model blend phases operate on linear color before one final display encoding.
package render

import gfx "../../gfx"
import shader "../../gfx/shader"
import adapter "../../gfx/shader_adapter"
import "core:mem"

MODEL_COMPOSITE_SOURCE :: #load("shaders/model_composite.wgsl",string)
Model_Compositor :: struct { compiled:shader.Compiled, mappings:[2]adapter.Graphics, colors:[]gfx.Color_Target, allocator:mem.Allocator }
model_compositor_compile :: proc(compiler:^shader.Compiler,format:gfx.Texture_Format,allocator:mem.Allocator)->(Model_Compositor,shader.Error) {
    result:=Model_Compositor{allocator=allocator}
    success:=false; defer { if !success { model_compositor_destroy(&result) } }
    error:shader.Error
    result.compiled,error=shader.compile(compiler,MODEL_COMPOSITE_SOURCE,{{"vs_composite",.Vertex},{"fs_decode",.Fragment},{"fs_encode",.Fragment}},allocator=allocator)
    if error!=.None { return {},error }
    result.colors=make([]gfx.Color_Target,2,allocator)
    for &color,i in result.colors {
        color={format=.RGBA16_Float if i==0 else format,write_mask={.Red,.Green,.Blue,.Alpha}}
        adapter_error:adapter.Error
        result.mappings[i],adapter_error=adapter.graphics(&result.compiled,"vs_composite","fs_decode" if i==0 else "fs_encode",{colors=result.colors[i:i+1],topology=.Triangle_List,cull=.None,front_counter_clockwise=true},allocator)
        if adapter_error!=.None { return {},.Reflection }
    }
    success=true; return result,.None
}
model_compositor_destroy :: proc(owner:^Model_Compositor) {
    for &mapping in owner.mappings { adapter.graphics_destroy(&mapping) }
    shader.compiled_destroy(&owner.compiled); delete(owner.colors,owner.allocator); owner^={}
}
