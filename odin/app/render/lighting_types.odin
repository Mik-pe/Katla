//! Flat scene-light snapshots retain authored linear colors and exact hierarchy placement.
package render

import app ".."
import ecs "../../ecs"
import km "../../math"
import "core:math"

MAX_POINT_LIGHTS :: 256
LIGHT_TILE_SIZE :: 16
MAX_TILE_LIGHTS :: 128
Point_Light_GPU :: struct { position:km.Vec3, range:f32, color:[3]f32, intensity:f32 }
Lighting_Frame :: struct { view,inverse_view_projection:km.Mat4, viewport,settings:km.Vec4 }
Lighting_Snapshot :: struct { points:[MAX_POINT_LIGHTS]Point_Light_GPU, count:u32, sun:app.Scene_Directional_Light, has_sun:bool, ambient:km.Vec3 }
/// Collection rejects invalid lights or capacity overflow before acquired-slot uploads.
lighting_collect :: proc(owner:^app.Authoring)->(Lighting_Snapshot,Scene_Error) {
    if owner==nil { return {ambient={0.01,0.01,0.01}},.None }
    ids:=ecs.entity_ids(&owner.world); defer delete(ids)
    return lighting_collect_entities(owner,ids[:])
}
/// Validates the prospective scene membership before native publication.
lighting_collect_entities :: proc(owner:^app.Authoring,ids:[]ecs.Entity_Id)->(Lighting_Snapshot,Scene_Error) {
    result:=Lighting_Snapshot{ambient={0.01,0.01,0.01}}
    if owner==nil { return result,.None }
    if ambient,present:=ecs.get_resource(&owner.world,app.Scene_Environment); present {
        if !app.light_color_valid(ambient.color,ambient.intensity) { return {},.Invalid_Material }
        result.ambient=ambient.color*ambient.intensity
    }
    // Lowest stable entity identity chooses the single shadow-casting directional light.
    sun_id:=max(u64)
    for id in ids {
        if _,hidden:=ecs.get_component(&owner.world,id,app.Editor_Hidden); hidden { continue }
        if light,present:=ecs.get_component(&owner.world,id,app.Scene_Directional_Light); present {
            if !app.directional_light_valid(light) { return {},.Invalid_Material }
            if u64(id)<sun_id { sun_id=u64(id); result.sun=light; result.sun.direction=km.normalize(light.direction); result.has_sun=true }
        }
        if light,present:=ecs.get_component(&owner.world,id,app.Scene_Point_Light); present {
            if !app.point_light_valid(light) || result.count>=MAX_POINT_LIGHTS { return {},.Invalid_Material }
            world,error:=app.scene_world_matrix(owner,id); if error!=.None { return {},.Invalid_Transform }
            result.points[result.count]={km.xyz(world[3]),light.range,light.color,light.intensity}; result.count+=1
        }
    }
    return result,.None
}
/// Recovers the rigid camera view from its perspective rows; column-major storage is unchanged.
lighting_frame :: proc(frame:Frame_Data,width,height:u32,snapshot:Lighting_Snapshot,sky,shadows:bool)->(Lighting_Frame,Scene_Error) {
    if width==0 || height==0 { return {},.Invalid_Camera }
    right:=km.Vec3{frame.view_projection[0][0],frame.view_projection[1][0],frame.view_projection[2][0]}
    up:= -km.Vec3{frame.view_projection[0][1],frame.view_projection[1][1],frame.view_projection[2][1]}
    if km.length_squared(right)<1e-10 || km.length_squared(up)<1e-10 { return {},.Invalid_Camera }
    right=km.normalize(right); up=km.normalize(up); back:=km.cross(right,up)
    position:=km.xyz(frame.camera_position)
    view:=km.Mat4{{right[0],up[0],back[0],0},{right[1],up[1],back[1],0},{right[2],up[2],back[2],0},{-km.dot(right,position),-km.dot(up,position),-km.dot(back,position),1}}
    inverse,ok:=km.inverse(frame.view_projection); if !ok { return {},.Invalid_Camera }
    for column in inverse { for value in column { if math.is_nan(value)||math.is_inf(value) { return {},.Invalid_Camera } } }
    return {view,inverse,{f32(width),f32(height),f32((width+15)/16),f32((height+15)/16)},{f32(snapshot.count),f32(int(sky)),f32(int(shadows && snapshot.has_sun)),frame.ambient[3]}},.None
}
/// Updates ordinary surface uniforms from authored scene illumination.
lighting_apply :: proc(frame:Frame_Data,snapshot:Lighting_Snapshot)->Frame_Data {
    result:=frame; for i in 0..<3 { result.ambient[i]=snapshot.ambient[i] }
    result.light_direction={0,-1,0,0}; result.light_color={0,0,0,0}
    if snapshot.has_sun { result.light_direction=km.vec4(snapshot.sun.direction); result.light_color={snapshot.sun.color[0],snapshot.sun.color[1],snapshot.sun.color[2],snapshot.sun.intensity} }
    return result
}
