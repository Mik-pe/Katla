//! The editor owns completed paired captures independently of newer authored scenes or surface frames.
package editor_app
import render "../render"
import gfx "../../gfx"
import ecs "../../ecs"

Editor_Picking_Snapshot :: render.Picking_Snapshot

/// Requests the next actual camera frame; concurrent requests share its immutable publication.
gpu_capture_request :: proc(gpu:^GPU_Owner($R))->bool {
    if gpu.capture.submission.owner!=nil || gpu.capture_error!=.None { return false }; gpu.capture_requested=true; return true
}
/// Publishes completed color and ID copies before the host retires or rebuilds any graph declarations.
gpu_capture_poll :: proc(gpu:^GPU_Owner($R))->(bool,gfx.Gpu_Error) {
    if gpu.capture_error!=.None { error:=gpu.capture_error; gpu.capture_error=.None; render.picking_capture_destroy(gpu.renderer,gpu.pick_ops,&gpu.capture); return false,error }
    if gpu.capture.submission.owner==nil { return false,.None }
    snapshot,complete,error:=render.picking_poll(gpu.renderer,gpu.pick_ops,&gpu.capture)
    if error!=.None { render.picking_capture_destroy(gpu.renderer,gpu.pick_ops,&gpu.capture); return false,error }
    if complete { render.picking_snapshot_destroy(&gpu.snapshot); gpu.snapshot=snapshot }
    return complete,.None
}
@(private="package")
gpu_pick_targets :: proc(gpu:^GPU_Owner($R),width,height:u32)->gfx.Gpu_Error {
    if gpu.pick_width==width && gpu.pick_height==height { return .None }
    id_desc:=gfx.Texture_Desc{width=width,height=height,depth=1,layers=1,mip_levels=1,format=.R32_Uint,usage={.Color_Attachment,.Transfer_Source}}
    depth_desc:=gfx.Texture_Desc{width=width,height=height,depth=1,layers=1,mip_levels=1,format=.D32_Float,usage={.Depth_Attachment}}
    id,error:=gpu.operations.create_texture(gpu.renderer,id_desc); if error!=.None { return error }
    depth:gfx.Texture_Handle; depth,error=gpu.operations.create_texture(gpu.renderer,depth_desc)
    if error!=.None { gpu.operations.destroy_texture(gpu.renderer,id); return error }
    for old in ([2]gfx.Texture_Handle{gpu.pick_id,gpu.pick_depth}) { if old.owner!=nil { error=gpu.operations.destroy_texture(gpu.renderer,old); if error!=.None { gpu.operations.destroy_texture(gpu.renderer,id); gpu.operations.destroy_texture(gpu.renderer,depth); return error } } }
    gpu.pick_id=id; gpu.pick_depth=depth; gpu.pick_width=width; gpu.pick_height=height; return .None
}
/// The panel queues an actual acquired camera capture through its stationary GPU host.
gpu_capture_callback :: proc($R:typeid)->proc(rawptr)->bool {
    return proc(state:rawptr)->bool { return gpu_capture_request(cast(^GPU_Owner(R))state) }
}

/// Correlates deferred view tickets with the exact next combined submission serial.
gpu_view_capture_callback :: proc($R:typeid)->proc(rawptr)->(u64,bool) {
    return proc(state:rawptr)->(u64,bool) {
        gpu:=cast(^GPU_Owner(R))state
        if !gpu_capture_request(gpu) { return 0,false }
        return gpu.serial+1,true
    }
}

/// Queues only a routed viewport click; the immutable map is resolved after that exact paired submission completes.
gpu_selection_request :: proc(gpu:^GPU_Owner($R),shell:^Shell) {
    if !shell.pick_requested || !gpu_capture_request(gpu) { return }
    shell.viewports.active=shell.pick_view; shell.viewports.has_active=true
    gpu.pick_selection_serial=gpu.serial+1; gpu.pick_selection_position=shell.pick_position; gpu.pick_selection_modifiers=shell.pick_modifiers
    shell.pick_requested=false
}
/// A newer world may have retired the captured entity; stale generations never select their replacement.
gpu_selection_complete :: proc(gpu:^GPU_Owner($R),shell:^Shell) {
    if gpu.pick_selection_serial==0 || gpu.snapshot.metadata.serial!=gpu.pick_selection_serial { return }
    gpu.pick_selection_serial=0
    sample:=render.picking_sample(&gpu.snapshot,i32(clamp(gpu.pick_selection_position[0],0,.999999)*f32(gpu.snapshot.metadata.width)),i32(clamp(gpu.pick_selection_position[1],0,.999999)*f32(gpu.snapshot.metadata.height)))
    if sample.mapped && picking_entity_exists(shell,sample.entity) {
        mode:=Selection_Mode.Toggle if .Control in gpu.pick_selection_modifiers || .Super in gpu.pick_selection_modifiers else Selection_Mode.Replace
        selection_set(shell.state,sample.entity,mode)
    } else if !sample.mapped { selection_clear(shell.state) }
}

@(private="package")
picking_entity_exists :: proc(shell:^Shell,entity:ecs.Entity_Id)->bool { return ecs.entity_exists(&shell.state.owner.world,entity) }
