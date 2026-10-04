//! Direct Luau instances receive owned ECS snapshots and publish host commands after the tick.
package app
import script "../script"
import luau "../deps/luau"
import ecs "../ecs"
import editor "../editor"
import "core:strings"
import "core:mem"

/// Stationary script VM and deferred query feedback belong to the application thread.
Script_Native_Runtime :: struct { runtime:^script.Runtime,queries:script.Query_Results,logs:[dynamic]script.Log,allocator:mem.Allocator }
@(private="package")
script_native_queries_clear :: proc(owner:^Script_Native_Runtime) { for _,values in owner.queries.overlaps { delete(values,owner.allocator) }; clear(&owner.queries.rays); clear(&owner.queries.overlaps) }
@(private="package")
script_native_destroy :: proc(value:rawptr) {
    owner:=cast(^Script_Native_Runtime)value; context.allocator=owner.allocator
    script_native_queries_clear(owner); delete(owner.queries.rays); delete(owner.queries.overlaps)
    for log in owner.logs { delete(log.message) }; delete(owner.logs)
    assert(script.destroy(owner.runtime)==luau.Error.None,"Luau owner requires its application thread"); free(owner.runtime); owner^={}
}
/// Initializes the source-pinned VM without publishing an owner on library or ABI failure.
script_native_init :: proc(app:^Authoring,path:string)->editor.Scene_Error {
    if ecs.contains_resource(&app.world,Script_Native_Runtime) { return .Invalid_Operation }
    context.allocator=app.world.allocator; runtime:=new(script.Runtime,app.world.allocator)
    if script.init(runtime,path,app.world.allocator)!=.None { free(runtime); return .Application_Owned }
    owner:=Script_Native_Runtime{runtime=runtime,allocator=app.world.allocator,queries={rays=make(map[script.Query_Key]script.Ray_Result),overlaps=make(map[script.Query_Key][]u64)},logs=make([dynamic]script.Log)}
    ecs.insert_resource(&app.world,owner,ecs.Value_Ops{destroy=script_native_destroy}); return .None
}
@(private="package")
script_native_diagnostics_destroy :: proc(values:[dynamic]script.Diagnostic,allocator:mem.Allocator) { for value in values { delete(value.path,allocator); delete(value.error,allocator) }; delete(values) }
/// Loads confined source revisions and stages every replacement before retiring any live instance.
script_native_sync :: proc(app:^Authoring,force_entity:ecs.Entity_Id={},force:=false)->editor.Scene_Error {
    context.allocator=app.world.allocator; owner:=ecs.get_resource_mut(&app.world,Script_Native_Runtime); if owner==nil { return .Application_Owned }
    attachments:=make([dynamic]script.Attachment); defer { for attachment in attachments { delete(attachment.source) }; delete(attachments) }
    ids:=ecs.entity_ids(&app.world); defer delete(ids)
    for entity in ids {
        if _,hidden:=ecs.get_component(&app.world,entity,Editor_Hidden); hidden { continue }
        component,present:=ecs.get_component(&app.world,entity,Script_Component); if !present { continue }
        source,error:=script_source_read(app,component); if error!=.None { return error }
        append(&attachments,script.Attachment{u64(entity),component.path,string(source)})
    }
    diagnostics,failure:=script.sync(owner.runtime,attachments[:],u64(force_entity),force); defer script_native_diagnostics_destroy(diagnostics,app.world.allocator); defer delete(failure)
    if failure!="" { if force { script_native_error(app,u64(force_entity),failure); append(&owner.logs,script.Log{u64(force_entity),.Warn,strings.clone(failure)}) }; return .Invalid_Operation }; return .None
}
/// Clears owned subscriptions, pending events and query feedback between preview sessions.
script_native_reset :: proc(app:^Authoring)->editor.Scene_Error {
    owner:=ecs.get_resource_mut(&app.world,Script_Native_Runtime); if owner==nil { return .Application_Owned }
    diagnostics:=script.reset(owner.runtime); defer script_native_diagnostics_destroy(diagnostics,app.world.allocator)
    script_native_queries_clear(owner); audio_runtime_reset(app)
    if len(diagnostics)>0 { return .Invalid_Operation }; return .None
}
/// Transfers completed console packets; the caller destroys them with the world allocator.
script_logs_drain :: proc(app:^Authoring)->[]script.Log {
    owner:=ecs.get_resource_mut(&app.world,Script_Native_Runtime); if owner==nil { return nil }
    logs:=make([]script.Log,len(owner.logs),app.world.allocator); copy(logs,owner.logs[:]); clear(&owner.logs); return logs
}
/// Releases a drained console packet array and every message it owns.
script_logs_destroy :: proc(logs:[]script.Log,allocator:=context.allocator) { for log in logs { delete(log.message,allocator) }; delete(logs,allocator) }

/// Reloads real source while preserving the previous instance and scalar state on compilation failure.
script_reload :: proc(app:^Authoring,entity:ecs.Entity_Id)->editor.Scene_Error {
    if error:=scene_action_target(app,entity); error!=.None { return error }
    if _,present:=ecs.get_component(&app.world,entity,Script_Component); !present { return .Component_Not_Found }
    return script_native_sync(app,entity,true)
}
