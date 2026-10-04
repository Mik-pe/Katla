#+test
package editor_app
import app ".."
import render "../render"
import ecs "../../ecs"
import editor "../../editor"
import km "../../math"
import ui "../../ui"
import m "core:math"
import "core:testing"

@(private="file")
gizmo_test_frame :: proc()->render.Frame_Data { value,_:=render.frame_data({position={4,3,8},target={},up={0,1,0},fov_degrees=60,near=.01,far=100},640,480,true); return value }
@(private="file")
gizmo_test_project :: proc(frame:render.Frame_Data,world:km.Vec3)->ui.Vec2 { clip:=km.matrix_vector(frame.view_projection,km.vec4(world,1)); return {20+(clip[0]/clip[3]+1)*320,30+(clip[1]/clip[3]+1)*240} }
@(private="file")
gizmo_test_pick :: proc(mesh:^render.Overlay_Mesh,frame:render.Frame_Data,handle:render.Overlay_Handle)->(render.Overlay_Hit,km.Vec3,km.Vec3,bool) {
    for identity,i in mesh.triangles {
        if identity.handle!=handle { continue }
        point:=km.xyz(mesh.vertices[i*3].position+mesh.vertices[i*3+1].position+mesh.vertices[i*3+2].position)/3
        pixel:=gizmo_test_project(frame,point); hit:=gizmo_hit(mesh,frame.view_projection,{20,30,640,480},pixel)
        if hit.hit && hit.handle==handle { origin,direction,ok:=gizmo_ray(frame.view_projection,{20,30,640,480},pixel); return hit,origin,direction,ok }
    }; return {},{},{},false
}
@(test)
test_gizmo_rendered_axis_pointer_multiselection_parent_filter_and_one_undo :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner); app.scene_components_register(&owner)
    a:=ecs.spawn(&owner.world,struct { transform:app.Scene_Transform }{{km.transform()}})
    b:=ecs.spawn(&owner.world,struct { transform:app.Scene_Transform }{{km.transform(position={3,2,0})}})
    child:=ecs.spawn(&owner.world,struct { transform:app.Scene_Transform,parent:app.Scene_Parent }{{km.transform(position={1,0,0})},{a}})
    frame:=gizmo_test_frame(); state:=render.Editor_Overlay_State{selected={a,b,child},gizmo=true,basis=km.identity(km.Mat4)}
    mesh,mesh_error:=render.overlay_mesh_prepare(&owner,state,frame,640,480); testing.expect_value(t,mesh_error,editor.Scene_Error.None); defer render.overlay_mesh_destroy(&mesh)
    hit,origin,direction,ok:=gizmo_test_pick(&mesh,frame,.Axis_X); testing.expect(t,ok && hit.entity==a)
    controller:Gizmo_Controller; defer gizmo_destroy(&controller)
    testing.expect_value(t,gizmo_begin(&controller,&owner,{a,b,child},hit,origin,direction,{},state.basis,.Translate,{translation=.5}),editor.Scene_Error.None)
    testing.expect(t,len(controller.rows)==2 && controller.gesture.active)
    pixel:=gizmo_test_project(frame,controller.start+km.Vec3{1.1,0,0}); origin,direction,ok=gizmo_ray(frame.view_projection,{20,30,640,480},pixel); testing.expect(t,ok)
    testing.expect_value(t,gizmo_move(&controller,origin,direction),editor.Scene_Error.None)
    first,_:=ecs.get_component(&owner.world,a,app.Scene_Transform); second,_:=ecs.get_component(&owner.world,b,app.Scene_Transform); local,_:=ecs.get_component(&owner.world,child,app.Scene_Transform)
    testing.expect(t,abs(first.local.position[0]-1)<.001 && abs(second.local.position[0]-4)<.001 && second.local.position[1]==2 && local.local.position==km.Vec3{1,0,0})
    world,error:=app.scene_world_matrix(&owner,child); testing.expect(t,error==.None && abs(world[3][0]-2)<.001)
    testing.expect(t,!editor.agent_can_undo(&owner.agent.session))
    testing.expect_value(t,gizmo_finish(&controller),editor.Scene_Error.None); testing.expect(t,!controller.gesture.active && editor.agent_can_undo(&owner.agent.session))
    testing.expect_value(t,app.authoring_undo_last(&owner),editor.Scene_Error.None); first,_=ecs.get_component(&owner.world,a,app.Scene_Transform); second,_=ecs.get_component(&owner.world,b,app.Scene_Transform)
    testing.expect(t,first.local.position==km.VEC3_ZERO && second.local.position==km.Vec3{3,2,0} && !editor.agent_can_undo(&owner.agent.session))
    testing.expect_value(t,app.authoring_redo_last(&owner),editor.Scene_Error.None); first,_=ecs.get_component(&owner.world,a,app.Scene_Transform); testing.expect(t,abs(first.local.position[0]-1)<.001)
}

