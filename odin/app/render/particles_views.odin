//! A single particle simulation feeds independent view cameras before their final HDR transform.
package render

import gfx "../../gfx"
import "core:mem"

/// Owns one camera upload per acquired native slot; simulation ownership stays with the consumer.
Particle_View :: struct($R:typeid) { consumer:^Particle_Consumer(R), cameras:[]gfx.Buffer_Handle, token:gfx.Frame_Token, prepared:bool, inputs:[1]gfx.Buffer_Input, allocator:mem.Allocator }
@(private="package")
particle_view_camera_bytes :: proc(values:[]Particle_Camera)->[]byte { return mem.slice_to_bytes(values) }
/// Allocates actual per-view camera resources before any frame preparation.
particle_view_init :: proc(view:^Particle_View($R),consumer:^Particle_Consumer(R))->gfx.Gpu_Error {
    if view.consumer!=nil || consumer==nil || len(consumer.slots)==0 { return .Invalid_Resource }
    view.consumer=consumer; view.allocator=consumer.allocator
    view.cameras=make([]gfx.Buffer_Handle,len(consumer.slots),view.allocator)
    zero:[112]byte
    success:=false; defer { if !success { particle_view_destroy(view) } }
    for &camera in view.cameras {
        error:gfx.Gpu_Error
        camera,error=consumer.operations.create_buffer(consumer.renderer,{size=112,usage={.Uniform},memory=.CPU_Visible},zero[:]); if error!=.None { return error }
    }
    success=true; return .None
}
/// Releases camera parents after the host drains its accepted shared submissions.
particle_view_destroy :: proc(view:^Particle_View($R))->gfx.Gpu_Error {
    if view.prepared { return .Busy }
    error:=gfx.Gpu_Error.None
    if view.consumer!=nil { for camera in view.cameras { if camera.owner!=nil { next:=view.consumer.operations.destroy_buffer(view.consumer.renderer,camera); if next!=.None { error=next } } } }
    delete(view.cameras,view.allocator); view^={}; return error
}
@(private="package")
particle_view_prepare :: proc(view:^Particle_View($R),scene:^Native_Scene(R),token:gfx.Frame_Token,frame:Frame_Data)->(Scene_Inputs,Native_Error) {
    consumer:=view.consumer
    if consumer==nil || view.prepared || !consumer.pending.ready || consumer.pending.token!=token || token.slot<0 || token.slot>=len(view.cameras) || consumer.graph==nil || scene_graph_target(consumer.graph)!=scene_graph_target(&scene.graph) { return {},{scene=.Invalid_Geometry} }
    camera,valid:=particle_camera(frame); if !valid { return {},{scene=.Invalid_Camera} }
    values:=[1]Particle_Camera{camera}
    upload_error:=consumer.operations.write_buffer(consumer.renderer,token,view.cameras[token.slot],0,particle_view_camera_bytes(values[:])); if upload_error!=.None { return {},{gpu=upload_error} }
    error:=scene_graph_extend(&scene.graph); if error!={} { return {},error }
    graph:=scene_graph_target(&scene.graph)
    camera_id,graph_error:=gfx.graph_buffer(graph,{size=112,usage={.Uniform},memory=.CPU_Visible},true,false); if graph_error!=.None { return {},{gpu=.Invalid_Graph} }
    r:=consumer.resources
    bindings:=[3]gfx.Stage_Buffer_Binding{{0,0,{.Vertex},{r.data,{0,u64(consumer.capacity)*64},.Read,.Storage}},{0,2,{.Vertex},{r.alive,{0,u64(consumer.capacity)*4},.Read,.Storage}},{1,0,{.Vertex},{camera_id,{0,112},.Read,.Uniform}}}
    command:=gfx.Buffer_Access{r.indirect,{0,16},.Read,.Indirect}
    color:=gfx.Color_Attachment{{scene.graph.color,gfx.image_full_range(scene.graph.color_desc),.Read_Write,.Color_Attachment},.Load,.Store,{}}
    depth:=gfx.Depth_Attachment{true,{scene.graph.depth,gfx.image_full_range(scene.graph.depth_desc),.Read_Write,.Depth_Attachment},.Load,.Store,1,0}
    pass,pass_error:=scene_graph_pass(&scene.graph,"Particle view billboards",.Graphics,{bindings[0].access,bindings[1].access,bindings[2].access,command},images={color.access,depth.access}); if pass_error!=.None { return {},{gpu=.Invalid_Graph} }
    packet_error:=gfx.graph_set_packet(graph,pass,gfx.Render{colors={color},depth=depth,buffers=bindings[:],phases={{pipeline=(consumer.reverse_pipeline if scene.graph.depth_sense==.Reverse else consumer.pipeline),draws={gfx.Draw_Indirect{command=command,count=1,stride=16}}}}}); if packet_error!=.None { return {},{packet=packet_error} }
    view.token=token; view.prepared=true
    view.inputs[0]={camera_id,view.cameras[token.slot]}
    return {buffers=view.inputs[:]},{}
}
@(private="package")
particle_view_prepare_callback :: proc($R:typeid)->proc(rawptr,^Native_Scene(R),gfx.Frame_Token,Frame_Data)->(Scene_Inputs,Native_Error) { return proc(state:rawptr,scene:^Native_Scene(R),token:gfx.Frame_Token,frame:Frame_Data)->(Scene_Inputs,Native_Error) { return particle_view_prepare(cast(^Particle_View(R))state,scene,token,frame) } }
@(private="package")
particle_view_accept_callback :: proc($R:typeid)->proc(rawptr,gfx.Submission) { return proc(state:rawptr,submission:gfx.Submission) { view:=cast(^Particle_View(R))state; assert(view.prepared && view.token==submission.token); view.prepared=false; view.token={} } }
@(private="package")
particle_view_abort_callback :: proc($R:typeid)->proc(rawptr) { return proc(state:rawptr) { view:=cast(^Particle_View(R))state; view.prepared=false; view.token={} } }
/// A follower view stages camera and draw work; only the leading simulation consumes authored bursts.
particle_view_composition :: proc(view:^Particle_View($R))->Scene_Composition(R) { return {state=view,prepare=particle_view_prepare_callback(R),accepted=particle_view_accept_callback(R),aborted=particle_view_abort_callback(R)} }
