//! One editor shader family stages every consumer before any live GPU program changes.
package editor_app
import render "../render"
import shader "../../gfx/shader"
import "core:mem"
Editor_Shader_Allocator :: mem.Allocator

Editor_Shader_Reload :: struct($R:typeid) {
    gpu:^GPU_Owner(R),surface:^render.Surface_Shader,model:^render.Model_Shader,ui:^render.UI_Shader,
    picking:[2]render.Picking_Shader,service:render.Shader_Reload_Service,allocator:Editor_Shader_Allocator,
}
Editor_Shader_Candidate :: struct($R:typeid) {
    scene:^render.Scene_Shader_Reload_Candidate(R),particles:^render.Particle_Shader_Reload_Candidate(R),
    overlay:^render.Overlay_Shader_Reload_Candidate(R),ui:^render.UI_Shader_Reload_Candidate(R),
    picking:^render.Picking_Shader_Reload_Candidate(R),allocator:Editor_Shader_Allocator,
}
/// Registers the complete scene/effects/UI/picking source family with an owned original picking ABI.
editor_shader_reload_init :: proc(owner:^Editor_Shader_Reload($R),gpu:^GPU_Owner(R),surface:^render.Surface_Shader,model:^render.Model_Shader,ui:^render.UI_Shader,compiler:^shader.Compiler,root:string)->render.Shader_Reload_Error {
    owner^={gpu=gpu,surface=surface,model=model,ui=ui,allocator=gpu.allocator}
    success:=false;defer { if !success { editor_shader_reload_destroy(owner) } }
    for &reference,index in owner.picking { error:shader.Error; reference,error=render.picking_shader_compile(compiler,allocator=owner.allocator,masked=index==1);if error!=.None { return .Compile } }
    if error:=render.shader_reload_init(&owner.service,compiler,root,owner.allocator);error!=.None { return error }
    modules:=[18]render.Shader_Reload_Module{
        {path="surface.wgsl",selections={{"vs_main",.Vertex},{"fs_main",.Fragment}}},
        {path="postprocess.wgsl",selections={{"vs_display",.Vertex},{"fs_display",.Fragment}}},
        {path="environment.wgsl",selections={{"vs_sky",.Vertex},{"fs_sky",.Fragment}}},
        {path="grid.wgsl",selections={{"vs_grid",.Vertex},{"fs_grid",.Fragment}}},
        {path="lighting_cull.wgsl",selections={{"cs_lights",.Compute}}},
        {path="shadow_primitives.wgsl",selections={{"vs_shadow",.Vertex},{"vs_mark",.Vertex},{"vs_outline",.Vertex},{"fs_depth",.Fragment},{"fs_outline",.Fragment},{"fs_indicator",.Fragment}}},
        {path="shadow_models.wgsl",selections={{"vs_shadow",.Vertex},{"vs_mark",.Vertex},{"vs_outline",.Vertex},{"fs_depth",.Fragment},{"fs_outline",.Fragment},{"fs_indicator",.Fragment}}},
        {path="model.wgsl",selections={{"vs_model",.Vertex},{"fs_model",.Fragment}}},
        {path="particles_emit.wgsl",selections={{"cs_main",.Compute}}},
        {path="particle_simulate.wgsl",selections={{"cs_main",.Compute}}},
        {path="particle_draw_command.wgsl",selections={{"cs_main",.Compute}}},
        {path="particles_dispatch.wgsl",selections={{"cs_main",.Compute}}},
        {path="particles_render.wgsl",selections={{"vs_main",.Vertex},{"fs_main",.Fragment}}},
        {path="editor_overlay.wgsl",selections={{"vs_overlay",.Vertex},{"fs_overlay",.Fragment}}},
        {path="ui.wgsl",selections={{"vs_ui",.Vertex},{"fs_ui",.Fragment}}},
        {path="ui_transfer.wgsl",selections={{"vs_transfer",.Vertex},{"fs_encode",.Fragment},{"fs_decode",.Fragment}}},
        {path="picking.wgsl",selections={{"vs_pick",.Vertex},{"fs_pick",.Fragment}}},
        {path="picking_mask.wgsl",selections={{"vs_pick",.Vertex},{"fs_pick",.Fragment}}},
    }
    _,error:=render.shader_reload_register(&owner.service,{name="canonical-editor",modules=modules[:],publisher=editor_shader_publisher(R,owner)})
    if error!=.None { return error };success=true;return .None
}
@(private="package")
editor_shader_candidate_destroy :: proc(candidate:^Editor_Shader_Candidate($R)) {
    if candidate==nil { return }
    render.scene_shader_reload_destroy(candidate.scene);render.particle_shader_reload_destroy(candidate.particles)
    render.overlay_shader_reload_destroy(candidate.overlay);render.ui_shader_reload_destroy(candidate.ui)
    render.picking_shader_reload_destroy(candidate.picking);free(candidate,candidate.allocator)
}
@(private="package")
editor_shader_publisher :: proc($R:typeid,owner:^Editor_Shader_Reload(R))->render.Shader_Reload_Publisher {
    return {
        state=owner,
        prepare=proc(state:rawptr,artifacts:[]shader.Compiled)->(rawptr,render.Shader_Reload_Error) {
            host:=cast(^Editor_Shader_Reload(R))state
            if len(artifacts)!=18 { return nil,.Prepare }
            candidate:=new(Editor_Shader_Candidate(R),host.allocator);candidate.allocator=host.allocator
            success:=false;defer { if !success { editor_shader_candidate_destroy(candidate) } }
            consumers:[4]^render.Native_Consumer(R);for &view,index in host.gpu.views { consumers[index]=&view }
            error:render.Shader_Reload_Error
            candidate.scene,error=render.scene_shader_reload_prepare(consumers[:],host.surface,host.model,artifacts[:8],host.allocator);if error!=.None { return nil,error }
            candidate.particles,error=render.particle_shader_reload_prepare(&host.gpu.particles,artifacts[8:13]);if error!=.None { return nil,error }
            candidate.overlay,error=render.overlay_shader_reload_prepare(&host.gpu.overlay,&host.gpu.overlay_shader,artifacts[13:14]);if error!=.None { return nil,error }
            candidate.ui,error=render.ui_shader_reload_prepare(&host.gpu.ui,host.ui,artifacts[14:16]);if error!=.None { return nil,error }
            references:=[2]^shader.Compiled{&host.picking[0].compiled,&host.picking[1].compiled}
            candidate.picking,error=render.picking_shader_reload_prepare(&host.gpu.picking,references,artifacts[16:18],host.allocator);if error!=.None { return nil,error }
            success=true;return candidate,.None
        },
        publish=proc(state,candidate_pointer:rawptr)->rawptr {
            host:=cast(^Editor_Shader_Reload(R))state;candidate:=cast(^Editor_Shader_Candidate(R))candidate_pointer
            candidate.scene=render.scene_shader_reload_publish(candidate.scene)
            candidate.particles=render.particle_shader_reload_publish(candidate.particles,&host.gpu.graph)
            candidate.overlay=render.overlay_shader_reload_publish(candidate.overlay,&host.gpu.graph)
            candidate.ui=render.ui_shader_reload_publish(candidate.ui)
            candidate.picking=render.picking_shader_reload_publish(candidate.picking)
            return candidate
        },
        destroy=proc(_:rawptr,candidate_pointer:rawptr) { editor_shader_candidate_destroy(cast(^Editor_Shader_Candidate(R))candidate_pointer) },
    }
}
/// Stops the compile worker before releasing original interface snapshots or native consumer owners.
editor_shader_reload_destroy :: proc(owner:^Editor_Shader_Reload($R)) {
    if owner.service.compiler!=nil { render.shader_reload_destroy(&owner.service) }
    for &reference in owner.picking { render.picking_shader_destroy(&reference) };owner^={}
}
