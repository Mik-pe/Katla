//! Real per-camera preparations, retained UI and the native surface share one acquired frame and graph.
package editor_app
import gfx "../../gfx"
import render "../render"
import km "../../math"
import app ".."
import editor "../../editor"
import "core:math"
import "core:fmt"
import "core:mem"
import "core:time"

/// Asset revisions stage their own uploads before the host acquires its render token.
gpu_owner_refresh :: proc(gpu:^GPU_Owner($R))->render.Native_Error {
    if gpu.has_pending { return {gpu=.Busy} }
    for &view,index in gpu.views {
        if error:=render.native_consumer_refresh(&view);error!={} { fmt.eprintln("Editor view refresh",index,error);return error }
    }
    return {}
}

/// Prepares the current UI texture roles after exact slot acquisition; uploads do not advance that slot.
gpu_owner_frame :: proc(gpu:^GPU_Owner($R),shell:^Shell,fonts:^render.UI_Font_System,mesh:^render.UI_Mesh,token:gfx.Frame_Token,surface:gfx.Surface_Frame,clip_y_down:bool,readable_output:bool=false)->(gfx.Submission,render.Native_Error) {
    if gpu.has_pending { return {},{gpu=.Busy} }
    release_error:=gpu.operations.release_exports(gpu.renderer,&gpu.graph); if release_error!=.None { return {},{gpu=release_error} }
    truncate_error:=gfx.graph_truncate(&gpu.graph,0,0,0); if truncate_error!=.None { return {},{gpu=.Invalid_Graph} }
    gfx.compiled_graph_destroy(&gpu.plan)
    shell.gizmo_meshes=&gpu.overlay_meshes
    prepared:[4]render.Native_Prepared(R)
    success:=false
    defer { if !success { for &view in prepared { render.native_scene_prepared_abort(&view) } } }
    buffers:=make([dynamic]gfx.Buffer_Input,gpu.allocator); textures:=make([dynamic]gfx.Texture_Input,gpu.allocator); defer delete(buffers); defer delete(textures)
    overlay_frames:[4]render.Overlay_Frame; imports:render.Overlay_Graph
    defer { for &upload in overlay_frames { render.overlay_frame_destroy(&gpu.overlay,&upload) } }
    count:=viewport_count(shell.viewports.layout)
    for index in 0..<count {
        slot:=&shell.viewports.slots[index]; consumer:=&gpu.views[index]
        width:=pixel_extent(slot.bounds.width,mesh.pixel_scale); height:=pixel_extent(slot.bounds.height,mesh.pixel_scale)
        if consumer.width!=width || consumer.height!=height { error:=render.native_consumer_resize(consumer,width,height); if error!={} { fmt.eprintln("Editor view resize",index,error);return {},error } }
        error:render.Native_Error
        scene:=consumer.active
        if shell.preferences!=nil { scene.feature_settings.grid=shell.preferences.show_grid }
        selected:=selected_ids(shell); defer delete(selected,gpu.allocator); scene.selected=selected
        frame,frame_error:=editor_camera_frame(&slot.camera,width,height,clip_y_down); if frame_error!=.None { return {},{scene=frame_error} }
        shell.gizmo_frames[index]=frame
        namespace:=view_namespace(index,gpu.allocator); defer delete(namespace,gpu.allocator)
        prepared[index],error=render.native_scene_prepare(scene,token,frame,consumer.batch.objects,consumer.batch.draws,destination=&gpu.graph,namespace=namespace)
        scene.selected=nil
        if error!={} { fmt.eprintln("Editor view preparation",index,error);return {},error }
        render.overlay_mesh_destroy(&gpu.overlay_meshes[index])
        overlay_state:=overlay_state_prepare(shell,selected)
        overlay_scene_error:Editor_Overlay_Error
        gpu.overlay_meshes[index],overlay_scene_error=render.overlay_mesh_prepare(shell.state.owner,overlay_state,frame,width,height); if overlay_scene_error!=.None { return {},{scene=.Invalid_Geometry} }
        overlay_frames[index],error=render.overlay_native_append(&gpu.overlay,&prepared[index],&gpu.overlay_meshes[index],frame,&imports); if error!={} { fmt.eprintln("Editor overlay preparation",index,error);return {},error }
        append(&buffers,..prepared[index].buffers[:]); append(&textures,..prepared[index].textures[:])
        texture:=render.UI_Texture{handle=scene.slots[token.slot].output,desc=scene.graph.output_desc,resource=scene.graph.output}
        if texture_error:=render.ui_gpu_texture(&gpu.ui,slot.texture,texture); texture_error!=.None { return {},{gpu=.Invalid_Resource} }
    }
    picking_input:render.Picking_Graph_Input; defer render.picking_graph_input_destroy(&picking_input)
    pick_color,pick_id:gfx.Image_Id
    metadata:render.Picking_Metadata
    if gpu.capture_requested {
        active:=clamp(shell.viewports.active,0,count-1); scene:=gpu.views[active].active
        width:=scene.graph.output_desc.width; height:=scene.graph.output_desc.height
        if error:=gpu_pick_targets(gpu,width,height); error!=.None { return {},{gpu=error} }
        id_desc:=gfx.Texture_Desc{width=width,height=height,depth=1,layers=1,mip_levels=1,format=.R32_Uint,usage={.Color_Attachment,.Transfer_Source}}
        depth_desc:=gfx.Texture_Desc{width=width,height=height,depth=1,layers=1,mip_levels=1,format=.D32_Float,usage={.Depth_Attachment}}
        graph_error:gfx.Graph_Error
        pick_id,graph_error=gfx.graph_image(&gpu.graph,id_desc,{},true,true); if graph_error!=.None { return {},{gpu=.Invalid_Graph} }
        depth:gfx.Image_Id; depth,graph_error=gfx.graph_image(&gpu.graph,depth_desc,{},true,false); if graph_error!=.None { return {},{gpu=.Invalid_Graph} }
        append(&textures,gfx.Texture_Input{pick_id,gpu.pick_id},gfx.Texture_Input{depth,gpu.pick_depth})
        draws,error:=render.picking_scene_draws(scene,token,gpu.picking.pipelines,gpu.allocator); if error!={} { fmt.eprintln("Editor geometry picking",active,error);return {},error }; defer delete(draws,gpu.allocator)
        entries:=make([dynamic]render.Picking_Entry,gpu.allocator); defer delete(entries)
        for draw in draws { known:=false; for entry in entries { if entry.entity==draw.entity { known=true; break } }; if !known { append(&entries,render.Picking_Entry{draw.encoded,draw.entity}) } }
        billboards,billboard_error:=render.overlay_picking_draws(&gpu.overlay,&overlay_frames[active],&gpu.overlay_meshes[active],&scene.graph,&imports,entries[:],gpu.picking.pipelines.masked[1]); if billboard_error!={} { fmt.eprintln("Editor billboard picking",active,billboard_error);return {},billboard_error }; defer delete(billboards,gpu.allocator)
        all_draws:=make([dynamic]render.Picking_Draw,gpu.allocator); defer delete(all_draws); append(&all_draws,..draws); append(&all_draws,..billboards)
        camera:=render.Picking_Buffer{scene.graph.frame,scene.slots[token.slot].frame,scene.graph.frame_desc}
        picking_input,error=render.picking_graph_append(&gpu.graph,gpu.picking.pipelines.opaque[0],camera,all_draws[:],pick_id,depth,gpu.allocator); if error!={} { return {},error }
        for input in picking_input.buffers { append_unique_buffer(&buffers,input) }; for input in picking_input.textures { append_unique_texture(&textures,input) }
        pick_color=scene.graph.output; metadata={frame=gpu.serial+1,serial=gpu.serial+1,capture_time_ns=capture_time_now(),width=width,height=height}
        if gpu.pick_selection_serial==metadata.serial { metadata.pointer={i32(clamp(gpu.pick_selection_position[0],0,.999999)*f32(width)),i32(clamp(gpu.pick_selection_position[1],0,.999999)*f32(height))}; metadata.has_pointer=true }
        frozen,valid:=capture_context(shell,active,metadata,picking_input.entries,gpu.allocator); if !valid { return {},{gpu=.Invalid_Resource} }; delete(gpu.capture_context,gpu.allocator); gpu.capture_context=frozen
    }
    ui_frame,prepare_error:=render.ui_gpu_prepare(&gpu.ui,fonts,mesh); if prepare_error!=.None { fmt.eprintln("Editor UI preparation",prepare_error);return {},{gpu=prepare_error} }
    defer render.ui_gpu_frame_destroy(&gpu.ui,&ui_frame)
    surface_desc:=gfx.Texture_Desc{width=surface.width,height=surface.height,depth=1,layers=1,mip_levels=1,format=.BGRA8_Unorm,usage={.Color_Attachment}}
    if readable_output { surface_desc.usage|={.Transfer_Source} }
    output,graph_error:=gfx.graph_image(&gpu.graph,surface_desc,{},true,true); if graph_error!=.None { return {},{gpu=.Invalid_Graph} }
    append(&textures,gfx.Texture_Input{output,surface.texture})
    composition,composition_error:=render.ui_graph_append(&gpu.graph,&gpu.ui,&ui_frame,mesh,output,surface_desc,true,clip_y_down,gpu.allocator)
    if composition_error!={} { return {},composition_error }; defer render.ui_graph_input_destroy(&composition)
    for input in composition.buffers { append_unique_buffer(&buffers,input) }; for input in composition.textures { append_unique_texture(&textures,input) }
    compile_error:gfx.Graph_Error
    gpu.plan,compile_error=gfx.graph_compile(&gpu.graph); if compile_error!=.None { return {},{gpu=.Invalid_Graph} }
    submission,error,packet_error:=gpu.operations.submit(gpu.renderer,token,&gpu.graph,&gpu.plan,buffers[:],textures[:])
    if error!=.None || packet_error!=.None { return {},{gpu=error,packet=packet_error} }
    for &view in prepared[:count] { render.native_scene_prepared_accept(&view,submission) }
    gpu.output=output; gpu.pending=submission; gpu.has_pending=true; success=true; gpu.serial+=1
    if gpu.capture_requested {
        gpu.capture_requested=false
        capture,capture_error:=render.picking_queue(gpu.renderer,gpu.pick_ops,submission,pick_color,pick_id,metadata,picking_input.entries,gpu.allocator)
        gpu.capture=capture; gpu.capture_error=capture_error
    }
    return submission,{}
}
@(private="package")
append_unique_buffer :: proc(inputs:^[dynamic]gfx.Buffer_Input,value:gfx.Buffer_Input) { for input in inputs^ { if input.resource==value.resource { assert(input.handle==value.handle); return } }; append(inputs,value) }
@(private="package")
append_unique_texture :: proc(inputs:^[dynamic]gfx.Texture_Input,value:gfx.Texture_Input) { for input in inputs^ { if input.resource==value.resource { assert(input.handle==value.handle); return } }; append(inputs,value) }

