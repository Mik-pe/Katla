//! Host access reads a captured world image and writes bounded, ordered commands.
package script
import luau "../deps/luau"
import km "../math"
import "core:strings"
import "core:fmt"
import "core:slice"
import "core:math"

@(private="package")
push_entity :: proc(owner:^Runtime,id:u64) { push_typed(owner,id,10) }
@(private="file")
entity_callback :: proc "c"(state:luau.State,opaque:rawptr)->i32 {
    binding:=cast(^Binding)opaque; owner:=binding.owner; context=owner.ctx
    entity:=cast(^u64)userdata(owner,1,10); if entity==nil { return binding_error(owner,"Entity userdata required") }
    if binding.name=="__eq" { other:=cast(^u64)userdata(owner,2,10); owner.vm.api.push_boolean(state,i32(other!=nil && entity^==other^)); return 1 }
    text:=fmt.aprintf("%d",entity^); luau.push_string(&owner.vm,text); delete(text); return 1
}
@(private="file")
proxy_destroy :: proc "c"(opaque:rawptr) { proxy:=cast(^World_Proxy)opaque; context=proxy.owner.ctx; snapshot_release(proxy.snapshot) }
@(private="package")
push_proxy :: proc(owner:^Runtime,snapshot:^Snapshot,instance:^Instance) {
    value:=cast(^World_Proxy)owner.vm.api.new_userdata_destroy(owner.vm.state,uint(size_of(World_Proxy)),proxy_destroy)
    if value==nil { return }; snapshot.references+=1
    value^={owner,snapshot,instance.id,instance.serial,owner.tick_serial}; luau.get_reference(&owner.vm,owner.metatables[11]); owner.vm.api.set_metatable(owner.vm.state,-2)
}
@(private="package")
finite_vector :: proc(values:km.Vec3)->bool { for value in values { if math.is_nan(value) || math.is_inf(value) { return false } }; return true }
@(private="file")
world_callback :: proc "c"(state:luau.State,opaque:rawptr)->i32 {
    binding:=cast(^Binding)opaque; owner:=binding.owner; context=owner.ctx; vm:=&owner.vm; api:=vm.api
    if api.type(state,1)!=.Userdata || api.userdata_tag(state,1)!=128 { return binding_error(owner,"Script world proxy required") }
    proxy:=cast(^World_Proxy)api.userdata(state,1); instance,present:=owner.instances[proxy.entity]
    if !present || instance.serial!=proxy.instance_serial || owner.tick_serial!=proxy.tick_serial { return binding_error(owner,"Script world proxy expired") }
    op:=binding.name; snapshot:=proxy.snapshot
    switch op {
    case "find_entity":
        name:=luau.to_string(vm,2); result:u64; found:=false
        for id,entity in snapshot.entities { if entity.name==name && (!found || id<result) { result=id; found=true } }
        if found { push_entity(owner,result) } else { api.push_nil(state) }; return 1
    case "get_all_with":
        name:=luau.to_string(vm,2); ids:=make([dynamic]u64,owner.allocator); defer delete(ids)
        for id,entity in snapshot.entities { for component in entity.components { if component==name { append(&ids,id); break } } }; slice.sort(ids[:])
        api.create_table(state,i32(len(ids)),0); for id,i in ids { push_entity(owner,id); api.raw_set_i(state,-2,i32(i+1)) }; return 1
    case "is_action_pressed","is_key_pressed":
        name:=luau.to_string(vm,2); values:=snapshot.input.actions; if op=="is_key_pressed" { values=snapshot.input.keys }; found:=false; for value in values { if value==name { found=true; break } }; api.push_boolean(state,i32(found)); return 1
    case "get_mouse_delta": api.push_number(state,f64(snapshot.input.mouse_delta[0])); api.push_number(state,f64(snapshot.input.mouse_delta[1])); return 2
    case "get_mouse_wheel": api.push_number(state,f64(snapshot.input.mouse_wheel)); return 1
    case "on_event":
        name:=luau.to_string(vm,2); if len(strings.trim_space(name))==0 || len(name)>128 || api.type(state,3)!=.Function { return binding_error(owner,"Event subscription requires a name and function") }
        if len(instance.subscriptions)>=256 { return binding_error(owner,"Subscription capacity exhausted") }
        append(&instance.subscriptions,Subscription{strings.clone(name,owner.allocator),api.reference(state,3)}); return 0
    case "get_raycast_result","get_trigger_overlaps":
        value,valid:=number(owner,2); if !valid || value<0 || value>=4096 || value!=math.floor(value) { return binding_error(owner,"Query result index requires bounded integer") }; key:=Query_Key{proxy.entity,int(value)}
        if op=="get_raycast_result" {
            ray,found:=snapshot.queries.rays[key]; if !found || !ray.hit { api.push_nil(state); return 1 }
            api.create_table(state,0,4); push_entity(owner,ray.entity); api.set_field(state,-2,"entity"); push_typed(owner,ray.point,1); api.set_field(state,-2,"point"); push_typed(owner,ray.normal,1); api.set_field(state,-2,"normal"); api.push_number(state,f64(ray.distance)); api.set_field(state,-2,"distance"); return 1
        }
        entities,found:=snapshot.queries.overlaps[key]; if !found { api.push_nil(state); return 1 }; api.create_table(state,i32(len(entities)),0); for entity,i in entities { push_entity(owner,entity); api.raw_set_i(state,-2,i32(i+1)) }; return 1
    }
    command:=Command{owner=proxy.entity,index=len(owner.commands),payload=-1}
    switch op {
    case "spawn_entity": command.kind=.Spawn_Entity
    case "emit":
        name:=luau.to_string(vm,2); if len(strings.trim_space(name))==0 || len(name)>128 { return binding_error(owner,"Event name requires 1..128 bytes") }
        command.kind=.Emit; command.name=strings.clone(name,owner.allocator)
        if api.get_top(state)>=3 { command.payload=api.reference(state,3) }
    case "play_sound","play_sound_at","play_sound_cue":
        path:=luau.to_string(vm,2); if len(path)==0 || len(path)>4096 { return binding_error(owner,"Sound requires bounded path or cue") }
        if op=="play_sound_cue" { command.kind=.Play_Sound_Cue; command.name=strings.clone(path,owner.allocator) } else {
            index:=i32(3); if op=="play_sound_at" { command.kind=.Play_Sound_At; position,valid:=vector(owner,3); if !valid || !finite_vector(position) { return binding_error(owner,"Sound position requires finite Vec3") }; command.origin=position; index=4 } else { command.kind=.Play_Sound }
            volume:=f32(1); volume_kind:=api.type(state,index)
            if volume_kind!=.None && volume_kind!=.Nil {
                explicit,valid:=number(owner,index)
                if volume_kind!=.Number || !valid || math.is_nan(explicit) || math.is_inf(explicit) || explicit<0 || explicit>1 { return binding_error(owner,"Sound volume requires a number in 0..1") }
                volume=explicit
            }
            looping:=false; loop_kind:=api.type(state,index+1)
            if loop_kind!=.None && loop_kind!=.Nil {
                if loop_kind!=.Boolean { return binding_error(owner,"Sound looping requires a bool") }
                looping=api.boolean(state,index+1)!=0
            }
            command.path=strings.clone(path,owner.allocator); command.volume=volume; command.looping=looping
        }
    case "raycast":
        origin,origin_valid:=vector(owner,2); direction,direction_valid:=vector(owner,3); maximum,valid:=number(owner,4)
        if !origin_valid || !direction_valid || !finite_vector(origin) || !finite_vector(direction) || km.length_squared(direction)==0 || !valid || (math.is_nan(maximum) || math.is_inf(maximum)) || maximum<=0 { return binding_error(owner,"Ray requires finite origin, direction and positive distance") }
        command.kind=.Raycast; command.origin=origin; command.vector=km.normalize(direction); command.max_distance=maximum
    case:
        target:=cast(^u64)userdata(owner,2,10); if target==nil { return binding_error(owner,"Entity argument required") }; command.entity=target^
        entity,exists:=snapshot.entities[target^]
        switch op {
        case "entity_exists": api.push_boolean(state,i32(exists)); return 1
        case "get_transform": if !exists || entity.without_transform { api.push_nil(state) } else { push_typed(owner,entity.transform,3) }; return 1
        case "get_position": if !exists || entity.without_transform { return binding_error(owner,"Entity is stale") }; push_typed(owner,entity.transform.position,1); return 1
        case "get_velocity": if !exists || !entity.has_velocity { api.push_nil(state) } else { push_typed(owner,entity.velocity,1) }; return 1
        }
        if !exists { return binding_error(owner,"Entity is stale") }
        switch op {
        case "destroy_entity": command.kind=.Destroy_Entity
        case "query_trigger_overlaps": command.kind=.Query_Trigger_Overlaps
        case "set_position","set_velocity","apply_force","apply_impulse":
            value,valid:=vector(owner,3); if !valid || !finite_vector(value) { return binding_error(owner,"Command requires finite Vec3") }; command.vector=value
            switch op {
            case "set_position": command.kind=.Set_Position
            case "set_velocity": command.kind=.Set_Velocity
            case "apply_force": command.kind=.Apply_Force
            case "apply_impulse": command.kind=.Apply_Impulse
            }
        case "set_transform":
            value:=cast(^km.Transform)userdata(owner,3,3); if value==nil || !finite_vector(value.position) || !finite_vector(value.scale) || !km.quat_is_normalized(value.rotation) { return binding_error(owner,"Transform command requires finite TRS and unit rotation") }; command.kind=.Set_Transform; command.transform=value^
        case "burst_particles":
            value,valid:=number(owner,3); if !valid || value<1 || value>100000 || value!=math.floor(value) { return binding_error(owner,"Burst count requires integer 1..100000") }; command.kind=.Burst_Particles; command.count=u32(value)
        case "set_particles_active": if api.type(state,3)!=.Boolean { return binding_error(owner,"Particle activation requires bool") }; command.kind=.Set_Particles_Active; command.active=api.boolean(state,3)!=0
        case: return binding_error(owner,"Unknown script world method")
        }
    }
    if len(owner.commands)>=4096 { command_destroy(owner,&command); return binding_error(owner,"Deferred script command queue is full") }
    append(&owner.commands,command)
    if op=="spawn_entity" || op=="raycast" || op=="query_trigger_overlaps" { api.push_number(state,f64(command.index)); return 1 }; return 0
}
@(private="package")
register_world :: proc(owner:^Runtime) {
    vm:=&owner.vm; vm.api.create_table(vm.state,0,4)
    for name in ([3]cstring{"id","__tostring","__eq"}) { bind(owner,string(name),entity_callback,name) }
    vm.api.push_value(vm.state,-1); vm.api.set_field(vm.state,-2,"__index"); lock_metatable(owner); owner.metatables[10]=vm.api.reference(vm.state,-1); luau.pop(vm)
    vm.api.create_table(vm.state,0,32)
    methods:=[]cstring{"find_entity","get_transform","set_transform","get_position","set_position","entity_exists","get_all_with","is_action_pressed","is_key_pressed","get_mouse_delta","get_mouse_wheel","spawn_entity","destroy_entity","emit","on_event","play_sound","play_sound_at","play_sound_cue","raycast","apply_force","apply_impulse","get_velocity","set_velocity","get_raycast_result","query_trigger_overlaps","get_trigger_overlaps","burst_particles","set_particles_active"}
    for name in methods { bind(owner,string(name),world_callback,name) }; vm.api.push_value(vm.state,-1); vm.api.set_field(vm.state,-2,"__index"); lock_metatable(owner); owner.metatables[11]=vm.api.reference(vm.state,-1); luau.pop(vm)
}
