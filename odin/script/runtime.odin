//! Environment replacement, lifecycle and subscription ownership stay outside the dependency VM.
package script
import luau "../deps/luau"
import "core:strings"
import "core:slice"

/// Initializes one stationary owner; moving it after callback registration is invalid.
init :: proc(owner:^Runtime,path:string,allocator:=context.allocator)->luau.Error {
    if owner.vm.state!=nil { return .Invalid }; owner.allocator=allocator; owner.ctx=context; owner.ctx.allocator=allocator
    result:=luau.init(&owner.vm,path,allocator); if result!=.None { return result }
    owner.instances=make(map[u64]^Instance,allocator); owner.commands=make([dynamic]Command,allocator); owner.deferred_events=make([dynamic]Event,allocator)
    owner.logs=make([dynamic]Log,allocator)
    register_math(owner); register_world(owner); register_logging(owner); owner.vm.api.sandbox(owner.vm.state)
    if owner.vm.api.take_error(owner.vm.state)!=nil { destroy(owner); return .Native }; return .None
}
@(private="package")
environment_prepare :: proc(owner:^Runtime,source,path:string)->(i32,string) {
    vm:=&owner.vm; base:=vm.api.get_top(vm.state)
    reference,failure:=environment_prepare_inner(owner,source,path)
    if native:=vm.api.take_error(vm.state); native!=nil {
        replacement:=strings.clone(string(native),owner.allocator)
        if reference>=0 { vm.api.unreference(vm.state,reference) }; reference=-1
        delete(failure,owner.allocator); failure=replacement
    }
    vm.api.set_top(vm.state,base); return reference,failure
}
@(private="package")
environment_prepare_inner :: proc(owner:^Runtime,source,path:string)->(reference:i32,failure:string) {
    reference=-1
    if len(source)>1024*1024 || len(path)>4096 { failure=strings.clone("Script source or path exceeds budget",owner.allocator); return }
    vm:=&owner.vm
    vm.api.create_table(vm.state,0,16); environment:=vm.api.abs_index(vm.state,-1)
    vm.api.create_table(vm.state,0,1); vm.api.push_value(vm.state,luau.GLOBALS_INDEX); vm.api.set_field(vm.state,-2,"__index"); vm.api.set_metatable(vm.state,environment)
    name:=strings.clone_to_cstring(path,owner.allocator); defer delete(name,owner.allocator)
    status:=vm.api.compile_load(vm.state,raw_data(source),uint(len(source)),name,environment)
    if status==0 { status=vm.api.run(vm.state,0,0) }; if status!=0 { failure=strings.clone(luau.to_string(vm,-1),owner.allocator); return }
    for hook in ([3]cstring{"on_spawn","on_update","on_destroy"}) { kind:=vm.api.get_field(vm.state,environment,hook); luau.pop(vm); if kind!=.Nil && kind!=.Function { failure=strings.clone("Lifecycle hooks must be functions",owner.allocator); return } }
    reference=vm.api.reference(vm.state,environment); return
}
/// Compiles and executes the chunk in a temporary isolated environment before accepting it.
validate :: proc(owner:^Runtime,source,path:string)->string {
    if luau.owner_error(&owner.vm)!=.None { return strings.clone("Script runtime thread mismatch",owner.allocator) }
    reference,error:=environment_prepare(owner,source,path); if reference>=0 { owner.vm.api.unreference(owner.vm.state,reference) }; return error
}
@(private="package")
clear_subscriptions :: proc(owner:^Runtime,instance:^Instance) { for entry in instance.subscriptions { delete(entry.name,owner.allocator); owner.vm.api.unreference(owner.vm.state,entry.callback) }; clear(&instance.subscriptions) }
@(private="package")
instance_destroy :: proc(owner:^Runtime,instance:^Instance)->string {
    vm:=&owner.vm; failure:=""
    if instance.spawned && !instance.disabled {
        owner.current_entity=instance.id
        base:=vm.api.get_top(vm.state); luau.get_reference(vm,instance.environment); kind:=vm.api.get_field(vm.state,-1,"on_destroy"); vm.api.remove(vm.state,-2)
        if native:=vm.api.take_error(vm.state); native!=nil { failure=strings.clone(string(native),owner.allocator) }
        else if kind==.Function { push_entity(owner,instance.id); if vm.api.run(vm.state,1,0)!=0 { failure=strings.clone(luau.to_string(vm,-1),owner.allocator) } }; vm.api.set_top(vm.state,base)
    }
    clear_subscriptions(owner,instance); delete(instance.subscriptions); vm.api.unreference(vm.state,instance.environment); delete(instance.path,owner.allocator); delete(instance.source,owner.allocator); free(instance,owner.allocator); return failure
}
/// Stages every changed environment before retiring any current instance or subscription.
sync :: proc(owner:^Runtime,attachments:[]Attachment,force_entity:u64=0,force:=false)->(diagnostics:[dynamic]Diagnostic,error:string) {
    context.allocator=owner.allocator; diagnostics=make([dynamic]Diagnostic,owner.allocator)
    if luau.owner_error(&owner.vm)!=.None { return diagnostics,strings.clone("Script runtime thread mismatch") }
    if len(attachments)>100_000 { return diagnostics,strings.clone("Script attachment capacity exhausted") }
    seen:=make(map[u64]bool,owner.allocator); defer delete(seen); staged:=make(map[u64]^Instance,owner.allocator); defer delete(staged)
    success:=false; defer { if !success { for _,instance in staged { failure:=instance_destroy(owner,instance); delete(failure) } } }
    for attachment in attachments {
        if seen[attachment.entity] { return diagnostics,strings.clone("Duplicate script entity") }; seen[attachment.entity]=true
        if old,present:=owner.instances[attachment.entity]; present && old.path==attachment.path && old.source==attachment.source && !(force && attachment.entity==force_entity) { continue }
        reference,failure:=environment_prepare(owner,attachment.source,attachment.path); if reference<0 { return diagnostics,failure }
        if old,present:=owner.instances[attachment.entity]; present && old.path==attachment.path {
            preserved,inspect_error:=environment_variables(owner,old.environment)
            if inspect_error!="" { owner.vm.api.unreference(owner.vm.state,reference); return diagnostics,inspect_error }
            for variable in preserved { set_error:=environment_set(owner,reference,variable.name,variable.value); if set_error!="" { variables_destroy(preserved,owner.allocator); owner.vm.api.unreference(owner.vm.state,reference); return diagnostics,set_error } }
            variables_destroy(preserved,owner.allocator)
        }
        if owner.next_serial==max(u64) { owner.vm.api.unreference(owner.vm.state,reference); return diagnostics,strings.clone("Script instance identity exhausted") }
        owner.next_serial+=1; instance:=new(Instance,owner.allocator)
        instance^={id=attachment.entity,serial=owner.next_serial,environment=reference,path=strings.clone(attachment.path),source=strings.clone(attachment.source),subscriptions=make([dynamic]Subscription,owner.allocator)}; staged[attachment.entity]=instance
    }
    retired:=make([dynamic]u64,owner.allocator); defer delete(retired)
    for id,_ in owner.instances { _,replace:=staged[id]; if !seen[id] || replace { append(&retired,id) } }; slice.sort(retired[:])
    for id in retired { instance:=owner.instances[id]; path:=strings.clone(instance.path); failure:=instance_destroy(owner,instance); if failure!="" { append(&diagnostics,Diagnostic{id,path,failure}) } else { delete(path) }; delete_key(&owner.instances,id) }
    for id,instance in staged { owner.instances[id]=instance }; success=true; return diagnostics,""
}
/// Invalidates all subscriptions, calls active destroy hooks and clears deferred events.
reset :: proc(owner:^Runtime)->[dynamic]Diagnostic {
    context.allocator=owner.allocator; diagnostics:=make([dynamic]Diagnostic,owner.allocator)
    if luau.owner_error(&owner.vm)!=.None { append(&diagnostics,Diagnostic{error=strings.clone("Script runtime thread mismatch")}); return diagnostics }
    ids:=make([dynamic]u64,owner.allocator); defer delete(ids); for id,_ in owner.instances { append(&ids,id) }; slice.sort(ids[:])
    for id in ids { instance:=owner.instances[id]; path:=strings.clone(instance.path); failure:=instance_destroy(owner,instance); if failure!="" { append(&diagnostics,Diagnostic{id,path,failure}) } else { delete(path) } }; clear(&owner.instances)
    for event in owner.deferred_events { delete(event.name); delete(event.animation_clip); if event.payload>0 { owner.vm.api.unreference(owner.vm.state,event.payload) } }; clear(&owner.deferred_events)
    for &command in owner.commands { command_destroy(owner,&command) }; clear(&owner.commands); return diagnostics
}
/// Destroys callbacks and the VM before releasing their captured Odin allocator state.
destroy :: proc(owner:^Runtime)->luau.Error {
    error:=luau.owner_error(&owner.vm); if error!=.None { return error }
    diagnostics:=reset(owner); for diagnostic in diagnostics { delete(diagnostic.path,owner.allocator); delete(diagnostic.error,owner.allocator) }; delete(diagnostics)
    result:=luau.destroy(&owner.vm); delete(owner.instances); delete(owner.commands); delete(owner.deferred_events)
    for entry in owner.logs { delete(entry.message,owner.allocator) }; delete(owner.logs)
    for binding in owner.bindings[:owner.binding_count] { delete(binding.name,owner.allocator) }; owner^={}; return result
}
