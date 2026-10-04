//! Scene-owned query results expose lossless entities without dependency handles.
package app
import ecs "../ecs"
import box3d "../physics/box3d"
import editor "../editor"
import km "../math"

Physics_Raycast_Result :: struct { entity:ecs.Entity_Id,point,normal:km.Vec3,distance:f32,hit:bool,error:editor.Scene_Error }
/// Casts against the prepared native world. Sensor inclusion is explicit for host policy.
physics_raycast :: proc(app:^Authoring,origin,direction:km.Vec3,max_distance:f32,include_sensors:=true)->Physics_Raycast_Result {
    result:Physics_Raycast_Result; backend:=ecs.get_resource_mut(&app.world,box3d.Backend)
    if backend==nil { result.error=.Application_Owned; return result }
    native:=box3d.backend_raycast(backend,origin,direction,max_distance,include_sensors=include_sensors)
    if native.error!=.None { result.error=.Invalid_Operation; return result }
    result.hit=native.ray.hit!=0; result.entity=ecs.Entity_Id(native.ray.id); result.point=native.ray.point; result.normal=native.ray.normal; result.distance=native.ray.distance; return result
}
/// Applies force or impulse on the app thread, retaining the native body's completed state.
physics_apply_motion :: proc(app:^Authoring,entity:ecs.Entity_Id,motion:box3d.Motion,vector:km.Vec3)->editor.Scene_Error {
    backend:=ecs.get_resource_mut(&app.world,box3d.Backend); if backend==nil { return .Application_Owned }
    if !ecs.entity_exists(&app.world,entity) { return .Entity_Not_Found }
    if box3d.backend_motion(backend,u64(entity),motion,vector)!=.None { return .Invalid_Operation }
    pose,error:=box3d.backend_pose(backend,u64(entity)); if error!=.None { return .Invalid_Operation }
    if body:=ecs.get_component_mut(&app.world,entity,Physics_Body); body!=nil { body.linear_velocity=pose.linear_velocity }
    return .None
}

