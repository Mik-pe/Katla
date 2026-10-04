//! Particle and overlay shader-only candidates preserve accepted simulation, fonts and immutable upload owners.
package render

import gfx "../../gfx"
import shader "../../gfx/shader"

Particle_Shader_Reload_Candidate :: struct($R:typeid) { owner:^Particle_Consumer(R),next:Particle_Shader,rasters:[2]Shader_Raster_Replacement,computes:[4]Shader_Compute_Replacement,allocator:type_of(context.allocator) }
/// Module order is Emit, Simulate, DrawCommand, DispatchCommand, Render.
particle_shader_reload_prepare :: proc(owner:^Particle_Consumer($R),artifacts:[]shader.Compiled)->(^Particle_Shader_Reload_Candidate(R),Shader_Reload_Error) {
    if owner==nil || owner.renderer==nil || len(artifacts)!=5 { return nil,.Prepare }
    if owner.pending.ready { return nil,.Busy }
    for &reference,index in owner.shaders.compiled { if !shader_reload_interface_compatible(&reference,&artifacts[index]) { return nil,.Prepare } }
    if !shader_reload_interface_compatible(&owner.shaders.rendering,&artifacts[4]) { return nil,.Prepare }
    candidate:=new(Particle_Shader_Reload_Candidate(R),owner.allocator); candidate.owner=owner; candidate.allocator=owner.allocator; candidate.next.allocator=owner.allocator
    for &compiled,index in candidate.next.compiled { compiled=shader_reload_snapshot(&artifacts[index],owner.allocator) }
    candidate.next.rendering=shader_reload_snapshot(&artifacts[4],owner.allocator); candidate.next.colors=shader_reload_colors(owner.shaders.colors,owner.allocator)
    for &mapping,index in candidate.next.compute {
        error:Shader_Reload_Error
        mapping,error=shader_reload_compute_map(&candidate.next.compiled[index],owner.shaders.compute[index].descriptor.entry,owner.allocator); if error!=.None { return candidate,error }
        handle,native_error:=owner.operations.create_compute(owner.renderer,mapping.descriptor); if native_error!=.None { shader_reload_native_report(native_error,false); return candidate,.Prepare }
        candidate.computes[index]={&owner.pipelines[index],handle}
    }
    descriptor:=owner.shaders.graphics.descriptor; descriptor.colors=candidate.next.colors
    error:Shader_Reload_Error
    candidate.next.graphics,error=shader_reload_map(&candidate.next.rendering,descriptor,owner.allocator); if error!=.None { return candidate,error }
    for sense in 0..<2 {
        handle,native_error:=owner.operations.create_graphics(owner.renderer,depth_descriptor(candidate.next.graphics.descriptor,Depth_Sense(sense))); if native_error!=.None { shader_reload_native_report(native_error,false); return candidate,.Prepare }
        candidate.rasters[sense]={&owner.pipeline if sense==0 else &owner.reverse_pipeline,handle}
    }
    return candidate,.None
}
/// Changes only future GPU programs; particles, counters, clocks, burst prefixes and rollover stay exact.
particle_shader_reload_publish :: proc(candidate:^Particle_Shader_Reload_Candidate($R),graph:^gfx.Graph=nil)->^Particle_Shader_Reload_Candidate(R) {
    for &replacement in candidate.rasters { replacement.target^,replacement.handle=replacement.handle,replacement.target^ }
    for &replacement in candidate.computes { replacement.target^,replacement.handle=replacement.handle,replacement.target^ }
    candidate.owner.shaders,candidate.next=candidate.next,candidate.owner.shaders
    if graph!=nil { shader_reload_graph_references(graph,candidate.rasters[:],candidate.computes[:]) }
    return candidate
}
/// Failed and previous candidates release their own artifacts and public native parents.
particle_shader_reload_destroy :: proc(candidate:^Particle_Shader_Reload_Candidate($R)) {
    if candidate==nil { return }
    for replacement in candidate.rasters { if replacement.handle.owner!=nil { error:=candidate.owner.operations.destroy_graphics(candidate.owner.renderer,replacement.handle); if error!=.None { shader_reload_native_report(error,true) } } }
    for replacement in candidate.computes { if replacement.handle.owner!=nil { error:=candidate.owner.operations.destroy_compute(candidate.owner.renderer,replacement.handle); if error!=.None { shader_reload_native_report(error,true) } } }
    particle_shader_destroy(&candidate.next); free(candidate,candidate.allocator)
}

Overlay_Shader_Reload_Candidate :: struct($R:typeid) { owner:^Overlay_Native(R),reference:^Overlay_Shader,next:Overlay_Shader,rasters:[3]Shader_Raster_Replacement,allocator:type_of(context.allocator) }
/// Glyph coverage and upload ownership are unchanged; both depth senses and Always prepare together.
overlay_shader_reload_prepare :: proc(owner:^Overlay_Native($R),reference:^Overlay_Shader,artifacts:[]shader.Compiled)->(^Overlay_Shader_Reload_Candidate(R),Shader_Reload_Error) {
    if owner==nil || owner.renderer==nil || reference==nil || len(artifacts)!=1 || !shader_reload_interface_compatible(&reference.compiled,&artifacts[0]) { return nil,.Prepare }
    if owner.frames>0 { return nil,.Busy }
    candidate:=new(Overlay_Shader_Reload_Candidate(R),owner.allocator); candidate^={owner=owner,reference=reference,allocator=owner.allocator}
    candidate.next={compiled=shader_reload_snapshot(&artifacts[0],owner.allocator),colors=shader_reload_colors(reference.colors,owner.allocator),allocator=owner.allocator}
    for &mapping,index in candidate.next.mappings {
        descriptor:=reference.mappings[index].descriptor; descriptor.colors=candidate.next.colors
        error:Shader_Reload_Error
        mapping,error=shader_reload_map(&candidate.next.compiled,descriptor,owner.allocator); if error!=.None { return candidate,error }
    }
    for index in 0..<3 {
        descriptor:=candidate.next.mappings[index if index<2 else 0].descriptor; if index==2 { descriptor=depth_descriptor(descriptor,.Reverse) }
        handle,error:=owner.ops.create_pipeline(owner.renderer,descriptor); if error!=.None { shader_reload_native_report(error,false); return candidate,.Prepare }
        candidate.rasters[index]={&owner.pipelines[index],handle}
    }
    return candidate,.None
}
/// The aggregate commits this only after every scene, model, UI, picking and particle candidate succeeds.
overlay_shader_reload_publish :: proc(candidate:^Overlay_Shader_Reload_Candidate($R),graph:^gfx.Graph=nil)->^Overlay_Shader_Reload_Candidate(R) {
    for &replacement in candidate.rasters { replacement.target^,replacement.handle=replacement.handle,replacement.target^ }
    candidate.reference^,candidate.next=candidate.next,candidate.reference^
    if graph!=nil { shader_reload_graph_references(graph,candidate.rasters[:],nil) }
    return candidate
}
/// Pending GPU work retains its previous glyph/pipeline owners independently of this public candidate.
overlay_shader_reload_destroy :: proc(candidate:^Overlay_Shader_Reload_Candidate($R)) {
    if candidate==nil { return }
    for replacement in candidate.rasters { if replacement.handle.owner!=nil { error:=candidate.owner.ops.destroy_pipeline(candidate.owner.renderer,replacement.handle); if error!=.None { shader_reload_native_report(error,true) } } }
    overlay_shader_destroy(&candidate.next); free(candidate,candidate.allocator)
}
