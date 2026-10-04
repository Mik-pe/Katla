#+test
package app

import editor "../editor"
import ecs "../ecs"
import "core:testing"

@(test)
test_scene_action_remove_and_add_default_surface_preserves_identity_and_reversible_history :: proc(t:^testing.T) {
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner)
    testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None)
    spawn:=editor.Scene_Op{kind=.Spawn,name="Surface roundtrip",position={1,2,3},shape="sphere",scale={1,1,1}}
    result,group:=scene_action_execute(&owner,spawn); testing.expect_value(t,result.error,editor.Scene_Error.None); assert(result.error==.None)
    entity:=result.entities[0]; editor.agent_record_action(&owner.agent.session,spawn,&result,&group)
    original,present:=ecs.get_component(&owner.world,entity,Surface_Material); testing.expect(t,present)
    remove:=editor.Scene_Op{kind=.Remove_Component,entity=entity,component="SurfaceMaterial"}
    result,group=scene_action_execute(&owner,remove); testing.expect_value(t,result.error,editor.Scene_Error.None); assert(result.error==.None); editor.agent_record_action(&owner.agent.session,remove,&result,&group)
    _,present=ecs.get_component(&owner.world,entity,Surface_Material); testing.expect(t,!present)
    add:=editor.Scene_Op{kind=.Add_Component,entity=entity,component="SurfaceMaterial"}
    result,group=scene_action_execute(&owner,add); testing.expect_value(t,result.error,editor.Scene_Error.None)
    if result.error!=.None { editor.tool_result_destroy(&result); editor.undo_group_destroy(&group); return }
    editor.agent_record_action(&owner.agent.session,add,&result,&group)
    added,added_present:=ecs.get_component(&owner.world,entity,Surface_Material); testing.expect(t,added_present && added.roughness==.5 && added.ao==1 && !added.has_tint)
    testing.expect_value(t,authoring_undo_last(&owner),editor.Scene_Error.None)
    _,present=ecs.get_component(&owner.world,entity,Surface_Material); testing.expect(t,!present)
    testing.expect_value(t,authoring_undo_last(&owner),editor.Scene_Error.None)
    restored,restored_present:=ecs.get_component(&owner.world,entity,Surface_Material); testing.expect(t,restored_present && restored==original)
    testing.expect_value(t,authoring_redo_last(&owner),editor.Scene_Error.None)
    _,present=ecs.get_component(&owner.world,entity,Surface_Material); testing.expect(t,!present)
    testing.expect_value(t,authoring_redo_last(&owner),editor.Scene_Error.None)
    restored,present=ecs.get_component(&owner.world,entity,Surface_Material); testing.expect(t,present && restored==added && ecs.entity_exists(&owner.world,entity))
}

@(test)
test_scene_action_remove_rejects_real_changed_surface_without_mutating_live_state :: proc(t:^testing.T) {
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner)
    testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None)
    spawn:=editor.Scene_Op{kind=.Spawn,name="Conflict",shape="sphere",scale={1,1,1}}
    created,creation:=scene_action_execute(&owner,spawn); defer editor.tool_result_destroy(&created); defer editor.undo_group_destroy(&creation)
    testing.expect_value(t,created.error,editor.Scene_Error.None); if created.error!=.None { return }
    entity:=created.entities[0]
    removed,history:=scene_action_execute(&owner,editor.Scene_Op{kind=.Remove_Component,entity=entity,component="SurfaceMaterial"})
    defer editor.tool_result_destroy(&removed); defer editor.undo_group_destroy(&history)
    testing.expect_value(t,removed.error,editor.Scene_Error.None); if removed.error!=.None { return }
    testing.expect_value(t,editor.undo_group(&owner.world,&owner.registry,&history),editor.Scene_Error.None)
    live:=ecs.get_component_mut(&owner.world,entity,Surface_Material); if !testing.expect(t,live!=nil) { return }; live.roughness=.37
    before:=live^
    testing.expect_value(t,editor.redo_group(&owner.world,&owner.registry,&history),editor.Scene_Error.Invalid_Operation)
    after,present:=ecs.get_component(&owner.world,entity,Surface_Material); testing.expect(t,present && before==after)
}

@(test)
test_scene_action_component_conflicts_keep_exact_unsigned_reference_values :: proc(t:^testing.T) {
    entry:=editor.Editor_Entry{T=Scene_Key}
    first:=Scene_Key{9007199254740992}; adjacent:=Scene_Key{9007199254740993}
    testing.expect(t,!scene_action_component_equal(&entry,&first,&adjacent,context.allocator))
    first.value=max(u64); adjacent.value=max(u64)-1
    testing.expect(t,!scene_action_component_equal(&entry,&first,&adjacent,context.allocator))
    testing.expect(t,scene_action_component_equal(&entry,&first,&first,context.allocator))
}