/// Borrowed local geometry for an exact world-space query; mesh streams never enter the ECS.
Physics_Query_Shape :: struct { shape:Physics_Shape,origin:km.Vec3,rotation:km.Quat,scale:km.Vec3,vertices:[][3]f32,indices:[]u32 }
/// Creates a query with conventional identity rotation and positive unit scale.
physics_query_shape :: proc(shape:Physics_Shape,origin:km.Vec3={},rotation:=km.QUAT_IDENTITY,scale:=km.VEC3_ONE,vertices:[][3]f32=nil,indices:[]u32=nil)->Physics_Query_Shape { return {shape,origin,rotation,scale,vertices,indices} }
@(private="package")
physics_native_query :: proc(query:Physics_Query_Shape)->(box3d.Body,[][3]f32,editor.Scene_Error) {
    shape:=query.shape
    body:=box3d.Body{body_type=.Fixed,shape_kind=cast(box3d.Shape_Kind)shape.kind,position=query.origin,rotation=cast([4]f32)query.rotation,density=1,layers=max(u32),mask=max(u32),half_extents=shape.half_extents,radius=shape.radius,half_height=shape.half_height}
    if !physics_body_valid(physics_body(shape)) || !km.quat_is_normalized(query.rotation) { return {},nil,.Invalid_Field_Value }
    for scale in query.scale { if !finite_nonnegative(scale) || scale<=0 { return {},nil,.Invalid_Field_Value } }
    if shape.kind!=.Trimesh && shape.kind!=.ConvexHull && (len(query.vertices)!=0 || len(query.indices)!=0) { return {},nil,.Invalid_Field_Value }
    points:[][3]f32
    switch shape.kind {
    case .Box: body.half_extents*=query.scale
    case .Sphere:
        if abs(query.scale[0]-query.scale[1])>0.001 || abs(query.scale[0]-query.scale[2])>0.001 { return {},nil,.Invalid_Field_Value }; body.radius*=query.scale[0]
    case .Capsule:
        if abs(query.scale[0]-query.scale[2])>0.001 { return {},nil,.Invalid_Field_Value }; body.radius*=query.scale[0]; body.half_height*=query.scale[1]
    case .Heightfield:
        body.heights=raw_data(shape.heights); body.rows=shape.rows; body.cols=shape.cols
        body.height_scale={f32(shape.cols)*query.scale[0]/f32(shape.cols-1),query.scale[1],f32(shape.rows)*query.scale[2]/f32(shape.rows-1)}
    case .Trimesh,.ConvexHull:
        if len(query.vertices)<3 || len(query.vertices)>1_000_000 || len(query.indices)>3_000_000 { return {},nil,.Invalid_Field_Value }
        points=make([][3]f32,len(query.vertices)); for point,i in query.vertices { points[i]=point*query.scale }
        body.vertices=raw_data(points); body.vertex_count=u32(len(points))
        if shape.kind==.Trimesh { body.indices=raw_data(query.indices); body.index_count=u32(len(query.indices)) }
    case .None: return {},nil,.Invalid_Field_Value
    }
    if !box3d.body_valid(body) { delete(points); return {},nil,.Invalid_Field_Value }; return body,points,.None
}
/// Matches the original shape-cast parameter: origin + direction * distance, with distance bounded by max_distance.
physics_shape_cast :: proc(app:^Authoring,query:Physics_Query_Shape,direction:km.Vec3,max_distance:f32,include_sensors:=true,layers:u32=max(u32),mask:u32=max(u32))->Physics_Raycast_Result {
    result:Physics_Raycast_Result; backend:=ecs.get_resource_mut(&app.world,box3d.Backend)
    if backend==nil { result.error=.Application_Owned; return result }
    context.allocator=app.world.allocator
    description,points,error:=physics_native_query(query); defer delete(points)
    if error!=.None { result.error=error; return result }
    native:=box3d.backend_shape_cast(backend,description,direction,max_distance,layers,mask,include_sensors)
    if native.error!=.None { result.error=.Invalid_Operation; return result }
    result.hit=native.ray.hit!=0; result.entity=ecs.Entity_Id(native.ray.id); result.point=native.ray.point; result.normal=native.ray.normal; result.distance=native.ray.distance; return result
}
/// Returns sorted unique generational scene IDs from the native narrow phase; the caller releases the slice.
physics_shape_overlaps :: proc(app:^Authoring,query:Physics_Query_Shape,include_sensors:=true,layers:u32=max(u32),mask:u32=max(u32))->([]ecs.Entity_Id,editor.Scene_Error) {
    backend:=ecs.get_resource_mut(&app.world,box3d.Backend); if backend==nil { return nil,.Application_Owned }; context.allocator=app.world.allocator
    description,points,error:=physics_native_query(query); defer delete(points); if error!=.None { return nil,error }
    ids,native_error:=box3d.backend_shape_overlaps(backend,description,layers,mask,include_sensors,app.world.allocator); defer delete(ids)
    if native_error!=.None { return nil,.Invalid_Operation }
    result:=make([]ecs.Entity_Id,len(ids),app.world.allocator); for id,i in ids { result[i]=ecs.Entity_Id(id) }; return result,.None
}

/// A completed native solver point; the unit normal is oriented from a to b, with a < b.
Physics_Contact :: struct { a,b:ecs.Entity_Id,point,normal:km.Vec3,separation,normal_impulse:f32 }
/// Returns owned actual touching contact points for app diagnostics after the last native step.
physics_contacts :: proc(app:^Authoring)->([]Physics_Contact,editor.Scene_Error) {
    backend:=ecs.get_resource_mut(&app.world,box3d.Backend); if backend==nil { return nil,.Application_Owned }
    contacts,error:=box3d.backend_contacts(backend,app.world.allocator); defer delete(contacts,app.world.allocator)
    if error!=.None { return nil,.Invalid_Operation }
    result:=make([]Physics_Contact,len(contacts),app.world.allocator)
    for contact,i in contacts { result[i]={ecs.Entity_Id(contact.a),ecs.Entity_Id(contact.b),contact.point,contact.normal,contact.separation,contact.normal_impulse} }
    return result,.None
}

Physics_Edge :: box3d.Edge
/// Copies the live convex collider's actual world-space topology for application diagnostics.
physics_collider_edges :: proc(app:^Authoring,entity:ecs.Entity_Id)->([]Physics_Edge,editor.Scene_Error) {
    backend:=ecs.get_resource_mut(&app.world,box3d.Backend); if backend==nil { return nil,.Application_Owned }
    if !ecs.entity_exists(&app.world,entity) { return nil,.Entity_Not_Found }
    edges,error:=box3d.backend_hull_edges(backend,u64(entity),app.world.allocator)
    if error!=.None { return nil,.Invalid_Operation }; return edges,.None
}
