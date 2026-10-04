//! The scene's canonical shader adapter verifies its application ABI before native publication.
package render

import gfx "../../gfx"
import shader "../../gfx/shader"
import adapter "../../gfx/shader_adapter"
import "core:mem"

/// Application-owned WGSL; all backend binaries/reflection come from the same validated source.
SURFACE_SOURCE :: #load("shaders/surface.wgsl",string)
/// Keeps compiled binaries and reflected descriptor arrays alive through native preparation.
Surface_Shader :: struct { compiled:shader.Compiled, mapping:adapter.Graphics, descriptor:gfx.Graphics_Desc, allocator:mem.Allocator }
/// Compiles selected real PBR stages and checks the exact frame/object/geometry ABI.
surface_shader_compile :: proc(compiler:^shader.Compiler,format:=gfx.Texture_Format.RGBA8_Unorm,allocator:=context.allocator)->(Surface_Shader,shader.Error) {
    if !(format in (bit_set[gfx.Texture_Format]{.RGBA8_Unorm,.BGRA8_Unorm})) { return {},.Invalid_Request }
    compiled,error:=shader.compile(compiler,SURFACE_SOURCE,{{"vs_main",.Vertex},{"fs_main",.Fragment}},allocator=allocator)
    if error!=.None { shader.compiled_destroy(&compiled); return {},error }
    result:=Surface_Shader{compiled=compiled,allocator=allocator}
    success:=false; defer { if !success { surface_shader_destroy(&result) } }
    if len(compiled.entries)!=2 { return {},.Reflection }
    for entry in compiled.entries {
        if entry.stage!=.Vertex && entry.stage!=.Fragment { return {},.Reflection }
        expected_count:=3 if entry.stage==.Vertex else 2
        if len(entry.bindings)!=expected_count { return {},.Reflection }
        for binding in entry.bindings {
            if binding.group!=0 || binding.binding>=u32(expected_count) || binding.kind!=.Buffer || binding.metal_kind!=.Buffer || binding.array_count!=1 || binding.access!=.Read { return {},.Reflection }
            slot:=binding.binding
            expected_size:=u64(size_of(Frame_Data)) if slot==0 else (u64(size_of(Object_Data)) if slot==1 else u64(size_of(Vertex)))
            if binding.minimum_size!=expected_size || binding.uniform!=(slot==0) || binding.runtime_array!=(slot!=0) { return {},.Reflection }
        }
    }
    result.descriptor.colors=make([]gfx.Color_Target,1,allocator)
    result.descriptor.colors[0]={format=format,write_mask={.Red,.Green,.Blue,.Alpha}}
    mapping,adapter_error:=adapter.graphics(&result.compiled,"vs_main","fs_main",{colors=result.descriptor.colors,depth={enabled=true,test=true,write=true,compare=.Less,format=.D32_Float},topology=.Triangle_List,cull=.Back,front_counter_clockwise=true},allocator)
    if adapter_error!=.None { return {},.Reflection }
    result.mapping=mapping; result.descriptor=mapping.descriptor
    success=true; return result,.None
}
/// Native pipeline owners must finish preparation before the temporary shader artifact is freed.
surface_shader_destroy :: proc(surface:^Surface_Shader) {
    adapter.graphics_destroy(&surface.mapping); delete(surface.descriptor.colors,surface.allocator)
    shader.compiled_destroy(&surface.compiled); surface^={}
}
