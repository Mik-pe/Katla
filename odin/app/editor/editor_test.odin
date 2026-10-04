package editor_app

import app ".."
import ecs "../../ecs"
import editor "../../editor"
import km "../../math"
import "core:strings"
import "core:testing"

@(test)
test_zero_entity_multiselection_collapsed_search_and_range :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    app.scene_components_register(&owner)
    root:=ecs.create_entity(&owner.world); testing.expect_value(t,root,ecs.Entity_Id(0))
    ecs.add_component(&owner.world,root,app.Scene_Name{strings.clone("Environment")})
    child:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,child,app.Scene_Name{strings.clone("Spotlight")}); ecs.add_component(&owner.world,child,app.Scene_Parent{root})
    other:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,other,app.Scene_Name{strings.clone("Other")})
    hidden:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,hidden,app.Editor_Hidden{})
    state:State; state_init(&state,&owner); defer state_destroy(&state)
    hierarchy_refresh(&state); testing.expect_value(t,len(state.rows),2)
    testing.expect(t,selection_set(&state,root) && state.selection.has_primary && state.selection.primary==0)
    testing.expect(t,!selection_set(&state,hidden))
    selection_set(&state,other,.Toggle); testing.expect_value(t,len(state.selection.entries),2)
    selection_set(&state,other,.Toggle); testing.expect(t,state.selection.has_primary && state.selection.primary==root)
    search_set(&state,"SPOT"); hierarchy_refresh(&state)
    testing.expect(t,len(state.rows)==2 && state.rows[0].entity==root && state.rows[1].entity==child)
    selection_set(&state,root); selection_set(&state,child,.Range); testing.expect_value(t,len(state.selection.entries),2)
    search_set(&state,""); selection_reveal(&state,child); hierarchy_refresh(&state); testing.expect_value(t,len(state.rows),3)
}

@(test)
test_selection_identity_replacement_and_corrupt_cycle_recovery :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    app.scene_components_register(&owner)
    original:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,original,app.Scene_Key{max(u64)})
    state:State; state_init(&state,&owner); defer state_destroy(&state)
    selection_set(&state,original)
    ecs.destroy_entity(&owner.world,original)
    replacement:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,replacement,app.Scene_Key{max(u64)})
    selection_refresh(&state); testing.expect(t,state.selection.has_primary && state.selection.primary==replacement && replacement!=original)
    second:=ecs.create_entity(&owner.world)
    ecs.add_component(&owner.world,replacement,app.Scene_Parent{second}); ecs.add_component(&owner.world,second,app.Scene_Parent{replacement})
    state.expanded[replacement]=true; state.expanded[second]=true
    hierarchy_refresh(&state); testing.expect(t,len(state.rows)==2 && state.rows[0].orphan)
    testing.expect(t,state.rows[0].entity!=state.rows[1].entity)
    ecs.destroy_entity(&owner.world,replacement); selection_refresh(&state); testing.expect(t,!state.selection.has_primary)
}

@(test)
test_nested_inspector_control_uses_canonical_shared_undo_and_stale_rejection :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    app.scene_components_register(&owner)
    id:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,id,app.Scene_Transform{km.TRANSFORM_IDENTITY}); ecs.add_component(&owner.world,id,app.Scene_Key{1})
    state:State; state_init(&state,&owner); defer state_destroy(&state); selection_set(&state,id)
    snapshot,error:=inspector_read(&state); testing.expect_value(t,error,editor.Scene_Error.None); defer inspector_destroy(&snapshot)
    found_position:=false; exposed_key:=false
    for component in snapshot.components {
        if component.name=="SceneKey" { exposed_key=true }
        for field in component.fields { if field.path=="/local/position/0" { found_position=true } }
    }
    testing.expect(t,found_position && !exposed_key)
    changed:=inspector_set(&state,id,"SceneTransform","/local/position/0",transmute([]byte)string("4.5"))
    testing.expect_value(t,changed,editor.Scene_Error.None)
    transform,_:=ecs.get_component(&owner.world,id,app.Scene_Transform); testing.expect_value(t,transform.local.position[0],f32(4.5))
    testing.expect(t,editor.agent_can_undo(&owner.agent.session))
    testing.expect_value(t,history_apply(&state,false),editor.Scene_Error.None)
    transform,_=ecs.get_component(&owner.world,id,app.Scene_Transform); testing.expect_value(t,transform.local.position[0],f32(0))
    testing.expect_value(t,history_apply(&state,true),editor.Scene_Error.None)
    transform,_=ecs.get_component(&owner.world,id,app.Scene_Transform); testing.expect_value(t,transform.local.position[0],f32(4.5))
    testing.expect_value(t,inspector_set(&state,id,"SceneTransform","/local/no_such_field",transmute([]byte)string("2")),editor.Scene_Error.Field_Not_Found)
    ecs.destroy_entity(&owner.world,id)
    testing.expect_value(t,inspector_set(&state,id,"SceneTransform","/local/position/0",transmute([]byte)string("2")),editor.Scene_Error.Entity_Not_Found)
}