@(test)
test_gizmo_rendered_rotation_and_local_plane_scale_snap_cancel :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner); app.scene_components_register(&owner)
    entity:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform}{{km.transform()}})
    frame:=gizmo_test_frame()
    state:=render.Editor_Overlay_State{selected={entity},gizmo=true,basis=km.identity(km.Mat4),mode=.Rotate}
    mesh,error:=render.overlay_mesh_prepare(&owner,state,frame,640,480); testing.expect_value(t,error,editor.Scene_Error.None); defer render.overlay_mesh_destroy(&mesh)
    hit,origin,direction,valid:=gizmo_test_pick(&mesh,frame,.Axis_Z); testing.expect(t,valid)
    controller:Gizmo_Controller; defer gizmo_destroy(&controller)
    testing.expect_value(t,gizmo_begin(&controller,&owner,{entity},hit,origin,direction,{},state.basis,.Rotate,{rotation=45}),editor.Scene_Error.None)
    rotated:=km.quat_transform_vector(km.quat_axis_angle(km.VEC3_Z,50*m.PI/180),controller.start)
    origin,direction,valid=gizmo_ray(frame.view_projection,{20,30,640,480},gizmo_test_project(frame,rotated)); testing.expect(t,valid)
    testing.expect_value(t,gizmo_move(&controller,origin,direction),editor.Scene_Error.None)
    transform,_:=ecs.get_component(&owner.world,entity,app.Scene_Transform); expected:=km.quat_axis_angle(km.VEC3_Z,m.PI/4)
    testing.expect(t,abs(km.quat_dot(transform.local.rotation,expected))>.9999 && transform.local.position==km.VEC3_ZERO)
    testing.expect_value(t,gizmo_cancel(&controller),editor.Scene_Error.None); transform,_=ecs.get_component(&owner.world,entity,app.Scene_Transform); testing.expect(t,transform.local==km.TRANSFORM_IDENTITY && !editor.agent_can_undo(&owner.agent.session))
    rotation:=km.quat_axis_angle(km.VEC3_Z,m.PI/6); ecs.get_component_mut(&owner.world,entity,app.Scene_Transform).local.rotation=rotation
    state.basis=km.quat_to_mat4(rotation); state.mode=.Scale
    scale_mesh,scale_error:=render.overlay_mesh_prepare(&owner,state,frame,640,480); testing.expect_value(t,scale_error,editor.Scene_Error.None); defer render.overlay_mesh_destroy(&scale_mesh)
    hit,origin,direction,valid=gizmo_test_pick(&scale_mesh,frame,.Axis_X); testing.expect(t,valid)
    testing.expect_value(t,gizmo_begin(&controller,&owner,{entity},hit,origin,direction,{},state.basis,.Scale,{scale=.25}),editor.Scene_Error.None)
    target:=controller.start+km.xyz(state.basis[0])*controller.start_axis*1.12
    origin,direction,valid=gizmo_ray(frame.view_projection,{20,30,640,480},gizmo_test_project(frame,target)); testing.expect(t,valid)
    testing.expect_value(t,gizmo_move(&controller,origin,direction),editor.Scene_Error.None); transform,_=ecs.get_component(&owner.world,entity,app.Scene_Transform)
    testing.expect(t,abs(transform.local.scale[0]-2)<.001 && transform.local.scale[1]==1 && transform.local.scale[2]==1 && abs(km.quat_dot(transform.local.rotation,rotation))>.9999)
    testing.expect_value(t,gizmo_cancel(&controller),editor.Scene_Error.None)
    state.mode=.Translate; plane_mesh,plane_error:=render.overlay_mesh_prepare(&owner,state,frame,640,480); testing.expect_value(t,plane_error,editor.Scene_Error.None); defer render.overlay_mesh_destroy(&plane_mesh)
    hit,origin,direction,valid=gizmo_test_pick(&plane_mesh,frame,.Plane_XY); testing.expect(t,valid)
    testing.expect_value(t,gizmo_begin(&controller,&owner,{entity},hit,origin,direction,{},state.basis,.Translate,{translation=.5}),editor.Scene_Error.None)
    target=controller.start+km.xyz(state.basis[0])*.6+km.xyz(state.basis[1])*1.1
    origin,direction,valid=gizmo_ray(frame.view_projection,{20,30,640,480},gizmo_test_project(frame,target)); testing.expect(t,valid)
    testing.expect_value(t,gizmo_move(&controller,origin,direction),editor.Scene_Error.None); transform,_=ecs.get_component(&owner.world,entity,app.Scene_Transform)
    testing.expect(t,km.length(transform.local.position-(km.xyz(state.basis[0])*.5+km.xyz(state.basis[1])))<.001 && transform.local.position[2]==0)
    testing.expect_value(t,gizmo_cancel(&controller),editor.Scene_Error.None)
}

