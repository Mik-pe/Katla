//! Luau receives immutable scene snapshots and publishes owned commands after its tick.
package app
import ecs "../ecs"
import editor "../editor"
import agent "../agent"
import resources "../resources"
import km "../math"
import "core:strings"
import "core:fmt"
import "core:encoding/json"

@(private="package")
Script_Wire_Attachment :: struct { entity_id,path,source:string }
@(private="package")
Script_Wire_Entity :: struct { id,name:string,position:[3]f32 }
@(private="package")
Script_Wire_Event :: struct { name,trigger,other:string }
@(private="package")
Script_Wire_Command :: struct { kind,owner_id,entity_id,name,trigger,other:string,position:[3]f32,count:u32,active:bool }
@(private="package")
Script_Wire_Diagnostic :: struct { entity_id,path,error:string }
@(private="package")
Script_Wire_Instance :: struct { entity_id:string,spawned,disabled:bool,consecutive_errors:u32 }
@(private="package")
Script_Output :: struct { commands:[]Script_Wire_Command,diagnostics:[]Script_Wire_Diagnostic,instances:[]Script_Wire_Instance }
@(private="package")
script_output_destroy :: proc(output:^Script_Output) {
    for command in output.commands { delete(command.kind); delete(command.owner_id); delete(command.entity_id); delete(command.name); delete(command.trigger); delete(command.other) }
    for diagnostic in output.diagnostics { delete(diagnostic.entity_id); delete(diagnostic.path); delete(diagnostic.error) }; for instance in output.instances { delete(instance.entity_id) }
    delete(output.commands); delete(output.diagnostics); delete(output.instances); output^={}
}
/// Synchronizes actual source revisions without restarting unchanged Luau instances or subscriptions.
script_sync :: proc(app:^Authoring)->editor.Scene_Error {
    context.allocator=app.world.allocator; attachments:=make([dynamic]Script_Wire_Attachment,app.world.allocator)
    defer { for attachment in attachments { delete(attachment.entity_id); delete(attachment.source) }; delete(attachments) }
    ids:=ecs.entity_ids(&app.world); defer delete(ids); roots:=ecs.get_resource_mut(&app.world,Asset_Roots)
    for entity in ids {
        component,present:=ecs.get_component(&app.world,entity,Script_Component); if !present { continue }
        if roots==nil || !resources.valid_relative_path(component.path) || !strings.has_suffix(component.path,".luau") || component.root not_in (bit_set[Mesh_Path_Root]{.Resource,.Project}) { return .Invalid_Operation }
        root:=&roots.resource; if component.root==.Project { root=&roots.project }
        source,read_error:=resources.read_text(root,component.path,1024*1024); if read_error!=.None { return .Invalid_Operation }
        append(&attachments,Script_Wire_Attachment{fmt.aprintf("%d",u64(entity)),component.path,string(source)})
    }
    if len(attachments)==0 && !ecs.contains_resource(&app.world,Scene_Runtime) { return .None }
    response:=scene_runtime_call(app,struct {method:string,scripts:[]Script_Wire_Attachment}{"script_sync",attachments[:]}); defer runtime_response_destroy(&response)
    if !response.ok { return .Invalid_Operation }; return .None
}
/// Resets dependency instances and deferred callback emissions between authored preview sessions.
script_reset :: proc(app:^Authoring)->editor.Scene_Error {
    if !ecs.contains_resource(&app.world,Scene_Runtime) { return .None }
    response:=scene_runtime_call(app,struct {method:string}{"script_reset"}); defer runtime_response_destroy(&response); if !response.ok { return .Invalid_Operation }; return .None
}
@(private="package")
script_apply_command :: proc(app:^Authoring,command:Script_Wire_Command)->editor.Scene_Error {
    if command.kind=="emit" { return .None }
    entity,valid:=agent.parse_entity_id(command.entity_id); if !valid { return .Invalid_Field_Value }
    if !ecs.entity_exists(&app.world,entity) { return .Entity_Not_Found }; _,hidden:=ecs.get_component(&app.world,entity,Editor_Hidden); if hidden { return .Protected_Entity }
    switch command.kind {
    case "set_position":
        for value in command.position { if !finite_nonnegative(abs(value)) { return .Invalid_Field_Value } }
        target:=ecs.get_component_mut(&app.world,entity,Scene_Transform); if target==nil { return .Component_Not_Found }; local:=km.Vec3(command.position)
        if parent,has_parent:=ecs.get_component(&app.world,entity,Scene_Parent); has_parent { parent_matrix,parent_error:=scene_world_matrix(app,parent.entity); if parent_error!=.None { return parent_error }; inverse,invertible:=km.inverse(parent_matrix); if !invertible { return .Invalid_Operation }; local=km.transform_point(inverse,local) }
        target.local.position=local; return .None
    case "burst_particles": return particle_burst(&app.world,entity,command.count)
    case "set_particles_active": return particle_set_active(&app.world,entity,command.active)
    }
    return .Invalid_Operation
}
/// Runs actual sandboxed hooks/subscriptions from immutable poses, then applies commands in order.
script_step :: proc(app:^Authoring,delta_seconds:f32)->editor.Scene_Error {
    context.allocator=app.world.allocator; if !finite_nonnegative(delta_seconds) { return .Invalid_Field_Value }
    sync_error:=script_sync(app); if sync_error!=.None { return sync_error }
    if !ecs.contains_resource(&app.world,Scene_Runtime) { return .None }
    entities:=make([dynamic]Script_Wire_Entity,app.world.allocator); defer { for entity in entities { delete(entity.id) }; delete(entities) }
    events:=make([dynamic]Script_Wire_Event,app.world.allocator); defer { for event in events { delete(event.trigger); delete(event.other) }; delete(events) }
    ids:=ecs.entity_ids(&app.world); defer delete(ids)
    for entity in ids {
        if _,hidden:=ecs.get_component(&app.world,entity,Editor_Hidden); hidden { continue }
        if _,has_transform:=ecs.get_component(&app.world,entity,Scene_Transform); !has_transform { continue }
        world_matrix,transform_error:=scene_world_matrix(app,entity); if transform_error!=.None { return transform_error }
        name:=""; if label,present:=ecs.get_component(&app.world,entity,Scene_Name); present { name=label.name }
        append(&entities,Script_Wire_Entity{fmt.aprintf("%d",u64(entity)),name,km.mat4_extract_translation(world_matrix)})
    }
    signals:=ecs.get_resource_mut(&app.world,Script_Signals)
    if signals!=nil { for signal in signals.pending { append(&events,Script_Wire_Event{signal.name,fmt.aprintf("%d",u64(signal.trigger)),fmt.aprintf("%d",u64(signal.other))}) } }
    response:=scene_runtime_call(app,struct {method:string,delta_seconds:f32,entities:[]Script_Wire_Entity,events:[]Script_Wire_Event}{"script_tick",delta_seconds,entities[:],events[:]}); defer runtime_response_destroy(&response)
    if !response.ok { return .Invalid_Operation }
    if signals!=nil { for signal in signals.pending { delete(signal.name) }; clear(&signals.pending) }
    bytes,marshal_error:=json.marshal(response.result); if marshal_error!=nil { return .Decode_Failed }; defer delete(bytes)
    output:Script_Output; decode_error:=json.unmarshal(bytes,&output,allocator=app.world.allocator); defer script_output_destroy(&output); if decode_error!=nil { return .Decode_Failed }
    for instance in output.instances {
        entity,valid:=agent.parse_entity_id(instance.entity_id); if !valid { return .Decode_Failed }; component:=ecs.get_component_mut(&app.world,entity,Script_Component); if component==nil { continue }
        component.disabled=instance.disabled; component.consecutive_errors=instance.consecutive_errors
        for error in component.last_errors { delete(error) }; clear(&component.last_errors); if component.last_errors.allocator.procedure==nil { component.last_errors=make([dynamic]string,app.world.allocator) }
    }
    for diagnostic in output.diagnostics {
        entity,valid:=agent.parse_entity_id(diagnostic.entity_id); if !valid { return .Decode_Failed }; component:=ecs.get_component_mut(&app.world,entity,Script_Component); if component==nil { continue }; append(&component.last_errors,strings.clone(diagnostic.error))
    }
    for command in output.commands {
        error:=script_apply_command(app,command)
        if error!=.None { owner,valid:=agent.parse_entity_id(command.owner_id); if !valid { return .Decode_Failed }; component:=ecs.get_component_mut(&app.world,owner,Script_Component); if component!=nil { if component.last_errors.allocator.procedure==nil { component.last_errors=make([dynamic]string,app.world.allocator) }; append(&component.last_errors,fmt.aprintf("%s: %v",command.kind,error)) } }
    }
    return .None
}
