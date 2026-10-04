//! Scene-owned physics descriptions synchronize through the canonical dependency runtime.
package app

import ecs "../ecs"
import editor "../editor"
import agent "../agent"
import scene "../agent/scene"
import km "../math"
import "core:encoding/json"
import "core:fmt"
import "core:mem"
import "core:math"

Trigger_Phase :: scene.Trigger_Phase
Physics_Body_Type :: enum { Dynamic, Kinematic, Fixed }
Physics_Shape_Kind :: enum { Box, Sphere, Capsule, Trimesh, ConvexHull }
/// Primitive dimensions stay local; mesh kinds use the entity's sole Scene_Mesh geometry.
Physics_Shape :: struct { kind:Physics_Shape_Kind,half_extents:[3]f32,radius,half_height:f32 }
/// Native body/collider handles remain wholly inside the selected dependency owner.
Physics_Body :: struct {
    body_type:Physics_Body_Type `inspect:"skip"`,shape:Physics_Shape `inspect:"skip"`,sensor:bool `inspect:"skip"`,
    linear_velocity:[3]f32 `inspect:"skip"`,gravity_scale,friction,restitution,density:f32 `inspect:"skip"`,layers,mask:u32 `inspect:"skip"`,ccd:bool `inspect:"skip"`,
}
/// Actual completed-step overlap feedback preserves complete generational identity.
Physics_Event :: struct { phase:Trigger_Phase,trigger,other:ecs.Entity_Id }
/// Owns actual dependency feedback; callers dispatch in order then destroy it.
Physics_Step_Result :: struct { events:[dynamic]Physics_Event,error:editor.Scene_Error,allocator:mem.Allocator }
/// Creates a conventional dynamic collider without native allocations.
physics_body :: proc(shape:Physics_Shape,body_type:=Physics_Body_Type.Dynamic,sensor:=false)->Physics_Body { return {body_type=body_type,shape=shape,sensor=sensor,gravity_scale=1,friction=0.5,density=1,layers=max(u32),mask=max(u32)} }
/// A durable motion descriptor retained by the scene and exposed to authored component tools.
Scene_Velocity :: struct {velocity,acceleration:[3]f32}
/// Installs optional authored physics descriptions; application spawn chooses participation.
physics_register :: proc(app:^Authoring) { editor.editor_register(&app.world,&app.registry,"PhysicsBody",Physics_Body{},spawn_default=false); editor.editor_register(&app.world,&app.registry,"Velocity",Scene_Velocity{},spawn_default=false) }
/// Releases actual feedback with its captured owner allocator.
physics_step_result_destroy :: proc(result:^Physics_Step_Result) { delete(result.events); result^={} }
@(private="package")
Physics_Wire_Body :: struct { entity_id,body_type:string,position:[3]f32,rotation:[4]f32,shape:json.Value,sensor:bool,linear_velocity:[3]f32,gravity_scale,friction,restitution,density:f32,layers,mask:u32,ccd:bool }
@(private="package")
Physics_Wire_Pose :: struct { entity_id:string,position:[3]f32,rotation:[4]f32,linear_velocity:[3]f32 }
@(private="package")
Physics_Wire_Event :: struct { phase,trigger_entity,other_entity:string }
@(private="package")
Physics_Wire_Overlap :: struct { trigger_entity,other_entity:string }
@(private="package")
Physics_Output :: struct { poses:[]Physics_Wire_Pose,events:[]Physics_Wire_Event,overlaps:[]Physics_Wire_Overlap }
@(private="package")
physics_output_destroy :: proc(output:^Physics_Output) { for pose in output.poses { delete(pose.entity_id) }; for event in output.events { delete(event.phase); delete(event.trigger_entity); delete(event.other_entity) }; for pair in output.overlaps { delete(pair.trigger_entity); delete(pair.other_entity) }; delete(output.poses); delete(output.events); delete(output.overlaps); output^={} }
/// Checks all local collision dimensions and authoring factors without touching the backend.
physics_body_valid :: proc(body:Physics_Body)->bool {
    if body.body_type not_in (bit_set[Physics_Body_Type]{.Dynamic,.Kinematic,.Fixed}) { return false }
    if !finite_nonnegative(body.density) || !finite_nonnegative(body.gravity_scale) || !finite_nonnegative(body.friction) || !finite_nonnegative(body.restitution) || body.restitution>1 { return false }
    for value in body.linear_velocity { if math.is_nan(value) || math.is_inf(value) { return false } }
    switch body.shape.kind {
    case .Box: for value in body.shape.half_extents { if !finite_nonnegative(value) || value<=0 { return false } }
    case .Sphere: if !finite_nonnegative(body.shape.radius) || body.shape.radius<=0 { return false }
    case .Capsule: if !finite_nonnegative(body.shape.radius) || body.shape.radius<=0 || !finite_nonnegative(body.shape.half_height) { return false }
    case .Trimesh,.ConvexHull:
    case: return false
    }
    return true
}
@(private="package")
physics_trs :: proc(world_matrix:km.Mat4)->(km.Transform,bool) {
    pose,ok:=km.mat4_decompose_approx(world_matrix); if !ok || !km.quat_is_normalized(pose.rotation) { return {},false }
    rebuilt:=km.transform_to_mat4(pose); for col in 0..<4 { for row in 0..<4 { if abs(rebuilt[col][row]-world_matrix[col][row])>0.001 { return {},false } } }; return pose,true
}
@(private="package")
physics_shape_wire :: proc(description:Physics_Resolved_Body)->(json.Value,bool) {
    shape:=description.body.shape
    bytes:[]byte; err:json.Marshal_Error
    switch shape.kind {
    case .Box: bytes,err=json.marshal(struct { kind:string,half_extents:[3]f32 }{"box",shape.half_extents})
    case .Sphere: bytes,err=json.marshal(struct { kind:string,radius:f32 }{"sphere",shape.radius})
    case .Capsule: bytes,err=json.marshal(struct { kind:string,radius,half_height:f32 }{"capsule",shape.radius,shape.half_height})
    case .Trimesh:
        triangles:=make([][3]u32,len(description.indices)/3,description.allocator); defer delete(triangles,description.allocator)
        for &triangle,i in triangles { copy(triangle[:],description.indices[i*3:i*3+3]) }
        bytes,err=json.marshal(struct {kind:string,vertices:[][3]f32,indices:[][3]u32}{"trimesh",description.vertices,triangles})
    case .ConvexHull: bytes,err=json.marshal(struct {kind:string,vertices:[][3]f32}{"convex_hull",description.vertices})
    }
    if err!=nil { return nil,false }; defer delete(bytes)
    tree,parse_error:=json.parse(bytes,spec=.JSON,parse_integers=true); return tree,parse_error==nil
}
/// Temporary collider geometry owns the exact affine bake; the durable source remains Scene_Mesh.
Physics_Resolved_Body :: struct { id:u64,body:Physics_Body,position:[3]f32,rotation:[4]f32,vertices:[][3]f32,indices:[]u32,allocator:mem.Allocator }
/// Releases every temporary collider stream and its container after native synchronization.
physics_collected_destroy :: proc(bodies:^[]Physics_Resolved_Body,allocator:=context.allocator) {
    for body in bodies^ { delete(body.vertices,body.allocator); delete(body.indices,body.allocator) }
    delete(bodies^,allocator); bodies^=nil
}
@(private="package")
physics_mesh_rotation :: proc(app:^Authoring,entity:ecs.Entity_Id)->(km.Quat,bool) {
    rotation:=km.QUAT_IDENTITY; cursor:=entity; steps:=0
    for {
        local,present:=ecs.get_component(&app.world,cursor,Scene_Transform); if !present || !km.quat_is_normalized(local.local.rotation) { return {},false }
        rotation=km.quat_mul(local.local.rotation,rotation)
        parent,has_parent:=ecs.get_component(&app.world,cursor,Scene_Parent); if !has_parent { break }
        cursor=parent.entity; steps+=1; if steps>100000 { return {},false }
    }
    return km.quat_normalize(rotation),true
}
@(private="package")
physics_mesh_collect :: proc(app:^Authoring,entity:ecs.Entity_Id,world_matrix:km.Mat4,resolved:^Physics_Resolved_Body)->editor.Scene_Error {
    mesh,present:=ecs.get_component(&app.world,entity,Scene_Mesh); if !present { return .Component_Not_Found }
    if len(mesh.geometry.vertices)<3 || len(mesh.geometry.vertices)>MAX_MESH_VERTICES || len(mesh.geometry.indices)==0 || len(mesh.geometry.indices)%3!=0 || len(mesh.geometry.indices)>MAX_MESH_INDICES { return .Invalid_Field_Value }
    rotation,rotation_ok:=physics_mesh_rotation(app,entity); if !rotation_ok { return .Invalid_Operation }
    resolved.position=km.mat4_extract_translation(world_matrix); resolved.rotation=cast([4]f32)rotation
    rigid:=km.mat4_trs(km.Vec3(resolved.position),rotation,km.VEC3_ONE); inverse,invertible:=km.inverse(rigid); if !invertible { return .Invalid_Operation }
    residual:=km.matrix_mul(inverse,world_matrix); determinant:=km.determinant(residual)
    if !finite_nonnegative(abs(determinant)) || abs(determinant)<0.000000000001 { return .Invalid_Operation }
    resolved.vertices=make([][3]f32,len(mesh.geometry.vertices),resolved.allocator)
    resolved.indices=make([]u32,len(mesh.geometry.indices),resolved.allocator); copy(resolved.indices,mesh.geometry.indices)
    for vertex,i in mesh.geometry.vertices { point:=km.transform_point(residual,vertex.position); if !mesh_vec_finite(point) { return .Invalid_Field_Value }; resolved.vertices[i]=point }
    for index in resolved.indices { if u64(index)>=u64(len(resolved.vertices)) { return .Invalid_Field_Value } }
    for triangle:=0;triangle<len(resolved.indices);triangle+=3 {
        ia,ib,ic:=resolved.indices[triangle],resolved.indices[triangle+1],resolved.indices[triangle+2]
        a,b,c:=km.Vec3(resolved.vertices[ia]),km.Vec3(resolved.vertices[ib]),km.Vec3(resolved.vertices[ic])
        if km.length_squared(km.cross(b-a,c-a))<0.000000000001 { return .Invalid_Field_Value }
        if determinant<0 { resolved.indices[triangle+1],resolved.indices[triangle+2]=resolved.indices[triangle+2],resolved.indices[triangle+1] }
    }
    return .None
}
/// A completed backend pose uses world space and retains complete runtime identity.
Physics_Resolved_Pose :: struct { id:u64,position:[3]f32,rotation:[4]f32,linear_velocity:[3]f32 }
/// Preflights all bodies and scales their shapes through the exact current world hierarchy.
physics_collect :: proc(app:^Authoring)->([]Physics_Resolved_Body,editor.Scene_Error) {
    context.allocator=app.world.allocator; bodies:=make([dynamic]Physics_Resolved_Body,app.world.allocator); transferred:=false
    defer { if !transferred { for body in bodies { delete(body.vertices,body.allocator); delete(body.indices,body.allocator) } }; delete(bodies) }
    ids:=ecs.entity_ids(&app.world); defer delete(ids)
    for entity in ids {
        body,present:=ecs.get_component(&app.world,entity,Physics_Body); if !present { continue }
        if !physics_body_valid(body) { return nil,.Invalid_Field_Value }
        if len(bodies)>=100000 { return nil,.Invalid_Operation }
        world_matrix,matrix_error:=scene_world_matrix(app,entity); if matrix_error!=.None { return nil,matrix_error }
        mesh_kind:=body.shape.kind==.Trimesh || body.shape.kind==.ConvexHull
        pose,representable:=physics_trs(world_matrix)
        if !mesh_kind || body.body_type==.Dynamic {
            if !representable { return nil,.Invalid_Operation }
            for scale in pose.scale { if !finite_nonnegative(scale) || scale<=0 { return nil,.Invalid_Operation } }
        }
        if parent,has_parent:=ecs.get_component(&app.world,entity,Scene_Parent); has_parent && body.body_type==.Dynamic {
            parent_matrix,parent_error:=scene_world_matrix(app,parent.entity); if parent_error!=.None { return nil,parent_error }
            parent_pose,parent_ok:=physics_trs(parent_matrix); if !parent_ok || abs(parent_pose.scale[0]-parent_pose.scale[1])>0.001 || abs(parent_pose.scale[0]-parent_pose.scale[2])>0.001 { return nil,.Invalid_Operation }
        }
        description:=Physics_Resolved_Body{id=u64(entity),body=body,position=pose.position,rotation=cast([4]f32)pose.rotation,allocator=app.world.allocator}
        if mesh_kind {
            mesh_error:=physics_mesh_collect(app,entity,world_matrix,&description)
            if mesh_error!=.None { delete(description.vertices,description.allocator); delete(description.indices,description.allocator); return nil,mesh_error }
        } else {
            switch description.body.shape.kind {
            case .Box: description.body.shape.half_extents*=pose.scale
            case .Sphere: if abs(pose.scale[0]-pose.scale[1])>0.001 || abs(pose.scale[0]-pose.scale[2])>0.001 { return nil,.Invalid_Operation }; description.body.shape.radius*=pose.scale[0]
            case .Capsule: if abs(pose.scale[0]-pose.scale[2])>0.001 { return nil,.Invalid_Operation }; description.body.shape.radius*=pose.scale[0]; description.body.shape.half_height*=pose.scale[1]
            case .Trimesh,.ConvexHull:
            }
        }
        if !physics_body_valid(description.body) { delete(description.vertices,description.allocator); delete(description.indices,description.allocator); return nil,.Invalid_Field_Value }
        append(&bodies,description)
    }
    resolved:=make([]Physics_Resolved_Body,len(bodies),app.world.allocator); copy(resolved,bodies[:]); transferred=true; return resolved,.None
}
/// Validates every completed pose and stages all local updates before any live component write.
physics_commit_poses :: proc(app:^Authoring,poses:[]Physics_Resolved_Pose)->editor.Scene_Error {
    context.allocator=app.world.allocator
    world_poses:=make(map[ecs.Entity_Id]km.Mat4,app.world.allocator); defer delete(world_poses)
    seen:=make(map[ecs.Entity_Id]bool,app.world.allocator); defer delete(seen)
    updates:=make(map[ecs.Entity_Id]Scene_Transform,app.world.allocator); defer delete(updates)
    velocities:=make(map[ecs.Entity_Id][3]f32,app.world.allocator); defer delete(velocities)
    for pose in poses {
        entity:=ecs.Entity_Id(pose.id)
        if !ecs.entity_exists(&app.world,entity) { return .Entity_Not_Found }
        if ecs.get_component_mut(&app.world,entity,Scene_Transform)==nil || ecs.get_component_mut(&app.world,entity,Physics_Body)==nil { return .Component_Not_Found }
        if seen[entity] { return .Invalid_Operation }; seen[entity]=true
        if !km.quat_is_normalized(km.Quat(pose.rotation)) { return .Invalid_Field_Value }
        for vector in ([2][3]f32{pose.position,pose.linear_velocity}) { for value in vector { if math.is_nan(value) || math.is_inf(value) { return .Invalid_Field_Value } } }
        body,_:=ecs.get_component(&app.world,entity,Physics_Body); if body.body_type!=.Dynamic { continue }
        current,current_error:=scene_world_matrix(app,entity); if current_error!=.None { return current_error }; scale:=km.mat4_extract_scale(current)
        world_poses[entity]=km.mat4_trs(km.Vec3(pose.position),km.Quat(pose.rotation),scale); velocities[entity]=pose.linear_velocity
    }
    for entity,world_pose in world_poses {
        local_matrix:=world_pose
        if parent,has_parent:=ecs.get_component(&app.world,entity,Scene_Parent); has_parent {
            parent_matrix,native:=world_poses[parent.entity]
            if !native { parent_error:editor.Scene_Error; parent_matrix,parent_error=scene_world_matrix(app,parent.entity); if parent_error!=.None { return parent_error } }
            parent_inverse,invertible:=km.inverse(parent_matrix); if !invertible { return .Invalid_Operation }; local_matrix=km.matrix_mul(parent_inverse,world_pose)
        }
        local,representable:=physics_trs(local_matrix); if !representable { return .Invalid_Operation }; updates[entity]={local}
    }
    for entity,local in updates { target:=ecs.get_component_mut(&app.world,entity,Scene_Transform); body:=ecs.get_component_mut(&app.world,entity,Physics_Body); target^=local; body.linear_velocity=velocities[entity] }
    return .None
}
/// Synchronizes retained Rapier owners through the common full-scene preflight.
physics_sync :: proc(app:^Authoring)->editor.Scene_Error {
    context.allocator=app.world.allocator; resolved,collect_error:=physics_collect(app); if collect_error!=.None { return collect_error }; defer physics_collected_destroy(&resolved,app.world.allocator)
    bodies:=make([]Physics_Wire_Body,len(resolved),app.world.allocator)
    defer { for body in bodies { delete(body.entity_id); json.destroy_value(body.shape) }; delete(bodies) }
    for description,i in resolved {
        body:=description.body; shape,shape_ok:=physics_shape_wire(description); if !shape_ok { return .Invalid_Operation }
        body_name:="dynamic"; if body.body_type==.Kinematic { body_name="kinematic" } else if body.body_type==.Fixed { body_name="fixed" }
        bodies[i]=Physics_Wire_Body{fmt.aprintf("%d",description.id),body_name,description.position,description.rotation,shape,body.sensor,body.linear_velocity,body.gravity_scale,body.friction,body.restitution,body.density,body.layers,body.mask,body.ccd}
    }
    if len(bodies)==0 && !ecs.contains_resource(&app.world,Scene_Runtime) { return .None }
    response:=scene_runtime_call(app,struct { method:string,bodies:[]Physics_Wire_Body }{"physics_sync",bodies}); defer runtime_response_destroy(&response)
    if !response.ok { return .Invalid_Operation }; return .None
}
/// Steps real Rapier and commits validated world poses to locals before returning overlap events.
physics_rapier_step :: proc(app:^Authoring,delta_seconds:f32)->Physics_Step_Result {
    context.allocator=app.world.allocator; result:=Physics_Step_Result{events=make([dynamic]Physics_Event,app.world.allocator),allocator=app.world.allocator}
    if !finite_nonnegative(delta_seconds) { result.error=.Invalid_Field_Value; return result }
    sync_error:=physics_sync(app); if sync_error!=.None { result.error=sync_error; return result }
    if !ecs.contains_resource(&app.world,Scene_Runtime) || delta_seconds==0 { return result }
    response:=scene_runtime_call(app,struct {method:string,delta_seconds:f32}{"physics_step",delta_seconds}); defer runtime_response_destroy(&response)
    if !response.ok { result.error=.Invalid_Operation; return result }
    bytes,marshal_error:=json.marshal(response.result); if marshal_error!=nil { result.error=.Decode_Failed; return result }; defer delete(bytes)
    output:Physics_Output; decode_error:=json.unmarshal(bytes,&output,allocator=app.world.allocator); defer physics_output_destroy(&output)
    if decode_error!=nil { result.error=.Decode_Failed; return result }
    poses:=make([]Physics_Resolved_Pose,len(output.poses),app.world.allocator); defer delete(poses)
    for pose,i in output.poses { entity,valid:=agent.parse_entity_id(pose.entity_id); if !valid { result.error=.Invalid_Operation; return result }; poses[i]={u64(entity),pose.position,pose.rotation,pose.linear_velocity} }
    for event in output.events {
        trigger,trigger_ok:=agent.parse_entity_id(event.trigger_entity); other,other_ok:=agent.parse_entity_id(event.other_entity)
        if !trigger_ok || !other_ok || (event.phase!="enter" && event.phase!="exit") { result.error=.Invalid_Operation; return result }
        phase:=Trigger_Phase.Enter if event.phase=="enter" else Trigger_Phase.Exit; append(&result.events,Physics_Event{phase,trigger,other})
    }
    result.error=physics_commit_poses(app,poses)
    return result
}
