//! Script inspector handles resolve live VM generations without exposing foreign registry references.
package app
import script "../script"
import ecs "../ecs"
import editor "../editor"

/// Compiles current authored revisions, then copies scalar variables for the inspector.
script_inspect :: proc(app:^Authoring,entity:ecs.Entity_Id)->([]script.Variable,script.Handle,editor.Scene_Error) {
    if error:=scene_action_target(app,entity); error!=.None { return nil,{},error }
    owner:=ecs.get_resource_mut(&app.world,Script_Native_Runtime); if owner==nil { return nil,{},.Application_Owned }
    sync_error:=script_native_sync(app)
    instance,present:=script.handle(owner.runtime,u64(entity)); if !present { if sync_error!=.None { return nil,{},sync_error }; return nil,{},.Component_Not_Found }
    values,failure:=script.inspect(owner.runtime,instance); defer delete(failure,app.world.allocator)
    if failure!="" { return nil,{},.Invalid_Operation }; return values,instance,.None
}
/// Mutates one actual VM scalar only if the inspected instance generation still exists.
script_set_variable :: proc(app:^Authoring,instance:script.Handle,name:string,value:script.Scalar)->editor.Scene_Error {
    if error:=scene_action_target(app,ecs.Entity_Id(instance.entity)); error!=.None { return error }
    owner:=ecs.get_resource_mut(&app.world,Script_Native_Runtime); if owner==nil { return .Application_Owned }
    failure:=script.set_variable(owner.runtime,instance,name,value); defer delete(failure,app.world.allocator)
    if failure!="" { return .Invalid_Field_Value }; return .None
}
