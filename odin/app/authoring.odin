//! Scene authoring composition owns the world, editor history and application tools.
package app

import ecs "../ecs"
import editor "../editor"
import agent "../agent"

/// Protects editor-owned entities from targeted scene and material tools.
Editor_Hidden :: struct {}
/// Authoring mutations require editing; preview lifecycle migration remains separate.
Play_Mode :: enum { Editing, Playing, Paused }
/// A stationary scene owner; producers receive only its synchronized agent mailbox.
Authoring :: struct { world:ecs.World, registry:editor.Component_Registry, agent:editor.Agent_Harness, mode:Play_Mode }
/// Initializes CPU authoring state without installing graphics or network services.
authoring_init :: proc(app:^Authoring,allocator:=context.allocator) {
    ecs.world_init(&app.world,allocator=allocator)
    editor.editor_registry_init(&app.registry,allocator)
    editor.editor_register(&app.world,&app.registry,"SurfaceMaterial",Surface_Material{metallic=0,roughness=0.5,ao=1})
    ecs.register_component(&app.world,Editor_Hidden)
    editor.agent_harness_init(&app.agent,allocator)
}
/// Join producers first; the shared history releases command state before the registry/world.
authoring_destroy :: proc(app:^Authoring) {
    editor.agent_harness_destroy(&app.agent)
    editor.editor_registry_destroy(&app.registry)
    ecs.world_destroy(&app.world)
    app^={}
}
/// Runs mailbox work on the scene owner thread.
authoring_tick :: proc(app:^Authoring)->int { return editor.agent_tick(&app.agent,&app.world,&app.registry,authoring_executor(app)) }
/// Supplies owner-thread dispatch explicitly; this state is never stored in the host mailbox.
authoring_executor :: proc(app:^Authoring)->editor.Application_Executor { return {app,execute_owned} }
/// Restores one agent history step only while authored state can be edited.
authoring_undo_last :: proc(app:^Authoring)->editor.Scene_Error {
    if app.mode!=.Editing { return .Editing_Required }
    return editor.agent_undo_last(&app.agent.session,&app.world,&app.registry)
}
@(private="package")
error_result :: proc(w:^ecs.World,error:editor.Scene_Error)->editor.Tool_Result {
    return {error=error,entities=make([dynamic]ecs.Entity_Id,w.allocator),allocator=w.allocator}
}
@(private="package")
execute_owned :: proc(state:rawptr,w:^ecs.World,reg:^editor.Component_Registry,op:editor.Scene_Op)->(editor.Tool_Result,editor.Undo_Group) {
    app:=cast(^Authoring)state
    assert(w==&app.world && reg==&app.registry)
    if op.kind==.Application {
        if op.tool_name!="material" { return error_result(w,.Application_Owned),{} }
        decoded,err:=agent.decode_material(op.value,w.allocator)
        if err!=.None { return error_result(w,.Invalid_Operation),{} }; defer agent.decoded_material_destroy(&decoded)
        return material_execute(app,decoded.operation)
    }
    mutation:=op.kind in bit_set[editor.Scene_Op_Kind]{.Spawn,.Destroy,.Set_Field,.Duplicate,.Add_Component,.Remove_Component,.Set_Parent,.Spawn_Model}
    if mutation && app.mode!=.Editing { return error_result(w,.Editing_Required),{} }
    if op.kind in (bit_set[editor.Scene_Op_Kind]{.Destroy,.Set_Field,.Duplicate,.Add_Component,.Remove_Component,.Get_Attributes,.Set_Parent}) {
        _,protected:=ecs.get_component(w,op.entity,Editor_Hidden)
        if protected { return error_result(w,.Protected_Entity),{} }
    }
    return editor.scene_execute(w,reg,op)
}
