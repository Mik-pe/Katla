//! Gizmo capture reuses rendered triangles and publishes atomic previews into shared scene history.
package editor_app
import app ".."
import render "../render"
import ecs "../../ecs"
import editor "../../editor"
import km "../../math"
import ui "../../ui"
import m "core:math"
import "core:mem"
import "core:encoding/json"

/// Zero disables snapping; rotation uses degrees and scale uses multiplicative increments.
Gizmo_Snap :: struct { translation,rotation,scale:f32 }
Gizmo_Row :: struct { entity,parent:ecs.Entity_Id,has_parent:bool,local:km.Transform,world,parent_world,parent_inverse:km.Mat4 }
Gizmo_Controller :: struct {
    gesture:app.Scene_Gesture,rows:[dynamic]Gizmo_Row,owner:^app.Authoring,
    mode:render.Overlay_Mode,handle:render.Overlay_Handle,pivot:km.Vec3,basis:km.Mat4,snap:Gizmo_Snap,
    start:km.Vec3,start_axis,last_angle,angle:f32,allocator:mem.Allocator,
}
@(private="file")
gizmo_finite :: proc(value:f32)->bool { return !m.is_nan(value) && !m.is_inf(value) }
@(private="file")
gizmo_ray_valid :: proc(origin,direction:km.Vec3)->bool { for vector in ([2]km.Vec3{origin,direction}) { for value in vector { if !gizmo_finite(value) { return false } } }; norm:=km.length_squared(direction); return gizmo_finite(norm) && norm>1e-10 }
/// Unprojects logical viewport points using the exact CPU projection shared by both native renderers.
gizmo_ray :: proc(view_projection:km.Mat4,bounds:ui.Rect,point:ui.Vec2)->(km.Vec3,km.Vec3,bool) {
    for value in ([4]f32{bounds.x,bounds.y,bounds.width,bounds.height}) { if !gizmo_finite(value) { return {},{},false } }
    if bounds.width<=0 || bounds.height<=0 { return {},{},false }
    for column in view_projection { for value in column { if !gizmo_finite(value) { return {},{},false } } }
    for value in point { if !gizmo_finite(value) { return {},{},false } }
    inverse,ok:=km.inverse(view_projection); if !ok { return {},{},false }
    x,y:=2*(point.x-bounds.x)/bounds.width-1,2*(point.y-bounds.y)/bounds.height-1
    near:=km.matrix_vector(inverse,km.Vec4{x,y,0,1}); far:=km.matrix_vector(inverse,km.Vec4{x,y,1,1})
    if abs(near[3])<1e-8 || abs(far[3])<1e-8 { return {},{},false }
    origin:=km.xyz(near/near[3]); direction:=km.xyz(far/far[3])-origin
    if !gizmo_ray_valid(origin,direction) { return {},{},false }; return origin,km.normalize(direction),true
}
/// Hit testing always consumes the rendered overlay mesh for the corresponding viewport frame.
gizmo_hit :: proc(mesh:^render.Overlay_Mesh,view_projection:km.Mat4,bounds:ui.Rect,point:ui.Vec2)->render.Overlay_Hit {
    if mesh==nil || mesh.overflow || len(mesh.triangles)>len(mesh.vertices)/3 { return {} }
    origin,direction,valid:=gizmo_ray(view_projection,bounds,point); if !valid { return {} }; return render.overlay_hit_test(mesh,origin,direction)
}
@(private="file")
gizmo_axes :: proc(handle:render.Overlay_Handle)->([3]bool,bool) {
    switch handle {
    case .Axis_X: return {true,false,false},true
    case .Axis_Y: return {false,true,false},true
    case .Axis_Z: return {false,false,true},true
    case .Plane_XY: return {true,true,false},true
    case .Plane_XZ: return {true,false,true},true
    case .Plane_YZ: return {false,true,true},true
    case .None: return {},false
    }; return {},false
}
@(private="file")
gizmo_parameter :: proc(c:^Gizmo_Controller,origin,direction:km.Vec3)->(km.Vec3,f32,bool) {
    if !gizmo_ray_valid(origin,direction) { return {},0,false }
    axes,_:=gizmo_axes(c.handle); index:=0; count:=0
    for enabled,i in axes { if enabled { index=i; count+=1 } }
    if count==1 && c.mode!=.Rotate {
        axis:=km.xyz(c.basis[index]); ray:=km.normalize(direction); dot:=km.dot(axis,ray); denominator:=1-dot*dot
        if denominator<1e-6 { return {},0,false }; offset:=origin-c.pivot
        time:=(dot*km.dot(offset,axis)-km.dot(offset,ray))/denominator
        value:=km.dot(offset,axis)+dot*time; return c.pivot+axis*value,value,gizmo_finite(value)
    }
    normal:=km.xyz(c.basis[index])
    if count==2 { for enabled,i in axes { if !enabled { normal=km.xyz(c.basis[i]) } } }
    denominator:=km.dot(normal,direction); if abs(denominator)<1e-6 { return {},0,false }
    time:=km.dot(c.pivot-origin,normal)/denominator; if time<0 || !gizmo_finite(time) { return {},0,false }; return origin+direction*time,0,true
}
@(private="file")
gizmo_matrix_exact :: proc(transform_matrix:km.Mat4)->(km.Transform,bool) {
    result,ok:=km.mat4_decompose_approx(transform_matrix); if !ok { return {},false }
    if km.dot(km.cross(km.xyz(transform_matrix[0]),km.xyz(transform_matrix[1])),km.xyz(transform_matrix[2]))<0 { result.scale[0]*=-1; rotation:=km.Mat3{km.xyz(transform_matrix[0])/result.scale[0],km.xyz(transform_matrix[1])/result.scale[1],km.xyz(transform_matrix[2])/result.scale[2]}; result.rotation=km.quat_from_mat3(rotation) }
    rebuilt:=km.transform_to_mat4(result)
    for column,i in transform_matrix { for value,j in column { if !gizmo_finite(value) || abs(rebuilt[i][j]-value)>1e-4*max(1,abs(value)) { return {},false } } }; return result,true
}
/// Captures selected roots once; selected descendants follow their ancestor without a second transform.
gizmo_begin :: proc(c:^Gizmo_Controller,owner:^app.Authoring,selected:[]ecs.Entity_Id,hit:render.Overlay_Hit,origin,direction,pivot:km.Vec3,basis:km.Mat4,mode:render.Overlay_Mode,snap:Gizmo_Snap={})->editor.Scene_Error {
    if c.gesture.active || owner==nil || !hit.hit || len(selected)==0 || len(selected)>256 || mode not_in (bit_set[render.Overlay_Mode]{.Translate,.Rotate,.Scale}) { return .Invalid_Operation }
    axes,valid:=gizmo_axes(hit.handle); if !valid { return .Invalid_Operation }
    selected_hit:=false; for entity in selected { if entity==hit.entity { selected_hit=true; break } }; if !selected_hit { return .Invalid_Operation }
    count:=0; for enabled in axes { if enabled { count+=1 } }; if mode==.Rotate && count!=1 { return .Invalid_Operation }
    for value in pivot { if !gizmo_finite(value) { return .Invalid_Field_Value } }; for value in ([3]f32{snap.translation,snap.rotation,snap.scale}) { if !gizmo_finite(value) || value<0 { return .Invalid_Field_Value } }
    for column in basis { for value in column { if !gizmo_finite(value) { return .Invalid_Field_Value } } }
    if basis[3]!=(km.Vec4{0,0,0,1}) { return .Invalid_Field_Value }
    for i in 0..<3 { axis:=km.xyz(basis[i]); if basis[i][3]!=0 || abs(km.length_squared(axis)-1)>1e-4 { return .Invalid_Field_Value }; for j in 0..<i { if abs(km.dot(axis,km.xyz(basis[j])))>1e-4 { return .Invalid_Field_Value } } }
    rows:=make([dynamic]Gizmo_Row,owner.world.allocator); transferred:=false; defer { if !transferred { delete(rows) } }
    ids:=make([dynamic]ecs.Entity_Id,owner.world.allocator); defer delete(ids)
    for entity,i in selected {
        for earlier in selected[:i] { if earlier==entity { return .Invalid_Operation } }
        if !ecs.entity_exists(&owner.world,entity) { return .Entity_Not_Found }
        ancestor:=entity; covered:=false
        for depth:=0;depth<256;depth+=1 {
            parent,present:=ecs.get_component(&owner.world,ancestor,app.Scene_Parent); if !present { break }; ancestor=parent.entity
            for chosen in selected { if chosen==ancestor { covered=true; break } }; if covered { break }; if depth==255 { return .Invalid_Operation }
        }; if covered { continue }
        transform,present:=ecs.get_component(&owner.world,entity,app.Scene_Transform); if !present { return .Component_Not_Found }
        world,error:=app.scene_world_matrix(owner,entity); if error!=.None { return error }
        row:=Gizmo_Row{entity=entity,local=transform.local,world=world,parent_world=km.identity(km.Mat4),parent_inverse=km.identity(km.Mat4)}
        if parent,has_parent:=ecs.get_component(&owner.world,entity,app.Scene_Parent); has_parent {
            row.parent=parent.entity; row.has_parent=true; row.parent_world,error=app.scene_world_matrix(owner,parent.entity); if error!=.None { return error }; inverse,invertible:=km.inverse(row.parent_world); if !invertible { return .Invalid_Operation }; row.parent_inverse=inverse
        }; append(&rows,row); append(&ids,entity)
    }
    candidate:=Gizmo_Controller{owner=owner,rows=rows,mode=mode,handle=hit.handle,pivot=pivot,basis=basis,snap=snap,allocator=owner.world.allocator}
    candidate.start,candidate.start_axis,valid=gizmo_parameter(&candidate,origin,direction); if !valid || mode==.Rotate && km.length_squared(candidate.start-pivot)<1e-10 { return .Invalid_Operation }
    if error:=app.scene_gesture_begin(owner,&candidate.gesture,ids[:]); error!=.None { return error }
    delete(c.rows); c^=candidate; transferred=true; return .None
}
/// Every move derives from the captured first transform, so rejected previews never accumulate drift.
gizmo_move :: proc(c:^Gizmo_Controller,origin,direction:km.Vec3)->editor.Scene_Error {
    if !c.gesture.active { return .Invalid_Operation }
    point,parameter,valid:=gizmo_parameter(c,origin,direction); if !valid { return .Invalid_Operation }
    axes,_:=gizmo_axes(c.handle); delta:=point-c.start; translation:km.Vec3; scale:=km.VEC3_ONE; angle:=c.angle; raw_angle:=c.last_angle
    rotation:=km.QUAT_IDENTITY
    if c.mode==.Rotate {
        axis:km.Vec3; for enabled,i in axes { if enabled { axis=km.xyz(c.basis[i]) } }
        first,current:=km.normalize(c.start-c.pivot),km.normalize(point-c.pivot)
        raw_angle=m.atan2(km.dot(axis,km.cross(first,current)),km.dot(first,current)); difference:=raw_angle-c.last_angle
        if difference>m.PI { difference-=2*m.PI }; if difference< -m.PI { difference+=2*m.PI }; angle+=difference
        value:=angle; if c.snap.rotation>0 { increment:=c.snap.rotation*m.PI/180; value=m.round(value/increment)*increment }; rotation=km.quat_axis_angle(axis,value)
    } else {
        count:=0; for enabled in axes { if enabled { count+=1 } }
        for enabled,i in axes { if !enabled { continue }; axis:=km.xyz(c.basis[i]); value:=km.dot(delta,axis)
            if c.mode==.Translate { if c.snap.translation>0 { pivot_axis:=km.dot(c.pivot,axis); value=m.round((pivot_axis+value)/c.snap.translation)*c.snap.translation-pivot_axis }; translation+=axis*value }
            else { baseline:=abs(c.start_axis) if count==1 else abs(km.dot(c.start-c.pivot,axis)); scale[i]=1+value/max(.1,baseline); if c.snap.scale>0 { scale[i]=1+m.round((scale[i]-1)/c.snap.scale)*c.snap.scale }; scale[i]=max(.001,scale[i]) }
        }
    }
    _=parameter
    operations:=make([]editor.Scene_Op,len(c.rows),c.allocator); defer { for operation in operations { delete(operation.value,c.allocator) }; delete(operations,c.allocator) }
    linear:=km.quat_to_mat4(rotation)
    if c.mode==.Scale { inverse,ok:=km.inverse(c.basis); if !ok { return .Invalid_Operation }; linear=km.matrix_mul(c.basis,km.matrix_mul(km.mat4_scale(scale),inverse)); linear[3]={0,0,0,1} }
    around:=km.matrix_mul(km.mat4_translation(c.pivot),km.matrix_mul(linear,km.mat4_translation(-c.pivot)))
    for row,i in c.rows {
        if row.has_parent { parent,error:=app.scene_world_matrix(c.owner,row.parent); if error!=.None { return error }; if parent!=row.parent_world { return .Invalid_Operation } }
        local:=row.local
        if c.mode==.Translate { local.position=km.transform_point(row.parent_inverse,km.xyz(row.world[3])+translation) }
        else { candidate:=km.matrix_mul(row.parent_inverse,km.matrix_mul(around,row.world)); exact:bool; local,exact=gizmo_matrix_exact(candidate); if !exact { return .Invalid_Operation } }
        value,error:=json.marshal(local,allocator=c.allocator); if error!=nil { return .Invalid_Field_Value }
        operations[i]={kind=.Set_Field,entity=row.entity,component="SceneTransform",field="local",value=value}
    }
    if error:=app.scene_gesture_preview_values(c.owner,&c.gesture,operations); error!=.None { return error }; c.last_angle=raw_angle; c.angle=angle; return .None
}
/// A rejected finish retains capture and its accepted preview for retry or cancellation.
gizmo_finish :: proc(c:^Gizmo_Controller)->editor.Scene_Error { if !c.gesture.active { return .None }; error:=app.scene_gesture_finish(c.owner,&c.gesture); if error==.None { delete(c.rows); c.rows=nil; c.handle=.None }; return error }
/// Escape or native blur restores the first accepted authored revision atomically.
gizmo_cancel :: proc(c:^Gizmo_Controller)->editor.Scene_Error { if !c.gesture.active { return .None }; error:=app.scene_gesture_cancel(c.owner,&c.gesture); if error==.None { delete(c.rows); c.rows=nil; c.handle=.None }; return error }
/// Teardown follows host finish/cancel; no native mesh or camera pointers are retained.
gizmo_destroy :: proc(c:^Gizmo_Controller) { app.scene_gesture_destroy(&c.gesture); delete(c.rows); c^={} }
