//! Integer picking stages and material visibility variants publish together as one native family.
package render

import gfx "../../gfx"
import shader "../../gfx/shader"
import adapter "../../gfx/shader_adapter"
import "core:mem"
import "core:log"

@(private="package")
picking_shader_reload_mapping :: proc(compiled:^shader.Compiled,variant:int,colors:[]gfx.Color_Target,allocator:mem.Allocator,sense:Depth_Sense)->(adapter.Graphics,adapter.Error) {
    depth:=depth_descriptor({depth={enabled=true,test=true,write=true,compare=.Less,format=.D32_Float}},sense).depth
    state:=adapter.Graphics_State{colors=colors,depth=depth,topology=.Triangle_List,cull=.None if variant%2==1 else .Back,front_counter_clockwise=variant<2}
    return adapter.graphics(compiled,"vs_pick","fs_pick",state,allocator)
}
@(private="package")
picking_shader_reload_report :: proc(error:gfx.Gpu_Error,cleanup:bool) {
    if cleanup { log.error("Picking shader pipeline cleanup failed",error) } else { log.warn("Picking shader candidate native creation failed",error) }
}
Picking_Shader_Reload_Candidate :: struct($R:typeid) { owner:^Picking_Native(R),pipelines,reverse_pipelines:Picking_Pipelines,allocator:mem.Allocator }
/// The caller supplies original opaque/masked compiled interfaces retained from startup.
picking_shader_reload_prepare :: proc(owner:^Picking_Native($R),references:[2]^shader.Compiled,artifacts:[]shader.Compiled,allocator:=context.allocator)->(^Picking_Shader_Reload_Candidate(R),Shader_Reload_Error) {
    if owner==nil || owner.renderer==nil || len(artifacts)!=2 { return nil,.Prepare }
    for &artifact,index in artifacts { if !shader_reload_interface_compatible(references[index],&artifact) { return nil,.Prepare } }
    candidate:=new(Picking_Shader_Reload_Candidate(R),allocator);candidate.owner=owner;candidate.allocator=allocator
    color:=[1]gfx.Color_Target{{format=.R32_Uint,write_mask={.Red}}}
    for sense in 0..<2 { for masked in 0..<2 { for variant in 0..<4 {
        mapping,error:=picking_shader_reload_mapping(&artifacts[masked],variant,color[:],allocator,Depth_Sense(sense));if error!=.None { return candidate,.Prepare }
        handle,native_error:=owner.create(owner.renderer,mapping.descriptor);adapter.graphics_destroy(&mapping)
        if native_error!=.None { picking_shader_reload_report(native_error,false);return candidate,.Prepare }
        set:=&candidate.pipelines if sense==0 else &candidate.reverse_pipelines
        if masked==0 { set.opaque[variant]=handle } else { set.masked[variant]=handle }
    } } }
    return candidate,.None
}
/// Changes future picking draws atomically; committed snapshots keep their accepted map and pixels.
picking_shader_reload_publish :: proc(candidate:^Picking_Shader_Reload_Candidate($R))->^Picking_Shader_Reload_Candidate(R) {
    forward,reverse:=candidate.owner.pipelines,candidate.owner.reverse_pipelines
    candidate.owner.pipelines=candidate.pipelines;candidate.owner.reverse_pipelines=candidate.reverse_pipelines
    candidate.pipelines,candidate.reverse_pipelines=forward,reverse;return candidate
}
/// Native recordings retain old variants while their public shader family is removed.
picking_shader_reload_destroy :: proc(candidate:^Picking_Shader_Reload_Candidate($R)) {
    if candidate==nil { return }
    for set in ([2]Picking_Pipelines{candidate.pipelines,candidate.reverse_pipelines}) {
        for handle in set.opaque { if handle.owner!=nil { error:=candidate.owner.destroy(candidate.owner.renderer,handle);if error!=.None { picking_shader_reload_report(error,true) } } }
        for handle in set.masked { if handle.owner!=nil { error:=candidate.owner.destroy(candidate.owner.renderer,handle);if error!=.None { picking_shader_reload_report(error,true) } } }
    }
    free(candidate,candidate.allocator)
}
