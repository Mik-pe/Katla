//! Native Box3D ownership shares scene preflight and atomic pose publication.
package app
import box3d "../physics/box3d"
import ecs "../ecs"
import editor "../editor"

@(private="package")
physics_box3d_destroy :: proc(value:rawptr) {
    owner:=cast(^box3d.Backend)value
    error:=box3d.backend_destroy(owner); assert(error==.None,"native physics owner requires its application thread")
}
/// Publishes the sole native physics owner after all mandatory ABI entries have initialized.
physics_select_box3d :: proc(owner:^Authoring,library_path:string)->editor.Scene_Error {
    if owner.mode!=.Editing { return .Editing_Required }
    if ecs.contains_resource(&owner.world,box3d.Backend) { return .Invalid_Operation }
    backend:box3d.Backend
    if box3d.backend_init(&backend,library_path,owner.world.allocator)!=.None { return .Application_Owned }
    ecs.insert_resource(&owner.world,backend,ecs.Value_Ops{destroy=physics_box3d_destroy})
    return .None
}
@(private="package")
physics_unconfigured :: proc(owner:^Authoring)->editor.Scene_Error {
    bodies,error:=physics_collect(owner); defer physics_collected_destroy(&bodies,owner.world.allocator)
    if error!=.None { return error }
    joints,joint_error:=physics_collect_joints(owner); defer delete(joints,owner.world.allocator)
    if joint_error!=.None { return joint_error }
    if len(bodies)>0 || len(joints)>0 { return .Application_Owned }
    return .None
}
/// Steps real Box3D; authored participants require an initialized native owner.
physics_step :: proc(owner:^Authoring,delta_seconds:f32)->Physics_Step_Result {
    if ecs.contains_resource(&owner.world,box3d.Backend) { return physics_box3d_step(owner,delta_seconds) }
    result:=Physics_Step_Result{events=make([dynamic]Physics_Event,owner.world.allocator),allocator=owner.world.allocator}
    result.error=.Invalid_Field_Value if !finite_nonnegative(delta_seconds) || delta_seconds>.25 else physics_unconfigured(owner)
    return result
}
/// Prepares every authored body and constraint before entering Play.
physics_prepare :: proc(owner:^Authoring)->editor.Scene_Error {
    if ecs.contains_resource(&owner.world,box3d.Backend) { return physics_box3d_sync(owner) }
    return physics_unconfigured(owner)
}
/// Clears native transient state before restoring the authored baseline.
physics_reset :: proc(owner:^Authoring)->editor.Scene_Error {
    backend:=ecs.get_resource_mut(&owner.world,box3d.Backend)
    if backend==nil { return physics_unconfigured(owner) }
    if box3d.backend_reset(backend)!=.None { return .Invalid_Operation }
    return .None
}
/// Synchronizes current authoring state through the shared hierarchy/collider preflight.
physics_box3d_sync :: proc(app:^Authoring)->editor.Scene_Error { return physics_box3d_sync_excluding(app,nil) }
@(private="package")
physics_box3d_sync_excluding :: proc(app:^Authoring,excluded:[]ecs.Entity_Id)->editor.Scene_Error {
    owner:=ecs.get_resource_mut(&app.world,box3d.Backend)
    if owner==nil { return .Application_Owned }
    joints,joint_error:=physics_collect_joints(app); defer delete(joints,app.world.allocator)
    if joint_error!=.None { return joint_error }
    native_joints:=make([dynamic]box3d.Joint,app.world.allocator); defer delete(native_joints)
    for resolved in joints { joint:=resolved.joint; skip:=false; for id in excluded { if u64(id)==resolved.id || id==joint.a || id==joint.b { skip=true; break } }; if skip { continue }; append(&native_joints,box3d.Joint{id=resolved.id,a=u64(joint.a),b=u64(joint.b),kind=cast(box3d.Joint_Kind)joint.kind,has_limits=u32(joint.has_limits),anchor_a=joint.anchor_a,anchor_b=joint.anchor_b,limits=joint.limits}) }
    collected,err:=physics_collect(app); defer physics_collected_destroy(&collected,app.world.allocator)
    if err!=.None { return err }
    bodies:=make([dynamic]box3d.Body,app.world.allocator); defer delete(bodies)
    for resolved in collected {
        skip:=false; for id in excluded { if u64(id)==resolved.id { skip=true; break } }; if skip { continue }
        body:=resolved.body
        indices:=resolved.indices; if body.shape.kind!=.Trimesh { indices=nil }
        append(&bodies,box3d.Body{id=resolved.id,body_type=cast(box3d.Body_Type)body.body_type,shape_kind=cast(box3d.Shape_Kind)body.shape.kind,
            position=resolved.position,rotation=resolved.rotation,linear_velocity=body.linear_velocity,
            heights=raw_data(body.shape.heights),rows=body.shape.rows,cols=body.shape.cols,height_scale=resolved.height_scale,
            half_extents=body.shape.half_extents,radius=body.shape.radius,half_height=body.shape.half_height,
            vertices=raw_data(resolved.vertices),indices=raw_data(indices),vertex_count=u32(len(resolved.vertices)),index_count=u32(len(indices)),density=body.density,
            gravity_scale=body.gravity_scale,friction=body.friction,restitution=body.restitution,
            layers=body.layers,mask=body.mask,sensor=u32(body.sensor),ccd=u32(body.ccd)})
    }
    if box3d.backend_sync(owner,bodies[:],native_joints[:])!=.None { return .Invalid_Operation }
    return .None
}
/// Publishes validated native world poses atomically and transfers directed native sensor events.
physics_box3d_step :: proc(app:^Authoring,delta_seconds:f32)->Physics_Step_Result {
    result:=Physics_Step_Result{events=make([dynamic]Physics_Event,app.world.allocator),allocator=app.world.allocator}
    if !finite_nonnegative(delta_seconds) || delta_seconds>0.25 { result.error=.Invalid_Field_Value; return result }
    result.error=physics_box3d_sync(app); if result.error!=.None || delta_seconds==0 { return result }
    owner:=ecs.get_resource_mut(&app.world,box3d.Backend)
    output:=box3d.backend_step(owner,delta_seconds); defer box3d.step_destroy(&output)
    if output.error!=.None { result.error=.Invalid_Operation; return result }
    poses:=make([]Physics_Resolved_Pose,len(output.poses),app.world.allocator); defer delete(poses,app.world.allocator)
    for pose,i in output.poses { poses[i]={pose.id,pose.position,pose.rotation,pose.linear_velocity} }
    result.error=physics_commit_poses(app,poses); if result.error!=.None { return result }
    for event in output.events {
        phase:=Trigger_Phase.Enter if event.phase==.Enter else Trigger_Phase.Exit
        append(&result.events,Physics_Event{phase,ecs.Entity_Id(event.pair.trigger),ecs.Entity_Id(event.pair.other)})
    }
    return result
}
