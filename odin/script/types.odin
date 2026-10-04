//! Script instances use immutable owned snapshots and defer every host mutation.
package script
import luau "../deps/luau"
import km "../math"
import "core:mem"
import "core:strings"
import "base:runtime"

Entity_State :: struct { id:u64,name:string,transform:km.Transform,velocity:km.Vec3,has_velocity:bool,components:[]string,without_transform:bool }
Input :: struct { actions,keys:[]string,mouse_delta:[2]f32,mouse_wheel:f32 }
Ray_Result :: struct { hit:bool,entity:u64,point,normal:km.Vec3,distance:f32 }
Query_Key :: struct { owner:u64,index:int }
Query_Results :: struct { rays:map[Query_Key]Ray_Result,overlaps:map[Query_Key][]u64 }
Command_Kind :: enum { Set_Transform, Set_Position, Spawn_Entity, Destroy_Entity, Burst_Particles, Set_Particles_Active, Emit, Play_Sound, Play_Sound_At, Play_Sound_Cue, Raycast, Apply_Force, Apply_Impulse, Set_Velocity, Query_Trigger_Overlaps }
/// Queue order is observable; query indices address results from the next host tick.
Command :: struct { kind:Command_Kind,owner,entity:u64,transform:km.Transform,vector,origin:km.Vec3,count:u32,active,looping:bool,name,path:string,volume,max_distance:f32,index:int,payload:i32 }
/// Host animation fields are borrowed for tick; delivery copies them into VM-owned packets.
Event :: struct { name:string,trigger,other:u64,payload:i32,animation_clip:string,animation_loop_count:u32,has_animation:bool }
Diagnostic :: struct { entity:u64,path,error:string }
Instance_State :: struct { entity:u64,spawned,disabled:bool,consecutive_errors:u32 }
Log_Level :: enum { Info, Warn }
Log :: struct { entity:u64,level:Log_Level,message:string }
/// Commands and diagnostics own strings; destroy before unloading their runtime.
Output :: struct { commands:[dynamic]Command,diagnostics:[dynamic]Diagnostic,instances:[dynamic]Instance_State,logs:[dynamic]Log,allocator:mem.Allocator }
Attachment :: struct { entity:u64,path,source:string }
@(private="package")
Subscription :: struct { name:string,callback:i32 }
@(private="package")
Instance :: struct { id,serial:u64,environment:i32,path,source:string,spawned,disabled:bool,errors:u32,subscriptions:[dynamic]Subscription }
@(private="package")
Snapshot :: struct { references:int,entities:map[u64]Entity_State,input:Input,queries:Query_Results,allocator:mem.Allocator }
@(private="package")
World_Proxy :: struct { owner:^Runtime,snapshot:^Snapshot,entity,instance_serial,tick_serial:u64 }
@(private="package")
Binding :: struct { owner:^Runtime,name:string }
/// Stationary VM owner. The host must destroy output and call destroy on the creating thread.
Runtime :: struct {
    vm:luau.VM,allocator:mem.Allocator,ctx:runtime.Context,
    instances:map[u64]^Instance,next_serial,tick_serial:u64,
    metatables:[12]i32,bindings:[128]Binding,binding_count:int,
    commands:[dynamic]Command,deferred_events:[dynamic]Event,
    logs:[dynamic]Log,current_entity:u64,
}
@(private="package")
command_destroy :: proc(owner:^Runtime,command:^Command) { delete(command.name); delete(command.path); if command.kind==.Emit && command.payload>0 { owner.vm.api.unreference(owner.vm.state,command.payload) }; command^={} }
/// Releases returned commands and diagnostics with their captured allocator.
output_destroy :: proc(owner:^Runtime,output:^Output) {
    context.allocator=output.allocator
    for &command in output.commands { command_destroy(owner,&command) }
    for diagnostic in output.diagnostics { delete(diagnostic.path); delete(diagnostic.error) }
    for entry in output.logs { delete(entry.message) }
    delete(output.commands); delete(output.diagnostics); delete(output.instances); delete(output.logs); output^={}
}
@(private="package")
snapshot_release :: proc(snapshot:^Snapshot) {
    snapshot.references-=1; if snapshot.references!=0 { return }; context.allocator=snapshot.allocator
    for _,entity in snapshot.entities { delete(entity.name); for name in entity.components { delete(name) }; delete(entity.components) }; delete(snapshot.entities)
    for name in snapshot.input.actions { delete(name) }; for name in snapshot.input.keys { delete(name) }; delete(snapshot.input.actions); delete(snapshot.input.keys)
    for _,entities in snapshot.queries.overlaps { delete(entities) }; delete(snapshot.queries.rays); delete(snapshot.queries.overlaps); free(snapshot)
}
@(private="package")
clone_names :: proc(source:[]string,allocator:mem.Allocator)->[]string { result:=make([]string,len(source),allocator); for name,i in source { result[i]=strings.clone(name,allocator) }; return result }
@(private="package")
snapshot_create :: proc(owner:^Runtime,entities:[]Entity_State,input:Input,queries:Query_Results)->^Snapshot {
    result:=new(Snapshot,owner.allocator); result.references=1; result.allocator=owner.allocator; result.entities=make(map[u64]Entity_State,owner.allocator)
    for entity in entities { copy:=entity; copy.name=strings.clone(entity.name,owner.allocator); copy.components=clone_names(entity.components,owner.allocator); result.entities[entity.id]=copy }
    result.input=input; result.input.actions=clone_names(input.actions,owner.allocator); result.input.keys=clone_names(input.keys,owner.allocator)
    result.queries.rays=make(map[Query_Key]Ray_Result,owner.allocator); for key,ray in queries.rays { result.queries.rays[key]=ray }
    result.queries.overlaps=make(map[Query_Key][]u64,owner.allocator); for key,overlaps in queries.overlaps { values:=make([]u64,len(overlaps),owner.allocator); copy(values,overlaps); result.queries.overlaps[key]=values }
    return result
}
