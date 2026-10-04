//! Native Box3D owners retain no ECS pointers; application composition stays in Odin.
package box3d

import "core:dynlib"
import "core:mem"
import "core:math"
import "core:slice"
import "core:sync"

Error :: enum { None, Invalid, Budget, Library, ABI, Native, Wrong_Thread, Uninitialized }
Body_Type :: enum u32 { Dynamic, Kinematic, Fixed }
Shape_Kind :: enum u32 { Box, Sphere, Capsule, Trimesh, ConvexHull }
/// One complete world-space body description; unchanged sync preserves native motion.
Body :: struct {
    id:u64, body_type:Body_Type, shape_kind:Shape_Kind,
    position:[3]f32,rotation:[4]f32,linear_velocity:[3]f32,half_extents:[3]f32,
    radius,half_height,gravity_scale,friction,restitution:f32,layers,mask:u32,sensor,ccd:u32,
    density:f32,vertices:[^][3]f32,indices:[^]u32,vertex_count,index_count:u32,
}
/// Completed native pose; IDs remain lossless u64 values until transport encoding.
Pose :: struct { id:u64,position:[3]f32,rotation:[4]f32,linear_velocity:[3]f32 }
Pair :: struct { trigger,other:u64 }
Phase :: enum { Enter, Exit }
Event :: struct { pair:Pair,phase:Phase }
/// Owns one completed step's sorted poses, directed transitions and current overlaps.
Step :: struct { poses:[dynamic]Pose,events:[dynamic]Event,overlaps:[dynamic]Pair,error:Error,allocator:mem.Allocator }
@(private="package")
Entry :: struct { body:Body,native:rawptr }
/// Stationary owner; every native call runs exclusively on its creating thread.
Backend :: struct {
    library:dynlib.Library,instance:rawptr,owner_thread:int,allocator:mem.Allocator,
    entries:map[u64]Entry,overlaps:map[Pair]bool,
    destroy:proc "c"(rawptr),create_body:proc "c"(rawptr,^Body)->rawptr,
    destroy_body:proc "c"(rawptr),update_body:proc "c"(rawptr,^Body),
    step:proc "c"(rawptr,f32),pose:proc "c"(rawptr,^Pose),
    overlap_ids:proc "c"(rawptr,[^]u64,i32)->i32,native_bytes:proc "c"()->i64,
}
#assert(size_of(Body)==136 && size_of(Pose)==48)
/// Releases owned feedback with the allocator captured at its creation.
step_destroy :: proc(step:^Step) { delete(step.poses); delete(step.events); delete(step.overlaps); step^={} }
@(private="package")
owner_error :: proc(b:^Backend)->Error {
    if b.instance==nil { return .Uninitialized }
    if b.owner_thread!=sync.current_thread_id() { return .Wrong_Thread }
    return .None
}
/// Loads the complete revision-two single-precision ABI before creating a native owner.
backend_init :: proc(b:^Backend,path:string,allocator:=context.allocator)->Error {
    if b.instance!=nil { return .Invalid }
    b.allocator=allocator; b.owner_thread=sync.current_thread_id()
    loaded:bool; b.library,loaded=dynlib.load_library(path,allocator=allocator)
    if !loaded { b^={}; return .Library }
    success:=false; defer { if !success { dynlib.unload_library(b.library); b^={} } }
    names:=[10]string{"katla_box3d_abi","katla_box3d_create","katla_box3d_destroy","katla_box3d_body_create","katla_box3d_body_destroy","katla_box3d_body_update","katla_box3d_step","katla_box3d_pose","katla_box3d_overlaps","katla_box3d_bytes"}
    addresses:[10]rawptr
    for name,i in names { found:bool; addresses[i],found=dynlib.symbol_address(b.library,name,allocator=allocator); if !found { return .ABI } }
    abi:=cast(proc "c"()->u32)addresses[0]; if abi()!=2 { return .ABI }
    create:=cast(proc "c"()->rawptr)addresses[1]
    b.destroy=cast(proc "c"(rawptr))addresses[2]; b.create_body=cast(proc "c"(rawptr,^Body)->rawptr)addresses[3]
    b.destroy_body=cast(proc "c"(rawptr))addresses[4]; b.update_body=cast(proc "c"(rawptr,^Body))addresses[5]
    b.step=cast(proc "c"(rawptr,f32))addresses[6]; b.pose=cast(proc "c"(rawptr,^Pose))addresses[7]
    b.overlap_ids=cast(proc "c"(rawptr,[^]u64,i32)->i32)addresses[8]; b.native_bytes=cast(proc "c"()->i64)addresses[9]
    b.instance=create(); if b.instance==nil { return .Native }
    b.entries=make(map[u64]Entry,allocator); b.overlaps=make(map[Pair]bool,allocator)
    success=true; return .None
}
/// Destroys body wrappers before the world and unloads only after native storage is released.
backend_destroy :: proc(b:^Backend)->Error {
    err:=owner_error(b); if err!=.None { return err }
    for _,entry in b.entries { b.destroy_body(entry.native); body_destroy(entry.body,b.allocator) }
    b.destroy(b.instance); delete(b.entries); delete(b.overlaps); dynlib.unload_library(b.library); b^={}; return .None
}
/// Removes all native bodies and transition membership without replacing the dependency owner.
backend_reset :: proc(b:^Backend)->Error {
    err:=owner_error(b); if err!=.None { return err }
    for _,entry in b.entries { b.destroy_body(entry.native); body_destroy(entry.body,b.allocator) }
    clear(&b.entries); clear(&b.overlaps); return .None
}
/// Validates the complete batch before applying authored changes or removing missing bodies.
backend_sync :: proc(b:^Backend,bodies:[]Body)->Error {
    err:=owner_error(b); if err!=.None { return err }
    if len(bodies)>100_000 { return .Budget }
    context.allocator=b.allocator
    staged:=make(map[u64]Body,b.allocator); defer delete(staged)
    vertices,indices:u64
    for body in bodies {
        if !body_valid(body) { return .Invalid }
        vertices+=u64(body.vertex_count); indices+=u64(body.index_count)
        if vertices>1_000_000 || indices>3_000_000 { return .Budget }
        if _,exists:=staged[body.id]; exists { return .Invalid }
        staged[body.id]=body
    }
    created:=make(map[u64]Entry,b.allocator); defer delete(created)
    success:=false; defer { if !success { for _,entry in created { b.destroy_body(entry.native); body_destroy(entry.body,b.allocator) } } }
    for id,body in staged {
        native_body:=body
        if old,exists:=b.entries[id]; exists {
            if geometry_equal(old.body,body) { continue }
            pose:Pose; b.pose(old.native,&pose)
            if old.body.position==body.position && old.body.rotation==body.rotation { native_body.position=pose.position; native_body.rotation=pose.rotation }
            if old.body.linear_velocity==body.linear_velocity { native_body.linear_velocity=pose.linear_velocity }
        }
        native:=b.create_body(b.instance,&native_body); if native==nil { return .Native }
        created[id]={body_clone(body,b.allocator),native}
    }
    removed:=make([dynamic]u64,b.allocator); defer delete(removed)
    for id,entry in b.entries { if _,exists:=staged[id]; !exists { b.destroy_body(entry.native); body_destroy(entry.body,b.allocator); append(&removed,id) } }
    for id in removed { delete_key(&b.entries,id) }
    for id,body in staged {
        if entry,exists:=created[id]; exists {
            if old,was:=b.entries[id]; was { b.destroy_body(old.native); body_destroy(old.body,b.allocator) }
            b.entries[id]=entry; continue
        }
        entry:=b.entries[id]
        if !body_equal(entry.body,body) {
            owned:=body; b.update_body(entry.native,&owned)
            body_destroy(entry.body,b.allocator); entry.body=body_clone(body,b.allocator); b.entries[id]=entry
        }
    }
    success=true; return .None
}
/// Executes actual Box3D integration and derives directed sensor transitions from native overlaps.
backend_step :: proc(b:^Backend,delta:f32)->Step {
    result:=Step{allocator=b.allocator}; result.error=owner_error(b)
    if result.error!=.None { return result }
    if !finite(delta) || delta<=0 || delta>0.25 { result.error=.Invalid; return result }
    context.allocator=b.allocator
    result.poses=make([dynamic]Pose,b.allocator); result.events=make([dynamic]Event,b.allocator); result.overlaps=make([dynamic]Pair,b.allocator)
    b.step(b.instance,delta)
    current:=make(map[Pair]bool,b.allocator); defer delete(current)
    visitors:=make([dynamic]u64,b.allocator); defer delete(visitors)
    for id,entry in b.entries {
        pose:Pose; b.pose(entry.native,&pose); append(&result.poses,pose)
        if entry.body.sensor==0 { continue }
        count:=b.overlap_ids(entry.native,nil,0)
        if count<0 || count>100_000 || len(current)+int(count)>1_000_000 { result.error=.Budget; return result }
        resize(&visitors,int(count)); written:=b.overlap_ids(entry.native,raw_data(visitors),count)
        if written<0 || written>count { result.error=.Native; return result }
        for other in visitors[:written] { current[{id,other}]=true }
    }
    for pair,_ in current {
        append(&result.overlaps,pair)
        if _,was:=b.overlaps[pair]; !was { append(&result.events,Event{pair,.Enter}) }
    }
    for pair,_ in b.overlaps { if _,now:=current[pair]; !now { append(&result.events,Event{pair,.Exit}) } }
    slice.sort_by(result.poses[:],proc(a,c:Pose)->bool { return a.id<c.id })
    slice.sort_by(result.overlaps[:],proc(a,c:Pair)->bool { return a.trigger<c.trigger || (a.trigger==c.trigger && a.other<c.other) })
    slice.sort_by(result.events[:],proc(a,c:Event)->bool { return a.pair.trigger<c.pair.trigger || (a.pair.trigger==c.pair.trigger && (a.pair.other<c.pair.other || (a.pair.other==c.pair.other && a.phase<c.phase))) })
    delete(b.overlaps); b.overlaps=current; current=nil
    return result
}
/// Rejects malformed enums, nonfinite values and invalid collider dimensions before native admission.
body_valid :: proc(body:Body)->bool {
    if body.body_type>Body_Type.Fixed || body.shape_kind>Shape_Kind.ConvexHull || body.sensor>1 || body.ccd>1 { return false }
    if body.shape_kind<.Trimesh && (body.vertex_count!=0 || body.index_count!=0 || body.vertices!=nil || body.indices!=nil) { return false }
    if body.shape_kind==.ConvexHull && (body.index_count!=0 || body.indices!=nil) { return false }
    owned:=body
    vectors:=[4][]f32{owned.position[:],owned.rotation[:],owned.linear_velocity[:],owned.half_extents[:]}
    for values in vectors { for value in values { if !finite(value) || abs(value)>1e8 { return false } } }
    factors:=[6]f32{body.radius,body.half_height,body.gravity_scale,body.friction,body.restitution,body.density}
    for value in factors { if !finite(value) || abs(value)>1e8 { return false } }
    norm:f32; for value in body.rotation { norm+=value*value }
    if abs(norm-1)>1e-4 || body.density<0 || body.friction<0 || body.restitution<0 || body.restitution>1 { return false }
    switch body.shape_kind {
    case .Box: for half in body.half_extents { if half<=0 { return false } }
    case .Sphere: if body.radius<=0 { return false }
    case .Capsule: if body.radius<=0 || body.half_height<0 { return false }
    case .Trimesh,.ConvexHull:
        if body.vertex_count<3 || body.vertex_count>1_000_000 || body.vertices==nil { return false }
        for point in body.vertices[:body.vertex_count] { for value in point { if !finite(value) || abs(value)>1e8 { return false } } }
        if body.shape_kind==.Trimesh {
            if body.body_type!=.Fixed || body.index_count==0 || body.index_count>3_000_000 || body.index_count%3!=0 || body.indices==nil { return false }
            for index in body.indices[:body.index_count] { if index>=body.vertex_count { return false } }
            for i:=0;i<int(body.index_count);i+=3 { if body.indices[i]==body.indices[i+1] || body.indices[i]==body.indices[i+2] || body.indices[i+1]==body.indices[i+2] { return false } }
        } else if body.vertex_count<4 || body.vertex_count>=255 { return false }
    }
    return true
}
@(private="package")
finite :: proc(value:f32)->bool { return !math.is_nan(value) && !math.is_inf(value) }
