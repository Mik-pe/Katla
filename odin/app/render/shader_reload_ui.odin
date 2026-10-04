//! UI candidates replace all three native stages without rebuilding fonts, atlas or textures.
package render

import gfx "../../gfx"
import shader "../../gfx/shader"
import adapter "../../gfx/shader_adapter"
import "core:mem"
import "core:log"

@(private="package")
ui_shader_reload_mapping :: proc(compiled:^shader.Compiled,descriptor:gfx.Graphics_Desc,allocator:mem.Allocator)->(adapter.Graphics,adapter.Error) {
    return adapter.graphics(compiled,descriptor.vertex_entry,descriptor.fragment_entry,shader_reload_graphics_state(descriptor),allocator)
}
@(private="package")
ui_shader_reload_report :: proc(error:gfx.Gpu_Error,cleanup:bool) {
    if cleanup { log.error("UI shader pipeline cleanup failed",error) } else { log.warn("UI shader candidate native creation failed",error) }
}
UI_Shader_Reload_Candidate :: struct($R:typeid) { owner:^UI_GPU(R),pipelines:[3]gfx.Graphics_Pipeline_Handle,allocator:mem.Allocator }
/// Prepares the entire UI/decode/encode set against the immutable application's original ABI.
ui_shader_reload_prepare :: proc(owner:^UI_GPU($R),reference:^UI_Shader,artifacts:[]shader.Compiled)->(^UI_Shader_Reload_Candidate(R),Shader_Reload_Error) {
    if owner==nil || owner.renderer==nil || reference==nil || len(artifacts)!=2 { return nil,.Prepare }
    if owner.prepared { return nil,.Busy }
    if !shader_reload_interface_compatible(&reference.compiled,&artifacts[0]) || !shader_reload_interface_compatible(&reference.transfer,&artifacts[1]) { return nil,.Prepare }
    candidate:=new(UI_Shader_Reload_Candidate(R),owner.allocator);candidate.owner=owner;candidate.allocator=owner.allocator
    descriptors:=[3]gfx.Graphics_Desc{reference.mapped.descriptor,reference.final_mapped.descriptor,reference.decode_mapped.descriptor}
    for descriptor,index in descriptors {
        artifact:=&artifacts[0] if index==0 else &artifacts[1]
        mapping,error:=ui_shader_reload_mapping(artifact,descriptor,owner.allocator)
        if error!=.None { return candidate,.Prepare }
        handle,native_error:=owner.ops.create_pipeline(owner.renderer,mapping.descriptor);adapter.graphics_destroy(&mapping)
        if native_error!=.None { ui_shader_reload_report(native_error,false);return candidate,.Prepare }
        candidate.pipelines[index]=handle
    }
    return candidate,.None
}
/// Called only after every aggregate editor candidate prepares, before any frame acquires UI packets.
ui_shader_reload_publish :: proc(candidate:^UI_Shader_Reload_Candidate($R))->^UI_Shader_Reload_Candidate(R) {
    owner:=candidate.owner
    previous:=[3]gfx.Graphics_Pipeline_Handle{owner.pipeline,owner.transfer,owner.decode}
    owner.pipeline,owner.transfer,owner.decode=candidate.pipelines[0],candidate.pipelines[1],candidate.pipelines[2]
    candidate.pipelines=previous;return candidate
}
/// Removes unpublished or previous public handles; accepted native GPU records retain their resources.
ui_shader_reload_destroy :: proc(candidate:^UI_Shader_Reload_Candidate($R)) {
    if candidate==nil { return }
    for handle in candidate.pipelines { if handle.owner!=nil { error:=candidate.owner.ops.destroy_pipeline(candidate.owner.renderer,handle);if error!=.None { ui_shader_reload_report(error,true) } } }
    free(candidate,candidate.allocator)
}
