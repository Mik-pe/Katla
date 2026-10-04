//! Native queries and motion operate exclusively on the dependency's authoritative bodies.
package box3d
import "core:math"
import "core:mem"
import "core:slice"

Ray :: struct { id:u64,point,normal:[3]f32,distance:f32,hit:u32 }
Ray_Result :: struct { ray:Ray,error:Error }
Motion :: enum u32 { Set_Velocity, Force, Impulse }
#assert(size_of(Ray)==40)
/// Uses actual native shape geometry and reciprocal query filters, including solid initial overlap.
backend_raycast :: proc(b:^Backend,origin,direction:[3]f32,distance:f32,layers:u32=max(u32),mask:u32=max(u32),include_sensors:=true)->Ray_Result {
    result:=Ray_Result{error=owner_error(b)}; if result.error!=.None { return result }
    for values in ([2][3]f32{origin,direction}) { for value in values { if !finite(value) || abs(value)>1e8 { result.error=.Invalid; return result } } }
    if !finite(distance) || distance<=0 || distance>1e8 { result.error=.Invalid; return result }
    norm:f32; for value in direction { norm+=value*value }; if !finite(norm) || norm<=0 { result.error=.Invalid; return result }
    unit:=direction/math.sqrt(norm); source:=origin
    if b.raycast(b.instance,&source,&unit,distance,layers,mask,u32(include_sensors),&result.ray)==0 { result.error=.Native }
    return result
}
/// Applies one genuine velocity, force or impulse command to a live native entity.
backend_motion :: proc(b:^Backend,id:u64,kind:Motion,vector:[3]f32)->Error {
    error:=owner_error(b); if error!=.None { return error }
    if kind>Motion.Impulse { return .Invalid }; for value in vector { if !finite(value) || abs(value)>1e8 { return .Invalid } }
    entry,present:=b.entries[id]; if !present { return .Invalid }; value:=vector
    if b.motion(entry.native,u32(kind),&value)==0 { return .Native }; return .None
}
/// Copies the last completed directed sensor overlaps after checking the native owner's thread.
backend_trigger_overlaps :: proc(b:^Backend,id:u64)->([]u64,Error) {
    if error:=owner_error(b); error!=.None { return nil,error }
    entry,present:=b.entries[id]; if !present || entry.body.sensor==0 { return nil,.Invalid }
    values:=make([dynamic]u64,b.allocator); defer delete(values)
    for pair in b.overlaps { if pair.trigger==id { append(&values,pair.other) } }
    slice.sort(values[:]); return slice.clone(values[:],b.allocator),.None
}
/// Copies authoritative motion after a host impulse or velocity command without advancing simulation.
backend_pose :: proc(b:^Backend,id:u64)->(Pose,Error) {
    if error:=owner_error(b); error!=.None { return {},error }
    entry,present:=b.entries[id]; if !present { return {},.Invalid }
    result:Pose; b.pose(entry.native,&result); return result,.None
}

/// Casts an exact rotated collider along direction * time; distance is the impact parameter and point is the moving origin.
backend_shape_cast :: proc(b:^Backend,shape:Body,direction:[3]f32,distance:f32,layers:u32=max(u32),mask:u32=max(u32),include_sensors:=true)->Ray_Result {
    result:=Ray_Result{error=owner_error(b)}; if result.error!=.None { return result }
    if !body_valid(shape) || shape.shape_kind==.None || !finite(distance) || distance<=0 || distance>1e8 { result.error=.Invalid; return result }
    norm:f32; for value in direction { if !finite(value) || abs(value)>1e8 { result.error=.Invalid; return result }; norm+=value*value }
    if !finite(norm) || norm<=0 { result.error=.Invalid; return result }; translation:=direction*distance
    for value in translation { if !finite(value) || abs(value)>1e8 { result.error=.Invalid; return result } }; vector:=direction; description:=shape
    if b.shape_query(b.instance,&description,&vector,distance,layers,mask,u32(include_sensors),&result.ray,nil,0)<0 { result.error=.Native }; return result
}
/// Returns sorted unique IDs of exact narrow-phase overlaps, owned by the supplied allocator.
backend_shape_overlaps :: proc(b:^Backend,shape:Body,layers:u32=max(u32),mask:u32=max(u32),include_sensors:=true,allocator:mem.Allocator=context.allocator)->([]u64,Error) {
    if error:=owner_error(b); error!=.None { return nil,error }
    if !body_valid(shape) || shape.shape_kind==.None { return nil,.Invalid }
    values:=make([]u64,len(b.entries),allocator); description:=shape; direction:[3]f32
    count:=b.shape_query(b.instance,&description,&direction,0,layers,mask,u32(include_sensors),nil,raw_data(values),i32(len(values)))
    if count<0 || int(count)>len(values) { delete(values,allocator); return nil,.Native }
    slice.sort(values[:int(count)]); result:=slice.clone(values[:int(count)],allocator); delete(values,allocator); return result,.None
}

/// One completed solver contact; normal points from the smaller ID to the larger ID.
Contact :: struct { a,b:u64,point,normal:[3]f32,separation,normal_impulse:f32 }
#assert(size_of(Contact)==48)
/// Copies actual touching manifolds and all their points before native data can change.
backend_contacts :: proc(b:^Backend,allocator:mem.Allocator=context.allocator)->([]Contact,Error) {
    if error:=owner_error(b); error!=.None { return nil,error }
    values:=make([dynamic]Contact,allocator); defer delete(values)
    for _,entry in b.entries {
        count:=b.body_contacts(entry.native,nil,0)
        if count<0 { return nil,.Native }; if count>1_000_000 || len(values)+int(count)>1_000_000 { return nil,.Budget }
        start:=len(values); resize(&values,start+int(count))
        if count>0 && b.body_contacts(entry.native,raw_data(values[start:]),count)!=count { return nil,.Native }
    }
    slice.sort_by(values[:],proc(a,b:Contact)->bool { if a.a!=b.a { return a.a<b.a }; if a.b!=b.b { return a.b<b.b }; for i in 0..<3 { if a.point[i]!=b.point[i] { return a.point[i]<b.point[i] } }; return a.separation<b.separation })
    return slice.clone(values[:],allocator),.None
}

/// One exact edge of the native retained convex collider in world coordinates.
Edge :: struct { start,end:[3]f32 }
#assert(size_of(Edge)==24)
/// Copies actual native convex-hull topology, without substituting source mesh faces.
backend_hull_edges :: proc(b:^Backend,id:u64,allocator:mem.Allocator=context.allocator)->([]Edge,Error) {
    if error:=owner_error(b); error!=.None { return nil,error }
    entry,present:=b.entries[id]; if !present { return nil,.Invalid }; if entry.body.shape_kind!=.ConvexHull { return nil,.Unsupported }
    count:=b.hull_edges(entry.native,nil,0); if count<0 || count>1_000_000 { return nil,.Native }
    edges:=make([]Edge,int(count),allocator)
    if count>0 && b.hull_edges(entry.native,raw_data(edges),count)!=count { delete(edges,allocator); return nil,.Native }; return edges,.None
}