@(private="package")
pixel_extent :: proc(logical,scale:f32)->u32 { return u32(max(1,math.round(logical*scale))) }
@(private="package")
view_namespace :: proc(index:int,allocator:mem.Allocator)->string { return fmt.aprintf("Editor view %d",index,allocator=allocator) }
@(private="package")
editor_camera_frame :: proc(camera:^Camera,width,height:u32,clip_y_down:bool,readable_output:bool=false)->(render.Frame_Data,render.Scene_Error) {
    return render.frame_data({position=camera_position(camera),target=camera.target,up=km.VEC3_Y,fov_degrees=camera.fov,near=camera.near,far=camera.far},width,height,clip_y_down)
}

@(private="package")
capture_time_now :: proc()->i64 { return time.to_unix_nanoseconds(time.now()) }

Editor_Overlay_Error :: editor.Scene_Error
@(private="package")
overlay_state_prepare :: proc(shell:^Shell,selected:Editor_Entities)->render.Editor_Overlay_State {
    state:=render.Editor_Overlay_State{selected=selected,basis=shell_gizmo_basis(shell),mode=shell.gizmo_mode,gizmo=shell.state.selection.has_primary && shell.state.owner.mode==.Editing,hover=shell.gizmo_hover,captured=shell.gizmo.handle if shell.gizmo.gesture.active else .None,billboards=true}
    if shell.state.selection.has_primary { world,error:=app.scene_world_matrix(shell.state.owner,shell.state.selection.primary); if error==.None { state.pivot={world[3][0],world[3][1],world[3][2]} } }
    if shell.preferences!=nil { state.physics=shell.preferences.show_physics_debug; state.reverb=shell.preferences.show_reverb_debug }; return state
}

/// Publishes completed pool readback separately from accepted authored emitter clocks.
gpu_owner_particle_statistics :: proc(gpu:^GPU_Owner($R),shell:^Shell) {
    clock:=""
    if shell.state.selection.has_primary {
        status,present:=render.particle_emitter_status(&gpu.particles,shell.state.selection.primary)
        if present { clock=fmt.aprintf(" · emitter %s · %.2f s remaining · accepted %d","finished" if status.finished else "active" if status.active else "inactive",status.remaining_duration,status.sequence,allocator=shell.allocator) }
    }
    defer delete(clock,shell.allocator)
    value:=fmt.aprintf("GPU pool: %d / %d alive · completed sequence %d%s",gpu.particles.observed_alive,gpu.particles.capacity,gpu.particles.observed_sequence,clock,allocator=shell.allocator)
    delete(shell.particle_statistics,shell.allocator); shell.particle_statistics=value
}
