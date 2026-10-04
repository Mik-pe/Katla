//! Complete source-model materials use one canonical shader and explicit raster variants.
package render

import gfx "../../gfx"
import shader "../../gfx/shader"
import adapter "../../gfx/shader_adapter"
import "core:mem"

MODEL_SOURCE :: #load("shaders/model.wgsl",string)
/// Retains exact native binaries and descriptor metadata through model publication.
Model_Shader :: struct {
    compiled:shader.Compiled,
    compositor:Model_Compositor,
    mappings:[8]adapter.Graphics,
    descriptors:[8]gfx.Graphics_Desc,
    colors:[]gfx.Color_Target,
    allocator:mem.Allocator,
}
/// Selects double-sided and transparent raster state without altering source material identity.
model_pipeline_variant :: proc(double_sided,transparent:bool,mirrored:=false)->int { return int(double_sided)+2*int(transparent)+4*int(mirrored) }
/// Compiles textured metallic/roughness and specular/glossiness materials before frame execution.
model_shader_compile :: proc(compiler:^shader.Compiler,format:=gfx.Texture_Format.RGBA8_Unorm,allocator:=context.allocator)->(Model_Shader,shader.Error) {
    if !(format in (bit_set[gfx.Texture_Format]{.RGBA8_Unorm,.BGRA8_Unorm})) { return {},.Invalid_Request }
    compiled,error:=shader.compile(compiler,MODEL_SOURCE,{{"vs_model",.Vertex},{"fs_model",.Fragment}},allocator=allocator)
    if error!=.None { shader.compiled_destroy(&compiled); return {},error }
    result:=Model_Shader{compiled=compiled,allocator=allocator}
    success:=false; defer { if !success { model_shader_destroy(&result) } }
    if len(compiled.entries)!=2 { return {},.Reflection }
    for entry in compiled.entries {
        expected_count:=3 if entry.stage==.Vertex else 12
        if len(entry.bindings)!=expected_count { return {},.Reflection }
        for binding in entry.bindings {
            if binding.array_count!=1 { return {},.Reflection }
            if binding.group==0 {
                if binding.binding>2 || binding.kind!=.Buffer || binding.access!=.Read { return {},.Reflection }
                expected_size:=u64(128) if binding.binding==0 else (u64(208) if binding.binding==1 else u64(144))
                if binding.minimum_size!=expected_size || binding.uniform!=(binding.binding==0) || binding.runtime_array!=(binding.binding!=0) { return {},.Reflection }
            } else {
                if binding.group!=1 || binding.binding>9 || (binding.binding<5 && binding.kind!=.Texture) || (binding.binding>=5 && binding.kind!=.Sampler) { return {},.Reflection }
            }
        }
    }
    result.colors=make([]gfx.Color_Target,8,allocator)
    for i in 0..<8 {
        transparent:=i%4>=2
        result.colors[i]={format=.RGBA16_Float,write_mask={.Red,.Green,.Blue,.Alpha},blend_enabled=transparent,source_color=.Source_Alpha,destination_color=.One_Minus_Source_Alpha,source_alpha=.One,destination_alpha=.One_Minus_Source_Alpha}
        mapped,adapter_error:=adapter.graphics(&result.compiled,"vs_model","fs_model",{colors=result.colors[i:i+1],depth={enabled=true,test=true,write=!transparent,compare=.Less,format=.D32_Float},topology=.Triangle_List,cull=.None if i%2==1 else .Back,front_counter_clockwise=i<4},allocator)
        if adapter_error!=.None { return {},.Reflection }
        result.mappings[i]=mapped; result.descriptors[i]=mapped.descriptor
    }
    result.compositor,error=model_compositor_compile(compiler,format,allocator)
    if error!=.None { return {},error }
    success=true; return result,.None
}
/// Native pipeline creation must retain its own artifacts before this temporary owner is released.
model_shader_destroy :: proc(model:^Model_Shader) {
    for &mapping in model.mappings { adapter.graphics_destroy(&mapping) }
    model_compositor_destroy(&model.compositor)
    delete(model.colors,model.allocator); shader.compiled_destroy(&model.compiled); model^={}
}
