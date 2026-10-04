//! Lifecycle and callback emissions observe one immutable tick; failed hooks discard their commands.
package script
import luau "../deps/luau"
import "core:strings"
import "core:slice"
import "core:math"
import "core:fmt"

@(private="package")
rollback_commands :: proc(owner:^Runtime,checkpoint:int) { for &command in owner.commands[checkpoint:] { command_destroy(owner,&command) }; resize(&owner.commands,checkpoint) }
@(private="package")
call_hook :: proc(owner:^Runtime,instance:^Instance,snapshot:^Snapshot,name:cstring,delta:f32)->string {
    vm:=&owner.vm; base:=vm.api.get_top(vm.state); defer vm.api.set_top(vm.state,base)
    luau.get_reference(vm,instance.environment); kind:=vm.api.get_field(vm.state,-1,name); vm.api.remove(vm.state,-2)
    if native:=vm.api.take_error(vm.state); native!=nil { return strings.clone(string(native),owner.allocator) }
    if kind==.Nil { return "" }; if kind!=.Function { return strings.clone("Lifecycle hook must be a function",owner.allocator) }
    push_entity(owner,instance.id); push_proxy(owner,snapshot,instance); arguments:=i32(2)
    if string(name)=="on_update" { vm.api.push_number(vm.state,f64(delta)); arguments=3 }
    checkpoint:=len(owner.commands); if vm.api.run(vm.state,arguments,0)==0 { return "" }
    rollback_commands(owner,checkpoint); return strings.clone(luau.to_string(vm,-1),owner.allocator)
}
@(private="package")
push_event :: proc(owner:^Runtime,event:Event) {
    vm:=&owner.vm
    if event.payload>0 { luau.get_reference(vm,event.payload); return }
    vm.api.create_table(vm.state,0,4); push_entity(owner,event.trigger); vm.api.set_field(vm.state,-2,"trigger"); push_entity(owner,event.other); vm.api.set_field(vm.state,-2,"other")
    trigger:=fmt.aprintf("%d",event.trigger); other:=fmt.aprintf("%d",event.other); defer { delete(trigger); delete(other) }
    luau.push_string(vm,trigger); vm.api.set_field(vm.state,-2,"trigger_entity")
    luau.push_string(vm,other); vm.api.set_field(vm.state,-2,"other_entity")
}
/// Executes deterministic entity order, then transfers commands for host processing after every script.
tick :: proc(owner:^Runtime,delta:f32,entities:[]Entity_State,events:[]Event=nil,input:Input={},queries:Query_Results={})->(output:Output,error:string) {
    context.allocator=owner.allocator; output.allocator=owner.allocator
    if luau.owner_error(&owner.vm)!=.None { return output,strings.clone("Script runtime thread mismatch") }
    if math.is_nan(delta) || math.is_inf(delta) || delta<0 || len(entities)>100_000 || len(events)>4096 || len(events)+len(owner.deferred_events)>8192 { return output,strings.clone("Invalid bounded script tick") }
    seen:=make(map[u64]bool,owner.allocator); defer delete(seen)
    for entity in entities { if seen[entity.id] || !finite_vector(entity.transform.position) || !finite_vector(entity.transform.scale) || !finite_vector(entity.velocity) { return output,strings.clone("Invalid entity snapshot") }; seen[entity.id]=true }
    for event in events { if len(strings.trim_space(event.name))==0 || len(event.name)>128 { return output,strings.clone("Invalid event name") } }
    for value in input.mouse_delta { if math.is_nan(value) || math.is_inf(value) { return output,strings.clone("Invalid input snapshot") } }; if math.is_nan(input.mouse_wheel) || math.is_inf(input.mouse_wheel) || len(input.actions)>256 || len(input.keys)>256 { return output,strings.clone("Invalid input snapshot") }
    if owner.tick_serial==max(u64) { return output,strings.clone("Script tick identity exhausted") }; owner.tick_serial+=1
    snapshot:=snapshot_create(owner,entities,input,queries); defer snapshot_release(snapshot)
    output.diagnostics=make([dynamic]Diagnostic,owner.allocator); output.instances=make([dynamic]Instance_State,owner.allocator)
    stale:=make([dynamic]u64,owner.allocator); defer delete(stale); ids:=make([dynamic]u64,owner.allocator); defer delete(ids)
    for id,_ in owner.instances { if !seen[id] { append(&stale,id) } else { append(&ids,id) } }; slice.sort(stale[:]); slice.sort(ids[:])
    for id in stale { instance:=owner.instances[id]; path:=strings.clone(instance.path); failure:=instance_destroy(owner,instance); if failure!="" { append(&output.diagnostics,Diagnostic{id,path,failure}) } else { delete(path) }; delete_key(&owner.instances,id) }
    delivery:=owner.deferred_events; owner.deferred_events=make([dynamic]Event,owner.allocator)
    defer { for event in delivery { delete(event.name); if event.payload>0 { owner.vm.api.unreference(owner.vm.state,event.payload) } }; delete(delivery) }
    for event in events { copy:=event; copy.name=strings.clone(event.name); if event.payload>0 { luau.get_reference(&owner.vm,event.payload); copy.payload=owner.vm.api.reference(owner.vm.state,-1); luau.pop(&owner.vm) }; append(&delivery,copy) }
    for id in ids {
        instance:=owner.instances[id]; failed:=false; owner.current_entity=id
        if !instance.disabled {
            if !instance.spawned { instance.spawned=true; failure:=call_hook(owner,instance,snapshot,"on_spawn",delta); if failure!="" { failed=true; append(&output.diagnostics,Diagnostic{id,strings.clone(instance.path),failure}) } }
            failure:=call_hook(owner,instance,snapshot,"on_update",delta); if failure!="" { failed=true; append(&output.diagnostics,Diagnostic{id,strings.clone(instance.path),failure}) }
            subscription_count:=len(instance.subscriptions)
            for event in delivery { for i in 0..<subscription_count {
                subscription:=instance.subscriptions[i]; if subscription.name!=event.name { continue }
                vm:=&owner.vm; base:=vm.api.get_top(vm.state); luau.get_reference(vm,subscription.callback); luau.push_string(vm,event.name); push_event(owner,event); push_proxy(owner,snapshot,instance)
                checkpoint:=len(owner.commands); if vm.api.run(vm.state,3,0)!=0 { failed=true; rollback_commands(owner,checkpoint); append(&output.diagnostics,Diagnostic{id,strings.clone(instance.path),strings.clone(luau.to_string(vm,-1))}) }; vm.api.set_top(vm.state,base)
            } }
            if failed { instance.errors+=1 } else { instance.errors=0 }; if instance.errors>=10 { instance.disabled=true; clear_subscriptions(owner,instance) }
        }
        append(&output.instances,Instance_State{id,instance.spawned,instance.disabled,instance.errors})
    }
    output.commands=owner.commands; owner.commands=make([dynamic]Command,owner.allocator)
    output.logs=owner.logs; owner.logs=make([dynamic]Log,owner.allocator); owner.current_entity=0
    for command in output.commands { if command.kind==.Emit {
        event:=Event{strings.clone(command.name),command.owner,command.owner,-1}; if command.payload>0 { luau.get_reference(&owner.vm,command.payload); event.payload=owner.vm.api.reference(owner.vm.state,-1); luau.pop(&owner.vm) }; append(&owner.deferred_events,event)
    } }
    return output,""
}
