#+test
//! Registered authoring edits reach actual native shapes; candidate failure preserves accepted owners.
package app
import ecs "../ecs"
import editor "../editor"
import km "../math"
import box3d "../physics/box3d"
import "core:testing"
import "core:mem"
import "core:encoding/json"

@(private="file")
Physics_Inspector_Candidate :: struct {library:string,prepared:int}
@(private="file")
physics_inspector_reject_candidate :: proc(state:rawptr,owner:^Authoring,_:[]ecs.Entity_Id,_:Scene_Preparation_Mode)->(rawptr,editor.Scene_Error) {
    witness:=cast(^Physics_Inspector_Candidate)state
    bodies,error:=physics_collect(owner); defer physics_collected_destroy(&bodies,owner.world.allocator)
    if error!=.None { return nil,error }
    candidate:box3d.Backend
    if box3d.backend_init(&candidate,witness.library,owner.world.allocator)!=.None { return nil,.Application_Owned }
    defer box3d.backend_destroy(&candidate)
    native:=make([dynamic]box3d.Body,owner.world.allocator); defer delete(native)
    for item in bodies {
        body:=item.body
        append(&native,box3d.Body{id=item.id,body_type=cast(box3d.Body_Type)body.body_type,shape_kind=cast(box3d.Shape_Kind)body.shape.kind,
            position=item.position,rotation=item.rotation,linear_velocity=body.linear_velocity,half_extents=body.shape.half_extents,radius=body.shape.radius,half_height=body.shape.half_height,
            density=body.density,gravity_scale=body.gravity_scale,friction=body.friction,restitution=body.restitution,layers=body.layers,mask=body.mask,sensor=u32(body.sensor),ccd=u32(body.ccd)})
    }
    if box3d.backend_sync(&candidate,native[:])!=.None { return nil,.Invalid_Operation }
    witness.prepared+=1
    // Inject rejection after genuine C world/body/shape allocation, before accepted owner publication.
    return nil,.Invalid_Operation
}
@(private="file")
physics_inspector_finish_candidate :: proc(_:rawptr,_:rawptr,_:bool) { panic("rejected preparation has no publication token") }
@(private="file")
physics_inspector_record :: proc(owner:^Authoring,op:editor.Scene_Op)->editor.Scene_Error {
    result,group:=scene_action_execute(owner,op); error:=result.error
    if error==.None { editor.agent_record_action(&owner.agent.session,op,&result,&group) }
    editor.tool_result_destroy(&result); editor.undo_group_destroy(&group); return error
}
@(private="file")
physics_inspector_native_acceptance :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:Authoring; authoring_init(&owner); testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None); testing.expect_value(t,physics_select_box3d(&owner,BOX3D_LIBRARY),editor.Scene_Error.None)
    entity:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,entity,Scene_Transform{km.TRANSFORM_IDENTITY})
    testing.expect_value(t,physics_inspector_record(&owner,{kind=.Add_Component,entity=entity,component="PhysicsBody"}),editor.Scene_Error.None)
    testing.expect_value(t,physics_prepare(&owner),editor.Scene_Error.None)
    native:=ecs.get_resource_mut(&owner.world,box3d.Backend); accepted:=native.entries[u64(entity)].native
    ray:=box3d.backend_raycast(native,{0,0,-2},{0,0,1},4); testing.expect(t,ray.error==.None && ray.ray.hit!=0 && abs(ray.ray.distance-1.5)<0.001)
    next:Physics_Shape={kind=.Sphere,half_extents={.5,.5,.5},radius=.25,half_height=.5}
    data,encode_error:=json.marshal(next); testing.expect(t,encode_error==nil)
    op:=editor.Scene_Op{kind=.Set_Field,entity=entity,component="PhysicsBody",field="shape",value=data}
    testing.expect_value(t,physics_inspector_record(&owner,op),editor.Scene_Error.None); delete(data)
    testing.expect_value(t,physics_prepare(&owner),editor.Scene_Error.None)
    changed:=native.entries[u64(entity)].native; testing.expect(t,changed!=accepted)
    ray=box3d.backend_raycast(native,{0,0,-2},{0,0,1},4); testing.expect(t,ray.error==.None && abs(ray.ray.distance-1.75)<0.001)
    testing.expect_value(t,authoring_undo_last(&owner),editor.Scene_Error.None); testing.expect_value(t,physics_prepare(&owner),editor.Scene_Error.None)
    ray=box3d.backend_raycast(native,{0,0,-2},{0,0,1},4); testing.expect(t,ray.error==.None && abs(ray.ray.distance-1.5)<0.001)
    testing.expect_value(t,authoring_redo_last(&owner),editor.Scene_Error.None); testing.expect_value(t,physics_prepare(&owner),editor.Scene_Error.None)
    witness:=Physics_Inspector_Candidate{library=BOX3D_LIBRARY}; ecs.insert_resource(&owner.world,Scene_Participant{&witness,physics_inspector_reject_candidate,physics_inspector_finish_candidate})
    before:=native.entries[u64(entity)].native; before_history:=len(owner.agent.session.actions); baseline:=native.native_bytes()
    result,group:=scene_action_execute(&owner,{kind=.Set_Field,entity=entity,component="PhysicsBody",field="friction",value=transmute([]byte)string("0.9")})
    testing.expect_value(t,result.error,editor.Scene_Error.Invalid_Operation); testing.expect(t,group.state==nil && witness.prepared==1 && len(owner.agent.session.actions)==before_history && native.entries[u64(entity)].native==before && native.native_bytes()==baseline)
    retained,_:=ecs.get_component(&owner.world,entity,Physics_Body); testing.expect(t,retained.friction==.5 && retained.shape.kind==.Sphere && retained.shape.radius==.25)
    editor.tool_result_destroy(&result); editor.undo_group_destroy(&group)
    authoring_destroy(&owner); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
when BOX3D_LIBRARY!="" {
@(test)
test_physics_inspector_native_shape_undo_and_post_allocation_candidate_rejection :: proc(t:^testing.T) { physics_inspector_native_acceptance(t) }
}
