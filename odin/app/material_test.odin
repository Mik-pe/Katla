#+test
package app

import ecs "../ecs"
import editor "../editor"
import agent "../agent"
import km "../math"
import "core:testing"
import "core:fmt"
import "core:encoding/json"
import "core:mem"

@(test)
test_material_batch_preflight_and_exact_undo_redo :: proc(t:^testing.T) {
    app:Authoring; authoring_init(&app); defer authoring_destroy(&app)
    a:=ecs.spawn(&app.world,struct { surface:Surface_Material }{Surface_Material{roughness=0.5,ao=1}})
    b:=ecs.create_entity(&app.world)
    op:=agent.Material_Set{entities={a,b},fields={.Roughness},values={roughness=0.2}}
    bad,no_undo:=material_execute(&app,op); defer editor.tool_result_destroy(&bad); defer editor.undo_group_destroy(&no_undo)
    testing.expect_value(t,bad.error,editor.Scene_Error.Component_Not_Found)
    surface,_:=ecs.get_component(&app.world,a,Surface_Material)
    testing.expect(t,surface.roughness==0.5 && !surface.has_tint)
    ecs.add_component(&app.world,b,Surface_Material{linear_color={0.17,0.31,0.44,0.71},has_tint=true,roughness=0.7,ao=0.6})
    before,_:=ecs.get_component(&app.world,b,Surface_Material)
    result,group:=material_execute(&app,op); defer editor.tool_result_destroy(&result); defer editor.undo_group_destroy(&group)
    testing.expect(t,result.error==.None && len(group.entities)==2 && len(result.entities)==2)
    after,_:=ecs.get_component(&app.world,b,Surface_Material)
    testing.expect(t,after.linear_color==before.linear_color && after.has_tint && after.roughness==0.2)
    testing.expect_value(t,editor.undo_group(&app.world,&app.registry,&group),editor.Scene_Error.None)
    surface,_=ecs.get_component(&app.world,a,Surface_Material); testing.expect(t,surface.roughness==0.5 && !surface.has_tint)
    restored,_:=ecs.get_component(&app.world,b,Surface_Material); testing.expect_value(t,restored,before)
    testing.expect_value(t,editor.redo_group(&app.world,&app.registry,&group),editor.Scene_Error.None)
    after,_=ecs.get_component(&app.world,b,Surface_Material); testing.expect(t,after.roughness==0.2 && after.linear_color==before.linear_color)
    ecs.remove_component(&app.world,b,Surface_Material)
    testing.expect_value(t,editor.undo_group(&app.world,&app.registry,&group),editor.Scene_Error.Component_Not_Found)
    surface,_=ecs.get_component(&app.world,a,Surface_Material); testing.expect_value(t,surface.roughness,f32(0.2))
}

Position :: struct { x:f32 }
@(test)
test_material_shared_history_recreated_generations_and_unrelated_state :: proc(t:^testing.T) {
    app:Authoring; authoring_init(&app); defer authoring_destroy(&app)
    editor.editor_register(&app.world,&app.registry,"Position",Position{})
    spawned:=editor.agent_execute(&app.agent.session,&app.world,&app.registry,{kind=.Spawn},authoring_executor(&app))
    id:=spawned.result.entities[0]
    args:=fmt.aprintf(`{{"action":"set","entity_ids":["%d"],"preset":"oak","roughness":0.3}}`,u64(id)); defer delete(args)
    testing.expect_value(t,agent.submit_call(&app.agent,{"material-1","material",transmute([]byte)args}),agent.Call_Error.None)
    testing.expect_value(t,authoring_tick(&app),1)
    response,ok:=editor.agent_take_result(&app.agent); testing.expect(t,ok && response.result.error==.None); editor.tool_result_destroy(&response.result)
    surface,_:=ecs.get_component(&app.world,id,Surface_Material)
    expected:=agent.material_preset_values(.Oak)
    actual:=material_values(surface); testing.expect(t,abs(actual.base_color[0]-expected.base_color[0])<0.00001 && surface.roughness==0.3)
    editor.agent_execute(&app.agent.session,&app.world,&app.registry,{kind=.Destroy,entity=id},authoring_executor(&app))
    testing.expect_value(t,authoring_undo_last(&app),editor.Scene_Error.None)
    current:=app.agent.session.actions[1].undo.entities[0]
    testing.expect(t,current!=id && !ecs.entity_exists(&app.world,id) && ecs.entity_exists(&app.world,current))
    ecs.get_component_mut(&app.world,current,Position).x=9
    testing.expect_value(t,authoring_undo_last(&app),editor.Scene_Error.None)
    surface,_=ecs.get_component(&app.world,current,Surface_Material); testing.expect(t,!surface.has_tint && surface.roughness==0.5)
    position,_:=ecs.get_component(&app.world,current,Position); testing.expect_value(t,position.x,f32(9))
    testing.expect_value(t,authoring_undo_last(&app),editor.Scene_Error.None)
    testing.expect_value(t,app.world.live_count,0)
}

