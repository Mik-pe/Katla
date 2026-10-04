//! Explicit native backend selection shares scene preflight and pose publication.
package app

import box3d "../physics/box3d"
import ecs "../ecs"
import editor "../editor"

Physics_Backend :: enum { Rapier, Box3D }
/// Stores the selected implementation; an unconfigured scene uses the established Rapier backend.
Physics_Selection :: struct { backend:Physics_Backend }
@(private="package")
physics_box3d_destroy :: proc(value:rawptr) {
    owner:=cast(^box3d.Backend)value
    err:=box3d.backend_destroy(owner); assert(err==.None,"native physics owner must be destroyed on its application thread")
}
/// Loads and selects actual Box3D only after the complete native ABI has initialized successfully.
physics_select_box3d :: proc(app:^Authoring,library_path:string)->editor.Scene_Error {
    if app.mode!=.Editing { return .Editing_Required }
    if ecs.contains_resource(&app.world,box3d.Backend) { return .Invalid_Operation }
    owner:box3d.Backend
    err:=box3d.backend_init(&owner,library_path,app.world.allocator)
    if err!=.None { return .Application_Owned }
    if ecs.contains_resource(&app.world,Scene_Runtime) {
        response:=scene_runtime_call(app,struct {method:string}{"physics_reset"}); defer runtime_response_destroy(&response)
        if !response.ok { box3d.backend_destroy(&owner); return .Invalid_Operation }
    }
    ecs.insert_resource(&app.world,owner,ecs.Value_Ops{destroy=physics_box3d_destroy})
    ecs.insert_resource(&app.world,Physics_Selection{.Box3D})
    return .None
}
/// Selects Rapier on the application thread and releases any previously selected Box3D owner.
physics_select_rapier :: proc(app:^Authoring)->editor.Scene_Error {
    if app.mode!=.Editing { return .Editing_Required }
    if !ecs.contains_resource(&app.world,Scene_Runtime) { return .Application_Owned }
    response:=scene_runtime_call(app,struct {method:string}{"physics_reset"}); defer runtime_response_destroy(&response)
    if !response.ok { return .Invalid_Operation }
    ecs.remove_resource(&app.world,box3d.Backend)
    ecs.insert_resource(&app.world,Physics_Selection{.Rapier})
    return .None
}
/// Steps the explicitly selected dependency through the same scene ownership and commit contract.
physics_step :: proc(app:^Authoring,delta_seconds:f32)->Physics_Step_Result {
    selection,present:=ecs.get_resource(&app.world,Physics_Selection)
    if present && selection.backend==.Box3D { return physics_box3d_step(app,delta_seconds) }
    return physics_rapier_step(app,delta_seconds)
}
/// Prepares native collision participation before play through the selected backend.
physics_prepare :: proc(app:^Authoring)->editor.Scene_Error {
    selection,present:=ecs.get_resource(&app.world,Physics_Selection)
    if present && selection.backend==.Box3D { return physics_box3d_sync(app) }
    return physics_sync(app)
}
/// Clears transient physics state before restoring authored scene identities.
physics_reset :: proc(app:^Authoring)->editor.Scene_Error {
    selection,present:=ecs.get_resource(&app.world,Physics_Selection)
    if present && selection.backend==.Box3D {
        owner:=ecs.get_resource_mut(&app.world,box3d.Backend)
        if owner==nil { return .Application_Owned }
        if box3d.backend_reset(owner)!=.None { return .Invalid_Operation }
        return .None
    }
    if !ecs.contains_resource(&app.world,Scene_Runtime) {
        ids:=ecs.entity_ids(&app.world); defer delete(ids)
        for entity in ids { if _,body:=ecs.get_component(&app.world,entity,Physics_Body); body { return .Application_Owned } }
        return .None
    }
    response:=scene_runtime_call(app,struct { method:string }{"physics_reset"}); defer runtime_response_destroy(&response)
    if !response.ok { return .Invalid_Operation }; return .None
}
/// Synchronizes current authoring state through the shared hierarchy/collider preflight.
physics_box3d_sync :: proc(app:^Authoring)->editor.Scene_Error {
    owner:=ecs.get_resource_mut(&app.world,box3d.Backend)
    if owner==nil { return .Application_Owned }
    collected,err:=physics_collect(app); defer physics_collected_destroy(&collected,app.world.allocator)
    if err!=.None { return err }
    bodies:=make([]box3d.Body,len(collected),app.world.allocator); defer delete(bodies,app.world.allocator)
    for resolved,i in collected {
        body:=resolved.body
        bodies[i]={id=resolved.id,body_type=cast(box3d.Body_Type)body.body_type,shape_kind=cast(box3d.Shape_Kind)body.shape.kind,
            position=resolved.position,rotation=resolved.rotation,linear_velocity=body.linear_velocity,
            half_extents=body.shape.half_extents,radius=body.shape.radius,half_height=body.shape.half_height,
            vertices=raw_data(resolved.vertices),indices=raw_data(resolved.indices),vertex_count=u32(len(resolved.vertices)),index_count=u32(len(resolved.indices)),density=body.density,
            gravity_scale=body.gravity_scale,friction=body.friction,restitution=body.restitution,
            layers=body.layers,mask=body.mask,sensor=u32(body.sensor),ccd=u32(body.ccd)}
    }
    if box3d.backend_sync(owner,bodies)!=.None { return .Invalid_Operation }
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
