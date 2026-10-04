//! Camera and scene metadata are frozen before the exact paired native capture is admitted.
package editor_app
import app ".."
import ecs "../../ecs"
import editor "../../editor"
import render "../render"
import km "../../math"
import ron "../../encoding/ron"
import "core:encoding/json"
import "core:fmt"
import "core:mem"
import "core:slice"

Capture_Orbit :: struct { target:[3]f32,yaw,pitch,distance:f32 }
Capture_Camera :: struct { position,direction:[3]f32,view_matrix,projection_matrix:km.Mat4,orbit:Capture_Orbit }
Capture_Candidate :: struct {
    entity_id:string,name,parent_id:Maybe(string),encoded:u32,position,bounds_center,bounds_extent:[3]f32,has_bounds:bool,
    world_bounds:km.AABB,distance:f32,screen_rect:Maybe([4]f32),bounds_fully_in_frustum:bool,visibility:string,
}
Capture_Context :: struct {
    frame_id,capture_serial:string,capture_time_ns:i64,width,height:u32,
    camera_position,camera_target:[3]f32,selected_entities:[]string,
    camera:Capture_Camera,frustum_candidates:[]Capture_Candidate,
    visibility_basis,coordinates:string,renderables_without_bounds:int,
    undo_available,redo_available,game_camera_modified:bool,
}
/// Serializes borrowed world data immediately; the returned bytes are independent of subsequent edits.
capture_context :: proc(shell:^Shell,active:int,metadata:render.Picking_Metadata,entries:[]render.Picking_Entry,allocator:mem.Allocator)->([]byte,bool) {
    if shell==nil || shell.state==nil || active<0 || active>=viewport_count(shell.viewports.layout) || metadata.width==0 || metadata.height==0 { return nil,false }
    camera:=&shell.viewports.slots[active].camera
    eye:=camera_position(camera); view,invertible:=km.inverse(km.mat4_lookat(eye,camera.target,km.VEC3_Y)); if !invertible { return nil,false }
    projection:=km.mat4_perspective(camera.fov,f32(metadata.width)/f32(metadata.height),camera.near,camera.far); vp:=km.matrix_mul(projection,view)
    value:=Capture_Context{capture_time_ns=metadata.capture_time_ns,width=metadata.width,height=metadata.height,camera_position=eye,camera_target=camera.target,visibility_basis="Encoded object IDs come from the same accepted GPU submission; geometric candidates do not establish occlusion visibility or semantic room membership",coordinates="Normalized native image coordinates: top-left (0,0), bottom-right (1,1); conservatively near-clipped rectangles",undo_available=editor.agent_can_undo(&shell.state.owner.agent.session),redo_available=editor.agent_can_redo(&shell.state.owner.agent.session),camera={eye,km.normalize(camera.target-eye),view,projection,{camera.target,camera.yaw,camera.pitch,camera.distance}}}
    value.frame_id=fmt.aprintf("%d",metadata.frame,allocator=allocator); defer delete(value.frame_id,allocator)
    value.capture_serial=fmt.aprintf("%d",metadata.serial,allocator=allocator); defer delete(value.capture_serial,allocator)
    selected:=selected_ids(shell); defer delete(selected,allocator)
    value.selected_entities=make([]string,len(selected),allocator); defer { for item in value.selected_entities { delete(item,allocator) }; delete(value.selected_entities,allocator) }
    for id,index in selected { value.selected_entities[index]=fmt.aprintf("%d",u64(id),allocator=allocator) }
    rows:=make([]Capture_Candidate,len(entries),allocator); defer { for row in rows { delete(row.entity_id,allocator); if parent,present:=row.parent_id.(string); present { delete(parent,allocator) } }; delete(rows,allocator) }
    count:=0
    for entry in entries {
        bounds,available,bounds_error:=app.scene_drawable_bounds(shell.state.owner,entry.entity)
        if bounds_error!=.None { return nil,false }; if !available { value.renderables_without_bounds+=1; continue }
        if !capture_bounds_visible(vp,bounds) { continue }
        row:=&rows[count]; row.entity_id=fmt.aprintf("%d",u64(entry.entity),allocator=allocator); row.encoded=entry.encoded
        if name,present:=ecs.get_component(&shell.state.owner.world,entry.entity,app.Scene_Name); present { row.name=name.name }
        if parent,present:=ecs.get_component(&shell.state.owner.world,entry.entity,app.Scene_Parent); present { row.parent_id=fmt.aprintf("%d",u64(parent.entity),allocator=allocator) }
        world,error:=app.scene_world_matrix(shell.state.owner,entry.entity); if error!=.None { return nil,false }; row.position={world[3][0],world[3][1],world[3][2]}
        row.bounds_center=bounds.center; row.bounds_extent=bounds.extent; row.has_bounds=true; row.world_bounds=bounds; row.distance=km.length(bounds.center-eye)
        row.screen_rect=capture_bounds_project(vp,bounds,camera.near); row.bounds_fully_in_frustum=capture_bounds_fully_visible(vp,bounds); row.visibility="frustum_candidate_occlusion_unknown"; count+=1
    }
    value.frustum_candidates=rows[:count]; slice.sort_by(value.frustum_candidates,proc(a,b:Capture_Candidate)->bool { left,_:=ron.decimal_u64(a.entity_id); right,_:=ron.decimal_u64(b.entity_id); return left<right })
    encoded,error:=json.marshal(value,allocator=allocator); return encoded,error==nil
}
@(private="package")
capture_bounds_corners :: proc(matrix_value:km.Mat4,bounds:km.AABB)->[8]km.Vec4 {
    result:[8]km.Vec4
    for corner in 0..<8 { point:=bounds.center; for axis in 0..<3 { point[axis]+=bounds.extent[axis] if (corner&(1<<u32(axis)))!=0 else -bounds.extent[axis] }; result[corner]=km.matrix_vector(matrix_value,km.Vec4{point[0],point[1],point[2],1}) }; return result
}
@(private="package")
capture_outside :: proc(clip:km.Vec4)->[6]bool { return {clip[0]<-clip[3],clip[0]>clip[3],clip[1]<-clip[3],clip[1]>clip[3],clip[2]<0,clip[2]>clip[3]} }
@(private="package")
capture_bounds_visible :: proc(matrix_value:km.Mat4,bounds:km.AABB)->bool {
    rejected:[6]bool={true,true,true,true,true,true}
    for clip in capture_bounds_corners(matrix_value,bounds) { outside:=capture_outside(clip); for &value,axis in rejected { value=value && outside[axis] } }
    for value in rejected { if value { return false } }; return true
}
@(private="package")
capture_bounds_fully_visible :: proc(matrix_value:km.Mat4,bounds:km.AABB)->bool { for clip in capture_bounds_corners(matrix_value,bounds) { for outside in capture_outside(clip) { if outside { return false } } }; return true }
@(private="package")
capture_bounds_project :: proc(matrix_value:km.Mat4,bounds:km.AABB,near:f32)->Maybe([4]f32) {
    corners:=capture_bounds_corners(matrix_value,bounds); points:[20]km.Vec4; count:=0
    for point in corners { if point[3]>=near { points[count]=point; count+=1 } }
    for a,index in corners { for bit in ([3]int{1,2,4}) { other:=index~bit; if other<=index { continue }; b:=corners[other]; if (a[3]>=near)!=(b[3]>=near) { points[count]=a+(b-a)*((near-a[3])/(b[3]-a[3])); count+=1 } } }
    if count==0 { return nil }
    rect:=[4]f32{1,1,0,0}
    for point in points[:count] { x:=clamp(point[0]/point[3]*0.5+0.5,0,1); y:=clamp(point[1]/point[3]*0.5+0.5,0,1); rect[0]=min(rect[0],x); rect[1]=min(rect[1],y); rect[2]=max(rect[2],x); rect[3]=max(rect[3],y) }; return rect
}