@(private="file")
gizmo_test_prepare :: proc(state:rawptr,owner:^app.Authoring,ids:[]ecs.Entity_Id,mode:app.Scene_Preparation_Mode)->(rawptr,editor.Scene_Error) { return nil,.Invalid_Operation if (cast(^bool)state)^ else .None }
@(private="file")
gizmo_test_finish :: proc(state,token:rawptr,committed:bool) {}
@(test)
test_gizmo_rejected_atomic_preview_retains_accepted_state_and_cancel :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner); app.scene_components_register(&owner)
    a:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform}{{km.transform()}})
    b:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform}{{km.transform(position={3,0,0})}})
    frame:=gizmo_test_frame(); state:=render.Editor_Overlay_State{selected={a,b},gizmo=true,basis=km.identity(km.Mat4)}
    mesh,error:=render.overlay_mesh_prepare(&owner,state,frame,640,480); testing.expect_value(t,error,editor.Scene_Error.None); defer render.overlay_mesh_destroy(&mesh)
    hit,origin,direction,ok:=gizmo_test_pick(&mesh,frame,.Axis_X); testing.expect(t,ok)
    controller:Gizmo_Controller; defer gizmo_destroy(&controller)
    testing.expect_value(t,gizmo_begin(&controller,&owner,{a,b},hit,origin,direction,{},state.basis,.Translate),editor.Scene_Error.None)
    origin,direction,ok=gizmo_ray(frame.view_projection,{20,30,640,480},gizmo_test_project(frame,controller.start+km.Vec3{1,0,0})); testing.expect(t,ok)
    testing.expect_value(t,gizmo_move(&controller,origin,direction),editor.Scene_Error.None)
    accepted_a,_:=ecs.get_component(&owner.world,a,app.Scene_Transform); accepted_b,_:=ecs.get_component(&owner.world,b,app.Scene_Transform)
    reject:=true; ecs.insert_resource(&owner.world,app.Scene_Participant{&reject,gizmo_test_prepare,gizmo_test_finish})
    origin,direction,ok=gizmo_ray(frame.view_projection,{20,30,640,480},gizmo_test_project(frame,controller.start+km.Vec3{2,0,0})); testing.expect(t,ok)
    testing.expect_value(t,gizmo_move(&controller,origin,direction),editor.Scene_Error.Invalid_Operation)
    first,_:=ecs.get_component(&owner.world,a,app.Scene_Transform); second,_:=ecs.get_component(&owner.world,b,app.Scene_Transform)
    testing.expect(t,first==accepted_a && second==accepted_b && controller.gesture.active && !editor.agent_can_undo(&owner.agent.session))
    testing.expect_value(t,gizmo_move(&controller,origin,{}),editor.Scene_Error.Invalid_Operation)
    testing.expect_value(t,gizmo_cancel(&controller),editor.Scene_Error.Invalid_Operation); testing.expect(t,controller.gesture.active)
    reject=false; testing.expect_value(t,gizmo_cancel(&controller),editor.Scene_Error.None)
    first,_=ecs.get_component(&owner.world,a,app.Scene_Transform); second,_=ecs.get_component(&owner.world,b,app.Scene_Transform)
    testing.expect(t,first.local==km.TRANSFORM_IDENTITY && second.local.position==km.Vec3{3,0,0} && !controller.gesture.active && !editor.agent_can_undo(&owner.agent.session))
}

