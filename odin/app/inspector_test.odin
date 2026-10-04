#+test
package app

import ecs "../ecs"
import editor "../editor"
import "core:testing"
import "core:mem"

@(test)
test_inspector_material_gesture_shared_history_and_exact_redo :: proc(t:^testing.T) {
    app:Authoring; authoring_init(&app); defer authoring_destroy(&app)
    original:=Surface_Material{linear_color={0.19,0.27,0.43,0.63},has_tint=true,roughness=0.73,metallic=0.11,ao=0.81}
    id:=ecs.spawn(&app.world,struct { surface:Surface_Material, position:Position }{original,Position{4}})
    gesture:Material_Gesture; defer material_gesture_destroy(&gesture)
    testing.expect_value(t,material_gesture_begin(&app,&gesture,{id}),editor.Scene_Error.None)
    for roughness in ([3]f32{0.3,0.6,0.1}) { testing.expect_value(t,material_gesture_preview(&app,&gesture,{.Roughness},{roughness=roughness}),editor.Scene_Error.None) }
    testing.expect_value(t,len(app.agent.session.actions),0)
    ecs.get_component_mut(&app.world,id,Position).x=7
    testing.expect_value(t,material_gesture_finish(&app,&gesture),editor.Scene_Error.None)
    testing.expect(t,!gesture.active && len(app.agent.session.actions)==1)
    testing.expect_value(t,authoring_undo_last(&app),editor.Scene_Error.None)
    restored,_:=ecs.get_component(&app.world,id,Surface_Material); testing.expect_value(t,restored,original)
    testing.expect_value(t,authoring_redo_last(&app),editor.Scene_Error.None)
    after,_:=ecs.get_component(&app.world,id,Surface_Material)
    testing.expect(t,after.roughness==0.1 && after.linear_color==original.linear_color && after.has_tint)
    position,_:=ecs.get_component(&app.world,id,Position); testing.expect_value(t,position.x,f32(7))
    testing.expect_value(t,authoring_undo_last(&app),editor.Scene_Error.None)
    restored,_=ecs.get_component(&app.world,id,Surface_Material); testing.expect_value(t,restored,original)
}

@(test)
test_inspector_material_gesture_cancel_invalid_batch_and_history_conflict :: proc(t:^testing.T) {
    app:Authoring; authoring_init(&app); defer authoring_destroy(&app)
    a:=ecs.spawn(&app.world,struct { surface:Surface_Material }{Surface_Material{roughness=0.5,ao=1}})
    b:=ecs.spawn(&app.world,struct { surface:Surface_Material }{Surface_Material{roughness=0.7,ao=1}})
    gesture:Material_Gesture; defer material_gesture_destroy(&gesture)
    testing.expect_value(t,material_gesture_begin(&app,&gesture,{a,a}),editor.Scene_Error.Invalid_Operation)
    testing.expect(t,!gesture.active)
    testing.expect_value(t,material_gesture_begin(&app,&gesture,{a,b}),editor.Scene_Error.None)
    testing.expect_value(t,material_gesture_preview(&app,&gesture,{.Roughness},{roughness=1.1}),editor.Scene_Error.Invalid_Operation)
    unchanged,_:=ecs.get_component(&app.world,a,Surface_Material); testing.expect_value(t,unchanged.roughness,f32(0.5))
    testing.expect_value(t,material_gesture_preview(&app,&gesture,{.Roughness},{roughness=0.1}),editor.Scene_Error.None)
    testing.expect_value(t,material_gesture_cancel(&app,&gesture),editor.Scene_Error.None)
    unchanged,_=ecs.get_component(&app.world,a,Surface_Material); testing.expect_value(t,unchanged.roughness,f32(0.5))
    other,_:=ecs.get_component(&app.world,b,Surface_Material); testing.expect_value(t,other.roughness,f32(0.7))
    testing.expect_value(t,len(app.agent.session.actions),0)
    testing.expect_value(t,material_gesture_begin(&app,&gesture,{a,b}),editor.Scene_Error.None)
    testing.expect_value(t,material_gesture_preview(&app,&gesture,{.AO},{ao=0.2}),editor.Scene_Error.None)
    ecs.remove_component(&app.world,b,Surface_Material)
    testing.expect_value(t,material_gesture_finish(&app,&gesture),editor.Scene_Error.Component_Not_Found)
    testing.expect_value(t,material_gesture_cancel(&app,&gesture),editor.Scene_Error.Component_Not_Found)
    after,_:=ecs.get_component(&app.world,a,Surface_Material); testing.expect_value(t,after.ao,f32(0.2))
    ecs.add_component(&app.world,b,other)
    app.agent.session.next_id+=1
    testing.expect_value(t,material_gesture_finish(&app,&gesture),editor.Scene_Error.Invalid_Operation)
}

@(test)
test_inspector_material_gesture_noop_and_allocator_ownership :: proc(t:^testing.T) {
    backing:=context.allocator; tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,backing)
    defer { testing.expect_value(t,len(tracker.allocation_map),0); mem.tracking_allocator_destroy(&tracker) }
    context.allocator=mem.tracking_allocator(&tracker)
    app:Authoring; authoring_init(&app); defer authoring_destroy(&app)
    id:=ecs.spawn(&app.world,struct { surface:Surface_Material }{Surface_Material{roughness=0.5,ao=1}})
    gesture:Material_Gesture; defer material_gesture_destroy(&gesture)
    testing.expect_value(t,material_gesture_begin(&app,&gesture,{id}),editor.Scene_Error.None)
    testing.expect_value(t,material_gesture_preview(&app,&gesture,{.Roughness},{roughness=0.5}),editor.Scene_Error.None)
    testing.expect_value(t,material_gesture_finish(&app,&gesture),editor.Scene_Error.None)
    testing.expect_value(t,len(app.agent.session.actions),0)
}
