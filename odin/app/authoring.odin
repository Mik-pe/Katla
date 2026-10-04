//! Scene authoring composition owns the world, editor history and application tools.
package app

import ecs "../ecs"
import editor "../editor"

/// Protects editor-owned entities from targeted scene and material tools.
Editor_Hidden :: struct {}
/// Authoring mutations require editing; preview lifecycle migration remains separate.
Play_Mode :: enum { Editing, Playing, Paused }
/// A stationary scene owner; producers receive only its synchronized agent mailbox.
Authoring :: struct {
    world:ecs.World,
    registry:editor.Component_Registry,
    agent:editor.Agent_Harness,
    mode:Play_Mode,
    before_mutation_state:rawptr,
    before_mutation:proc(rawptr)->editor.Scene_Error,
}
/// Finishes pending application edits before a different authored mutation is admitted.
authoring_before_mutation :: proc(app:^Authoring)->editor.Scene_Error {
    if app.before_mutation!=nil { return app.before_mutation(app.before_mutation_state) }
    return .None
}
/// Initializes CPU authoring state without installing graphics or network services.
authoring_init :: proc(app:^Authoring,allocator:=context.allocator,agent_capacity:=256) {
    ecs.world_init(&app.world,allocator=allocator)
    editor.editor_registry_init(&app.registry,allocator)
    editor.editor_register(&app.world,&app.registry,"SurfaceMaterial",Surface_Material{metallic=0,roughness=0.5,ao=1})
    ecs.register_component(&app.world,Editor_Hidden)
    editor.agent_harness_init(&app.agent,allocator,agent_capacity)
    scene_action_restore_install(app)
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
authoring_executor :: proc(app:^Authoring)->editor.Application_Executor { return {state=app,execute=execute_owned} }
/// Restores one agent history step only while authored state can be edited.
authoring_undo_last :: proc(app:^Authoring)->editor.Scene_Error {
    if app.mode!=.Editing { return .Editing_Required }
    if error:=authoring_before_mutation(app); error!=.None { return error }
    return editor.agent_undo_last(&app.agent.session,&app.world,&app.registry)
}
/// Reapplies shared authored history only while the application is editing.
authoring_redo_last :: proc(app:^Authoring)->editor.Scene_Error {
    if app.mode!=.Editing { return .Editing_Required }
    if error:=authoring_before_mutation(app); error!=.None { return error }
    return editor.agent_redo_last(&app.agent.session,&app.world,&app.registry)
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
        return authoring_application(app,op)
    }
    if app.registry.entries["SceneTransform"]!=nil { return scene_action_execute(app,op) }
    mutation:=op.kind in bit_set[editor.Scene_Op_Kind]{.Spawn,.Destroy,.Set_Field,.Duplicate,.Add_Component,.Remove_Component,.Set_Parent,.Spawn_Model}
    if mutation && app.mode!=.Editing { return error_result(w,.Editing_Required),{} }
    if mutation { if error:=authoring_before_mutation(app); error!=.None { return error_result(w,error),{} } }
    if op.kind in (bit_set[editor.Scene_Op_Kind]{.Destroy,.Set_Field,.Duplicate,.Add_Component,.Remove_Component,.Get_Attributes,.Set_Parent}) {
        _,protected:=ecs.get_component(w,op.entity,Editor_Hidden)
        if protected { return error_result(w,.Protected_Entity),{} }
    }
    return editor.scene_execute(w,reg,op)
}