@(test)
test_gizmo_parent_grid_snap_and_rotation_wrap_from_projected_pointer :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner); app.scene_components_register(&owner)
    parent:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform}{{km.transform(position={10,0,0})}})
    entity:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,parent:app.Scene_Parent}{{km.transform(position={.3,.2,0})},{parent}})
    pivot:=km.Vec3{10.3,.2,0}
    frame,_:=render.frame_data({position=pivot+km.Vec3{4,3,8},target=pivot,up={0,1,0},fov_degrees=60,near=.01,far=100},640,480,false)
    state:=render.Editor_Overlay_State{selected={entity},pivot=pivot,gizmo=true,basis=km.identity(km.Mat4)}
    mesh,error:=render.overlay_mesh_prepare(&owner,state,frame,640,480); testing.expect_value(t,error,editor.Scene_Error.None); defer render.overlay_mesh_destroy(&mesh)
    hit,origin,direction,ok:=gizmo_test_pick(&mesh,frame,.Axis_X); testing.expect(t,ok)
    controller:Gizmo_Controller; defer gizmo_destroy(&controller)
    testing.expect_value(t,gizmo_begin(&controller,&owner,{entity},hit,origin,direction,pivot,state.basis,.Translate,{translation=.5}),editor.Scene_Error.None)
    origin,direction,ok=gizmo_ray(frame.view_projection,{20,30,640,480},gizmo_test_project(frame,controller.start+km.Vec3{1.1,0,0})); testing.expect(t,ok)
    testing.expect_value(t,gizmo_move(&controller,origin,direction),editor.Scene_Error.None)
    transform,_:=ecs.get_component(&owner.world,entity,app.Scene_Transform); testing.expect(t,abs(transform.local.position[0]-1.5)<.001 && transform.local.position[1]==.2)
    testing.expect_value(t,gizmo_cancel(&controller),editor.Scene_Error.None)
    state.mode=.Rotate; rotate_mesh,rotate_error:=render.overlay_mesh_prepare(&owner,state,frame,640,480); testing.expect_value(t,rotate_error,editor.Scene_Error.None); defer render.overlay_mesh_destroy(&rotate_mesh)
    hit,origin,direction,ok=gizmo_test_pick(&rotate_mesh,frame,.Axis_Z); testing.expect(t,ok)
    testing.expect_value(t,gizmo_begin(&controller,&owner,{entity},hit,origin,direction,pivot,state.basis,.Rotate),editor.Scene_Error.None)
    for quarter in 1..<6 {
        point:=pivot+km.quat_transform_vector(km.quat_axis_angle(km.VEC3_Z,f32(quarter)*m.PI/2),controller.start-pivot)
        origin,direction,ok=gizmo_ray(frame.view_projection,{20,30,640,480},gizmo_test_project(frame,point)); testing.expect(t,ok)
        testing.expect_value(t,gizmo_move(&controller,origin,direction),editor.Scene_Error.None)
    }
    testing.expect(t,abs(controller.angle-2.5*m.PI)<.001)
    transform,_=ecs.get_component(&owner.world,entity,app.Scene_Transform); testing.expect(t,abs(km.quat_dot(transform.local.rotation,km.quat_axis_angle(km.VEC3_Z,m.PI/2)))>.9999)
    testing.expect_value(t,gizmo_cancel(&controller),editor.Scene_Error.None)
}
