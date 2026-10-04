//! Canonical gameplay wrappers use the stationary native Luau owner.
package app
import ecs "../ecs"
import editor "../editor"

@(private="package")
script_runtime_required :: proc(owner:^Authoring)->bool {
    ids:=ecs.entity_ids(&owner.world); defer delete(ids)
    for entity in ids {
        if _,hidden:=ecs.get_component(&owner.world,entity,Editor_Hidden); hidden { continue }
        if _,present:=ecs.get_component(&owner.world,entity,Script_Component); present { return true }
    }
    return false
}
/// Synchronizes real source revisions; absent script work does not require a VM.
script_sync :: proc(owner:^Authoring)->editor.Scene_Error {
    if !ecs.contains_resource(&owner.world,Script_Native_Runtime) { return .Application_Owned if script_runtime_required(owner) else .None }
    return script_native_sync(owner)
}
/// Resets actual owned VMs between preview sessions.
script_reset :: proc(owner:^Authoring)->editor.Scene_Error {
    if !ecs.contains_resource(&owner.world,Script_Native_Runtime) { return .Application_Owned if script_runtime_required(owner) else .None }
    return script_native_reset(owner)
}
/// Executes sandboxed hooks and their typed deferred commands on the application owner.
script_step :: proc(owner:^Authoring,delta_seconds:f32)->editor.Scene_Error {
    if !finite_nonnegative(delta_seconds) { return .Invalid_Field_Value }
    if !ecs.contains_resource(&owner.world,Script_Native_Runtime) { return .Application_Owned if script_runtime_required(owner) else .None }
    return script_native_step(owner,delta_seconds)
}
