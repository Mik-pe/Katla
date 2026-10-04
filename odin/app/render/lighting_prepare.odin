//! Acquired-slot preparation freezes scene-light data and actual shadow draw ranges.
package render

import app ".."
import ecs "../../ecs"
import gfx "../../gfx"
import km "../../math"
import "core:mem"

@(private="package")
lighting_fallback_snapshot :: proc(frame:Frame_Data)->Lighting_Snapshot {
    return {has_sun=frame.light_color[3]>0,sun=app.Scene_Directional_Light{direction=km.xyz(frame.light_direction),color={frame.light_color[0],frame.light_color[1],frame.light_color[2]},intensity=frame.light_color[3]}}
}
@(private="package")
lighting_upload_bytes :: proc(frame:[]Lighting_Frame,points:[]Point_Light_GPU,shadow:[]Shadow_Frame)->[3][]byte { return {mem.slice_to_bytes(frame),mem.slice_to_bytes(points),mem.slice_to_bytes(shadow)} }
/// Selection is validated on the scene owner and copied before any pending frame changes.
native_scene_select :: proc(scene:^Native_Scene($R),ids:[]ecs.Entity_Id)->Native_Error {
    if scene.authoring==nil && len(ids)>0 { return {scene=.Invalid_Geometry} }
    for id,i in ids {
        if !ecs.entity_exists(&scene.authoring.world,id) { return {scene=.Invalid_Geometry} }
        for previous in ids[:i] { if previous==id { return {scene=.Invalid_Geometry} } }
    }
    replacement:=make([]ecs.Entity_Id,len(ids),scene.allocator); copy(replacement,ids)
    delete(scene.selected,scene.allocator); scene.selected=replacement; return {}
}
native_features_prepare :: proc(scene:^Native_Scene($R),token:gfx.Frame_Token,frame:Frame_Data,draws:[]gfx.Draw_Op)->(Frame_Data,Scene_Inputs,Native_Error) {
    if token.slot<0 || token.slot>=len(scene.features.slots) { return {},{},{gpu=.Invalid_Resource} }
    if scene.feature_settings.shadow_size!=scene.graph.features.atlas_desc.width || !postprocess_valid(scene.feature_settings.postprocess) { return {},{},{scene=.Invalid_Material} }
    snapshot:Lighting_Snapshot
    effective:=frame
    if scene.authoring!=nil {
        error:Scene_Error
        snapshot,error=lighting_collect(scene.authoring); if error!=.None { return {},{},{scene=error} }
        if !snapshot.has_sun { fallback:=lighting_fallback_snapshot(frame); snapshot.sun=fallback.sun; snapshot.has_sun=fallback.has_sun }
        effective=lighting_apply(frame,snapshot)
    } else {
        snapshot=lighting_fallback_snapshot(frame)
    }
    data,error:=lighting_frame(effective,scene.graph.color_desc.width,scene.graph.color_desc.height,snapshot,scene.feature_settings.sky,scene.feature_settings.shadows); if error!=.None { return {},{},{scene=error} }
    direction:=km.Vec3{0,-1,0}; if snapshot.has_sun { direction=snapshot.sun.direction }
    shadow:=Shadow_Frame{direction=km.vec4(direction),bias={1.5/f32(scene.feature_settings.shadow_size),3,0,f32(scene.feature_settings.shadow_size)}}
    if scene.feature_settings.shadows && snapshot.has_sun {
        shadow_error:Scene_Error
        shadow,shadow_error=shadow_cascades(effective,data,direction,scene.feature_settings.shadow_size,true,scene.graph.depth_sense); if shadow_error!=.None { return {},{},{scene=shadow_error} }
    }
    slot:=scene.features.slots[token.slot]
    frame_values:=[1]Lighting_Frame{data}; shadow_values:=[1]Shadow_Frame{shadow}
    bytes:=lighting_upload_bytes(frame_values[:],snapshot.points[:],shadow_values[:])
    uploads:=[3]struct { handle:gfx.Buffer_Handle,bytes:[]byte }{{slot.frame,bytes[0]},{slot.points,bytes[1]},{slot.shadow,bytes[2]}}
    for upload in uploads { gpu_error:=scene.operations.write_buffer(scene.renderer,token,upload.handle,0,upload.bytes); if gpu_error!=.None { return {},{},{gpu=gpu_error} } }
    packet_error:=feature_shadow_packet(&scene.graph,0,scene.graph.objects,scene.graph.geometry,scene.graph.object_desc,scene.graph.geometry_desc,draws,effective.ambient[3],scene.feature_settings.shadows && snapshot.has_sun); if packet_error!={} { return {},{},packet_error }
    model_draws:=make([dynamic]gfx.Draw_Op,scene.allocator); defer delete(model_draws)
    if scene.models!=nil {
        for entry in scene.models.batch.entries { if entry.material.alpha_mode!=.Blend && !entry.material.unlit { append(&model_draws,gfx.Draw{entry.vertex_count,1,entry.first_vertex,entry.object_index}) } }
        packet_error=feature_shadow_packet(&scene.graph,1,scene.models.objects,scene.models.geometry,scene.models.object_desc,scene.models.geometry_desc,model_draws[:],effective.ambient[3],scene.feature_settings.shadows && snapshot.has_sun); if packet_error!={} { return {},{},packet_error }
    }
    scene.graph.wallhack=scene.feature_settings.wallhack
    scene.graph.postprocess=scene.feature_settings.postprocess
    f:=&scene.graph.features
    buffers:=make([]gfx.Buffer_Input,5,scene.allocator)
    handles:=[5]gfx.Buffer_Handle{slot.frame,slot.points,slot.indices,slot.counts,slot.shadow}
    for handle,i in handles { buffers[i]={f.buffers[i],handle} }
    textures:=make([]gfx.Texture_Input,2,scene.allocator); textures[0]={f.atlas,slot.atlas}; textures[1]={f.indicator,slot.indicator}
    return effective,{buffers,textures},{}
}
