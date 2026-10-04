//! The editor stages all real camera consumers before publishing an authored scene revision.
package editor_app
import app ".."
import ecs "../../ecs"
import editor "../../editor"
import gfx "../../gfx"
import render "../render"
import shader "../../gfx/shader"
import ui "../../ui"
import "core:mem"

Editor_Overlay_Shader :: render.Overlay_Shader
Editor_Pointer :: ui.Vec2
Editor_Modifiers :: ui.Modifiers
Editor_Authoring :: ^app.Authoring
Editor_Entities :: []ecs.Entity_Id
GPU_Owner :: struct($R:typeid) {
    renderer:^R,operations:render.GPU_Ops(R),views:[4]render.Native_Consumer(R),
    output:gfx.Image_Id,graph:gfx.Graph,plan:gfx.Compiled_Graph,pending:gfx.Submission,has_pending:bool,
    overlay_shader:Editor_Overlay_Shader,overlay:render.Overlay_Native(R),overlay_meshes:[4]render.Overlay_Mesh,
    particles:render.Particle_Consumer(R),particle_views:[3]render.Particle_View(R),
    ui:render.UI_GPU(R),picking:render.Picking_Native(R),pick_ops:render.Picking_Ops(R),
    pick_id,pick_depth:gfx.Texture_Handle,pick_width,pick_height:u32,
    pick_selection_serial:u64,pick_selection_position:Editor_Pointer,pick_selection_modifiers:Editor_Modifiers,
    capture:render.Picking_Capture,snapshot:render.Picking_Snapshot,capture_requested:bool,capture_error:gfx.Gpu_Error,serial:u64,capture_context:[]byte,
    allocator:mem.Allocator,owner:Editor_Authoring,
}
GPU_Preparation :: struct { tokens:[4]rawptr,allocator:mem.Allocator }
/// Initializes every stationary camera cache before the aggregate participant becomes visible.
gpu_owner_init :: proc(gpu:^GPU_Owner($R),owner:Editor_Authoring,renderer:^R,operations:render.GPU_Ops(R),pipelines:render.Scene_Pipelines,models:^render.Model_Config(R),ui_ops:render.UI_GPU_Ops(R),ui_shader:^render.UI_Shader,fonts:^render.UI_Font_System,particle_ops:render.Particle_GPU_Ops(R),pick_ops:render.Picking_Ops(R),compiler:^shader.Compiler,slots:int)->render.Native_Error {
    gpu^={owner=owner,renderer=renderer,operations=operations,pick_ops=pick_ops,allocator=owner.world.allocator}; gfx.graph_init(&gpu.graph,owner.world.allocator)
    success:=false; defer { if !success { gpu_owner_destroy(gpu) } }
    for &view in gpu.views { error:=render.native_consumer_init(&view,owner,renderer,operations,pipelines,slots,256,256,models=models,install_participant=false); if error!={} { return error } }
    if error:=render.ui_gpu_init(&gpu.ui,renderer,ui_ops,ui_shader,owner.world.allocator); error!=.None { return {gpu=error} }
    compiled_overlay,overlay_error:=render.overlay_shader_compile(compiler); if overlay_error!=.None { return {gpu=.Invalid_Shader} }; gpu.overlay_shader=compiled_overlay
    if error:=render.overlay_native_init(&gpu.overlay,renderer,ui_ops,&gpu.overlay_shader,fonts); error!={} { return error }
    particle_error,shader_error:=render.particle_consumer_init(&gpu.particles,owner,renderer,particle_ops,compiler,.RGBA16_Float,slot_count=slots,allocator=owner.world.allocator)
    if particle_error!={} || shader_error!=.None { return {gpu=particle_error.gpu,packet=particle_error.packet,scene=.Invalid_Geometry} }
    if error:=render.native_consumer_compose(&gpu.views[0],render.particle_composition(&gpu.particles)); error!={} { return error }
    for &view,index in gpu.particle_views {
        if error:=render.particle_view_init(&view,&gpu.particles); error!=.None { return {gpu=error} }
        if error:=render.native_consumer_compose(&gpu.views[index+1],render.particle_view_composition(&view)); error!={} { return error }
    }
    if error:=render.picking_native_init(&gpu.picking,renderer,operations,compiler); error!={} { return error }
    ecs.insert_resource(&owner.world,app.Scene_Participant{gpu,gpu_prepare_callback(R),gpu_finish_callback(R)})
    ecs.insert_resource(&owner.world,app.Material_Native_Inspection{gpu,gpu_material_inspection_callback(R)})
    success=true; return {}
}
@(private="package")
gpu_prepare_callback :: proc($R:typeid)->proc(rawptr,^app.Authoring,[]ecs.Entity_Id,app.Scene_Preparation_Mode)->(rawptr,editor.Scene_Error) {
    return proc(state:rawptr,owner:^app.Authoring,ids:[]ecs.Entity_Id,mode:app.Scene_Preparation_Mode)->(rawptr,editor.Scene_Error) {
        gpu:=cast(^GPU_Owner(R))state; if gpu.owner!=owner { return nil,.Invalid_Operation }
        preparation:=new(GPU_Preparation,gpu.allocator); preparation.allocator=gpu.allocator
        for &view,index in gpu.views {
            token,error:=render.native_consumer_prepare(&view,owner,ids,mode)
            if error!=.None { for previous in 0..<index { render.native_consumer_finish(&gpu.views[previous],preparation.tokens[previous],false) }; free(preparation,gpu.allocator); return nil,error }
            preparation.tokens[index]=token
        }; return preparation,.None
    }
}
@(private="package")
gpu_finish_callback :: proc($R:typeid)->proc(rawptr,rawptr,bool) {
    return proc(state,token:rawptr,commit:bool) {
        gpu:=cast(^GPU_Owner(R))state; preparation:=cast(^GPU_Preparation)token
        for &view,index in gpu.views { render.native_consumer_finish(&view,preparation.tokens[index],commit) }; free(preparation,preparation.allocator)
    }
}
/// Retires the exact combined submission before truncating shared graph declarations or resizing views.
gpu_owner_wait :: proc(gpu:^GPU_Owner($R))->gfx.Gpu_Error {
    if !gpu.has_pending { return .None }; error:=gpu.operations.wait(gpu.renderer,gpu.pending); if error==.None { gpu.has_pending=false; gpu.pending={} }; return error
}
/// Shared graph exports belong to this host and outlive every individual view's accepted preparation.
gpu_owner_destroy :: proc(gpu:^GPU_Owner($R))->gfx.Gpu_Error {
    first:=gpu_owner_wait(gpu)
    if gpu.owner!=nil { inspection,present:=ecs.get_resource(&gpu.owner.world,app.Material_Native_Inspection);if present && inspection.state==gpu { ecs.remove_resource(&gpu.owner.world,app.Material_Native_Inspection) } }
    if gpu.owner!=nil { participant,present:=ecs.get_resource(&gpu.owner.world,app.Scene_Participant); if present && participant.state==gpu { ecs.remove_resource(&gpu.owner.world,app.Scene_Participant) } }
    if gpu.renderer!=nil { error:=gpu.operations.release_exports(gpu.renderer,&gpu.graph); if first==.None { first=error } }
    render.picking_capture_destroy(gpu.renderer,gpu.pick_ops,&gpu.capture); render.picking_snapshot_destroy(&gpu.snapshot); delete(gpu.capture_context,gpu.allocator)
    if gpu.picking.renderer!=nil { error:=render.picking_native_destroy(&gpu.picking); if first==.None { first=error } }
    for handle in ([2]gfx.Texture_Handle{gpu.pick_id,gpu.pick_depth}) { if handle.owner!=nil { error:=gpu.operations.destroy_texture(gpu.renderer,handle); if first==.None { first=error } } }
    for &view in gpu.views { if view.authoring!=nil { error:=render.native_consumer_destroy(&view); if first==.None { first=error } } }
    for &view in gpu.particle_views { if view.consumer!=nil { error:=render.particle_view_destroy(&view); if first==.None { first=error } } }
    if gpu.particles.renderer!=nil { error:=render.particle_consumer_destroy(&gpu.particles); if first==.None { first=error } }
    for &mesh in gpu.overlay_meshes { render.overlay_mesh_destroy(&mesh) }
    if gpu.overlay.renderer!=nil { error:=render.overlay_native_destroy(&gpu.overlay); if first==.None { first=error } }
    if gpu.ui.renderer!=nil { error:=render.ui_gpu_destroy(&gpu.ui); if first==.None { first=error } }
    render.overlay_shader_destroy(&gpu.overlay_shader)
    gfx.compiled_graph_destroy(&gpu.plan); gfx.graph_destroy(&gpu.graph); gpu^={}; return first
}

/// Reads provenance from accepted native material receipts without borrowing CPU source guesses.
gpu_material_inspection_callback :: proc($R:typeid)->proc(rawptr,ecs.Entity_Id,int)->(bool,bool) {
    return proc(state:rawptr,entity:ecs.Entity_Id,role:int)->(bool,bool) {
        gpu:=cast(^GPU_Owner(R))state
        if gpu==nil || gpu.views[0].active==nil { return false,false }
        return render.model_native_image_fallback(gpu.views[0].active.models,entity,role)
    }
}