@(test)
test_material_edit_mode_protection_and_generic_field_boundary :: proc(t:^testing.T) {
    app:Authoring; authoring_init(&app); defer authoring_destroy(&app)
    id:=ecs.spawn(&app.world,struct { surface:Surface_Material }{Surface_Material{roughness=0.5,ao=1}})
    app.mode=.Playing
    result,group:=material_execute(&app,agent.Material_Set{entities={id},fields={.Roughness},values={roughness=0.2}})
    testing.expect_value(t,result.error,editor.Scene_Error.Editing_Required); editor.tool_result_destroy(&result); editor.undo_group_destroy(&group)
    app.mode=.Editing; ecs.add_component(&app.world,id,Editor_Hidden{})
    result,group=material_execute(&app,agent.Material_Inspect{id})
    testing.expect_value(t,result.error,editor.Scene_Error.Protected_Entity); editor.tool_result_destroy(&result); editor.undo_group_destroy(&group)
    destroyed:=editor.agent_execute(&app.agent.session,&app.world,&app.registry,{kind=.Destroy,entity=id},authoring_executor(&app))
    testing.expect(t,destroyed.result.error==.Protected_Entity && ecs.entity_exists(&app.world,id))
    ecs.remove_component(&app.world,id,Editor_Hidden)
    field:=editor.agent_execute(&app.agent.session,&app.world,&app.registry,{kind=.Set_Field,entity=id,component="SurfaceMaterial",field="roughness",value=transmute([]byte)string("-2")},authoring_executor(&app))
    testing.expect_value(t,field.result.error,editor.Scene_Error.Field_Not_Found)
    surface,_:=ecs.get_component(&app.world,id,Surface_Material); testing.expect_value(t,surface.roughness,f32(0.5))
}

@(test)
test_material_presets_and_response_ownership :: proc(t:^testing.T) {
    backing:=context.allocator
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,backing); defer mem.tracking_allocator_destroy(&tracker)
    context.allocator=mem.tracking_allocator(&tracker)
    app:Authoring; authoring_init(&app)
    result,empty:=material_execute(&app,agent.Material_Presets{})
    tree,err:=json.parse(result.data); testing.expect_value(t,err,json.Error.None)
    object,ok:=tree.(json.Object); testing.expect(t,ok)
    presets:=object["presets"].(json.Array); testing.expect_value(t,len(presets),6)
    json.destroy_value(tree); editor.tool_result_destroy(&result); editor.undo_group_destroy(&empty)
    id:=ecs.spawn(&app.world,struct { surface:Surface_Material }{Surface_Material{roughness=0.5,ao=1}})
    args:=fmt.aprintf(`{{"action":"set","entity_ids":["%d"],"base_color":[0.5,0.3,0.2,0.7]}}`,u64(id))
    testing.expect_value(t,agent.submit_call(&app.agent,{"tint","material",transmute([]byte)args}),agent.Call_Error.None); delete(args)
    authoring_tick(&app)
    response,has_response:=editor.agent_take_result(&app.agent); testing.expect(t,has_response && response.result.error==.None); editor.tool_result_destroy(&response.result)
    surface,_:=ecs.get_component(&app.world,id,Surface_Material)
    testing.expect(t,surface.linear_color==km.color_to_linear({0.5,0.3,0.2,0.7}) && surface.has_tint)
    authoring_destroy(&app)
    context.allocator=backing
    testing.expect_value(t,len(tracker.allocation_map),0)
}
@(test)
test_material_maximum_batch_and_invalid_direct_patch :: proc(t:^testing.T) {
    app:Authoring; authoring_init(&app); defer authoring_destroy(&app)
    ids:[256]ecs.Entity_Id
    for &id in ids { id=ecs.spawn(&app.world,struct { surface:Surface_Material }{Surface_Material{roughness=0.5,ao=1}}) }
    op:=agent.Material_Set{entities=ids[:],fields={.Roughness},values={roughness=-0.2}}
    rejected,none:=material_execute(&app,op)
    testing.expect_value(t,rejected.error,editor.Scene_Error.Invalid_Operation); editor.tool_result_destroy(&rejected); editor.undo_group_destroy(&none)
    op.values.roughness=0.37
    result,group:=material_execute(&app,op); defer editor.tool_result_destroy(&result); defer editor.undo_group_destroy(&group)
    testing.expect(t,result.error==.None && len(result.entities)==256)
    for id in ids { surface,_:=ecs.get_component(&app.world,id,Surface_Material); testing.expect_value(t,surface.roughness,f32(0.37)) }
    testing.expect_value(t,editor.undo_group(&app.world,&app.registry,&group),editor.Scene_Error.None)
    for id in ids { surface,_:=ecs.get_component(&app.world,id,Surface_Material); testing.expect_value(t,surface.roughness,f32(0.5)) }
}
@(test)
test_material_stale_id_cannot_modify_replacement_entity :: proc(t:^testing.T) {
    app:Authoring; authoring_init(&app); defer authoring_destroy(&app)
    old:=ecs.spawn(&app.world,struct { surface:Surface_Material }{Surface_Material{roughness=0.5,ao=1}})
    ecs.destroy_entity(&app.world,old)
    current:=ecs.spawn(&app.world,struct { surface:Surface_Material }{Surface_Material{roughness=0.7,ao=1}})
    testing.expect(t,current!=old)
    result,none:=material_execute(&app,agent.Material_Set{entities={old},fields={.Roughness},values={roughness=0.2}})
    testing.expect_value(t,result.error,editor.Scene_Error.Entity_Not_Found); editor.tool_result_destroy(&result); editor.undo_group_destroy(&none)
    surface,_:=ecs.get_component(&app.world,current,Surface_Material); testing.expect_value(t,surface.roughness,f32(0.7))
}
