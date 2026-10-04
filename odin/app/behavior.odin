//! Validated script/emitter attachments share targeted undo and the actual runtime owner.
package app
import scene "../agent/scene"
import script "../script"
import ecs "../ecs"
import editor "../editor"
import km "../math"
import "core:mem"
import "core:slice"
import "core:strings"
import "core:fmt"
import "core:encoding/json"

/// Durable script path stays relative to its retained resource or project root.
Script_Component :: struct { path:string `inspect:"skip"`,root:Mesh_Path_Root `inspect:"skip"`,disabled:bool `inspect:"skip"`,consecutive_errors:u32 `inspect:"skip"`,last_errors:[dynamic]string `inspect:"skip"` }
@(private="package")
script_component_destroy :: proc(value:rawptr) { component:=cast(^Script_Component)value; delete(component.path); for error in component.last_errors { delete(error) }; delete(component.last_errors); component^={} }
@(private="package")
script_component_clone :: proc(dst,src:rawptr) { source:=cast(^Script_Component)src; target:=cast(^Script_Component)dst; target^=source^; target.path=strings.clone(source.path); target.last_errors=make([dynamic]string,0,len(source.last_errors)); for error in source.last_errors { append(&target.last_errors,strings.clone(error)) } }
/// Registers optional owned behavior components without adding them to every scene spawn.
behavior_register :: proc(app:^Authoring) { editor.editor_register(&app.world,&app.registry,"Script",Script_Component{},ecs.Value_Ops{script_component_destroy,script_component_clone},spawn_default=false); particle_register(&app.world,&app.registry) }
@(private="package")
Attachment_Kind :: enum { Script, Particles }
@(private="package")
Attachment :: struct { kind:Attachment_Kind,present:bool,script:Script_Component,particles:Particle_Emitter }
@(private="package")
Attachment_Command :: struct { entity:ecs.Entity_Id,before,after:Attachment }
@(private="package")
attachment_destroy :: proc(attachment:^Attachment) { if attachment.kind==.Script { script_component_destroy(&attachment.script) } else { particle_destroy(&attachment.particles) }; attachment^={} }
@(private="package")
attachment_clone :: proc(source:Attachment)->Attachment { source_copy:=source; target:=source; if source.kind==.Script { script_component_clone(&target.script,&source_copy.script) } else { particle_clone(&target.particles,&source_copy.particles); delete(target.particles.descriptor.burst_queue); target.particles.descriptor.burst_queue=nil }; return target }
@(private="package")
attachment_apply :: proc(w:^ecs.World,entity:ecs.Entity_Id,attachment:Attachment)->editor.Scene_Error {
    if !ecs.entity_exists(w,entity) { return .Entity_Not_Found }; _,hidden:=ecs.get_component(w,entity,Editor_Hidden); if hidden { return .Protected_Entity }
    context.allocator=w.allocator; source_copy:=attachment
    if attachment.kind==.Script { if attachment.present { script:Script_Component; script_component_clone(&script,&source_copy.script); ecs.add_component(w,entity,script) } else { ecs.remove_component(w,entity,Script_Component) } }
    else { if attachment.present { particles:Particle_Emitter; particle_clone(&particles,&source_copy.particles); if live:=ecs.get_component_mut(w,entity,Particle_Emitter); live!=nil { delete(particles.descriptor.burst_queue); particles.descriptor.burst_queue=slice.clone_to_dynamic(live.descriptor.burst_queue[:],w.allocator); particles.descriptor.emission_revision=max(particles.descriptor.emission_revision,live.descriptor.emission_revision); if particles.descriptor.active && !live.descriptor.active { particles.descriptor.emission_revision=particle_next_revision(particles.descriptor.emission_revision) } }; ecs.add_component(w,entity,particles) } else { ecs.remove_component(w,entity,Particle_Emitter) } }; return .None
}
@(private="package")
attachment_command_apply :: proc(state:rawptr,w:^ecs.World,_:^editor.Component_Registry,redo:bool,_:^[dynamic]editor.Entity_Remap)->editor.Scene_Error { command:=cast(^Attachment_Command)state; return attachment_apply(w,command.entity,command.after if redo else command.before) }
@(private="package")
attachment_command_destroy :: proc(state:rawptr,allocator:mem.Allocator) { context.allocator=allocator; command:=cast(^Attachment_Command)state; attachment_destroy(&command.before); attachment_destroy(&command.after); free(command,allocator) }
@(private="package")
attachment_command_remap :: proc(state:rawptr,remap:editor.Entity_Remap) { command:=cast(^Attachment_Command)state; if command.entity==remap.before { command.entity=remap.after } }
@(private="package")
behavior_edit :: proc(app:^Authoring,entity:ecs.Entity_Id,after:Attachment)->(editor.Undo_Group,editor.Scene_Error) {
    if app.mode!=.Editing { return {},.Editing_Required }; context.allocator=app.world.allocator
    before:=Attachment{kind=after.kind}
    if after.kind==.Script { component,present:=ecs.get_component(&app.world,entity,Script_Component); before.present=present; before.script=component }
    else { component,present:=ecs.get_component(&app.world,entity,Particle_Emitter); before.present=present; before.particles=component }
    command:=new(Attachment_Command,app.world.allocator); command^={entity,attachment_clone(before),attachment_clone(after)}
    err:=attachment_apply(&app.world,entity,after); if err!=.None { attachment_command_destroy(command,app.world.allocator); return {},err }
    return editor.undo_group_create(command,{attachment_command_apply,attachment_command_destroy,attachment_command_remap},{entity},app.world.allocator),.None
}
@(private="package")
particle_document :: proc(p:Particle_Descriptor)->json.Value {
    bytes,err:=json.marshal(p,opt=json.Marshal_Options{use_enum_names=true}); if err!=nil { return nil }; defer delete(bytes); tree,parse_error:=json.parse(bytes,spec=.JSON,parse_integers=true); if parse_error!=nil { return nil }
    object,_:=tree.(json.Object)
    keys:=make([dynamic]string); defer delete(keys)
    for key in object { if key=="has_timed_emission" || (key=="timed_emission" && !p.has_timed_emission) { append(&keys,key) } }
    for key in keys { json.destroy_value(object[key]); delete_key(&object,key); delete(key) }; return tree
}
@(private="package")
behavior_target :: proc(app:^Authoring,entity:ecs.Entity_Id)->editor.Scene_Error { if !ecs.entity_exists(&app.world,entity) { return .Entity_Not_Found }; _,hidden:=ecs.get_component(&app.world,entity,Editor_Hidden); if hidden { return .Protected_Entity }; _,transform:=ecs.get_component(&app.world,entity,Scene_Transform); if !transform { return .Component_Not_Found }; return .None }
/// Reads only a confined Luau source and validates it with the actual sandbox before mutation.
script_validate_attachment :: proc(app:^Authoring,path:string)->(string,editor.Scene_Error) {
    resolved,resolve_error:=script_source_resolve(app,path); if resolve_error!=.None { return "",resolve_error }; defer delete(resolved.path,app.world.allocator)
    bytes,read_error:=script_source_read(app,resolved); if read_error!=.None { return "",read_error }; defer delete(bytes,app.world.allocator)
    if owner:=ecs.get_resource_mut(&app.world,Script_Native_Runtime); owner!=nil { failure:=script.validate(owner.runtime,string(bytes),path); if failure!="" { return failure,.Invalid_Operation }; return "",.None }
    return strings.clone("Initialize the native Luau runtime before attaching scripts",app.world.allocator),.Application_Owned
}
/// Authors complete descriptors, explicit detach and bounded previews on the application owner.
behavior_execute :: proc(app:^Authoring,op:scene.Behavior_Op)->(editor.Tool_Result,editor.Undo_Group) {
    context.allocator=app.world.allocator; result:=error_result(&app.world,.None); undo:editor.Undo_Group
    if op.action==.Describe {
        document:=particle_document(particle_defaults()); defer json.destroy_value(document)
        result.data,_=json.marshal(struct {particle_example:json.Value,script_path,particle_color_space,detach:string,burst_limits:struct {count,queued_bursts:int}}{document,"scripts/prefab-effect.luau","linear RGBA","path/document null removes an attachment; shared undo restores it",{100000,1024}}); if result.data==nil { result.error=.Decode_Failed }; return result,undo
    }
    result.error=behavior_target(app,op.entity); if result.error!=.None { return result,undo }
    switch op.action {
    case .Set_Script:
        if app.mode!=.Editing { result.error=.Editing_Required; return result,undo }
        source:Script_Component; defer delete(source.path)
        if !op.detach {
            resolve_error:editor.Scene_Error; source,resolve_error=script_source_resolve(app,op.path); if resolve_error!=.None { result.error=resolve_error; return result,undo }
            message,validate_error:=script_validate_attachment(app,op.path); defer delete(message); if validate_error!=.None { result.error=validate_error; if message!="" { result.data,_=json.marshal(struct {error:string}{message}) }; return result,undo }
        }
        undo,result.error=behavior_edit(app,op.entity,Attachment{kind=.Script,present=!op.detach,script=source})
    case .Set_Particles:
        if app.mode!=.Editing { result.error=.Editing_Required; return result,undo }; descriptor:=particle_defaults()
        if !op.detach { valid:bool; descriptor,valid=particle_decode(op.document,app.world.allocator); if !valid { result.error=.Invalid_Field_Value; return result,undo } }; defer delete(descriptor.burst_queue)
        if len(descriptor.burst_queue)>0 { if _,present:=ecs.get_component(&app.world,op.entity,Particle_Emitter); present { result.error=.Invalid_Field_Value; return result,undo } }
        undo,result.error=behavior_edit(app,op.entity,Attachment{kind=.Particles,present=!op.detach,particles=Particle_Emitter{descriptor}})
    case .Burst: result.error=particle_burst(&app.world,op.entity,op.count)
    case .Set_Active:
        if app.mode==.Editing { emitter,present:=ecs.get_component(&app.world,op.entity,Particle_Emitter); if !present { result.error=.Component_Not_Found; return result,undo }; emitter.descriptor.active=op.active; if op.active { emitter.descriptor.emission_revision=particle_next_revision(emitter.descriptor.emission_revision) }; undo,result.error=behavior_edit(app,op.entity,Attachment{kind=.Particles,present=true,particles=emitter}) }
        else { result.error=particle_set_active(&app.world,op.entity,op.active) }
    case .Inspect:
    case .Describe:
    }
    if result.error!=.None { return result,undo }
    script:=ecs.get_component_mut(&app.world,op.entity,Script_Component); emitter:=ecs.get_component_mut(&app.world,op.entity,Particle_Emitter)
    script_path:json.Value=json.Null{}; if script!=nil { script_path=script.path }
    particle_value:json.Value=json.Null{}; if emitter!=nil { particle_value=particle_document(emitter.descriptor) }; defer json.destroy_value(particle_value)
    entity_text:=fmt.aprintf("%d",u64(op.entity)); defer delete(entity_text); world_matrix,_:=scene_world_matrix(app,op.entity)
    script_status:json.Value=json.Null{}; if script!=nil { script_status=trigger_json_value(script^) }; defer json.destroy_value(script_status)
    result.data,_=json.marshal(struct {entity_id:string,script,particles:json.Value,world_position:[3]f32,editing:bool,script_runtime:json.Value}{entity_text,script_path,particle_value,km.mat4_extract_translation(world_matrix),app.mode==.Editing,script_status})
    append(&result.entities,op.entity); if result.data==nil { result.error=.Decode_Failed }; return result,undo
}
