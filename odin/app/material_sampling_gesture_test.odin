#+test
package app

import ecs "../ecs"
import editor "../editor"
import "core:testing"

Sampling_Test_Gate :: struct {owner:^Authoring,gesture:^Material_Sampling_Gesture,calls:int}
sampling_test_gate :: proc(state:rawptr)->editor.Scene_Error {
    gate:=cast(^Sampling_Test_Gate)state; gate.calls+=1
    if gate.gesture.scene.active { return material_sampling_gesture_finish(gate.owner,gate.gesture) }; return .None
}
@(test)
test_material_sampling_gesture_one_history_preserves_other_components_and_gate :: proc(t:^testing.T) {
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner)
    testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None)
    geometry,error:=mesh_cube({1,1,1}); testing.expect(t,error==.None)
    entity:=ecs.spawn(&owner.world,struct{mesh:Scene_Mesh,surface:Surface_Material,transform:Scene_Transform}{ {geometry=geometry},{roughness=.5,ao=1},{} })
    gesture:Material_Sampling_Gesture; defer material_sampling_gesture_destroy(&gesture)
    gate:=Sampling_Test_Gate{&owner,&gesture,0}; owner.before_mutation_state=&gate; owner.before_mutation=sampling_test_gate
    testing.expect_value(t,material_sampling_gesture_begin(&owner,&gesture,{entity},.Albedo),editor.Scene_Error.None)
    begin_calls:=gate.calls
    testing.expect_value(t,material_sampling_gesture_preview(&owner,&gesture,{fields={.Rotation},rotation=.5}),editor.Scene_Error.None)
    testing.expect_value(t,material_sampling_gesture_preview(&owner,&gesture,{fields={.Rotation},rotation=.75}),editor.Scene_Error.None)
    testing.expect(t,gate.calls==begin_calls && len(owner.agent.session.actions)==0)
    ecs.get_component_mut(&owner.world,entity,Scene_Transform).local.position={2,3,4}
    testing.expect_value(t,material_sampling_gesture_finish(&owner,&gesture),editor.Scene_Error.None)
    testing.expect(t,len(owner.agent.session.actions)==1)
    testing.expect_value(t,authoring_undo_last(&owner),editor.Scene_Error.None)
    surface,_:=ecs.get_component(&owner.world,entity,Surface_Material); transform,_:=ecs.get_component(&owner.world,entity,Scene_Transform)
    testing.expect(t,!surface.has_sampling && transform.local.position==[3]f32{2,3,4})
    testing.expect_value(t,authoring_redo_last(&owner),editor.Scene_Error.None)
    surface,_=ecs.get_component(&owner.world,entity,Surface_Material); testing.expect(t,surface.sampling.albedo.uv.rotation==.75)
    testing.expect_value(t,material_sampling_gesture_begin(&owner,&gesture,{entity},.Albedo),editor.Scene_Error.None)
    testing.expect_value(t,material_sampling_gesture_preview(&owner,&gesture,{fields={.Rotation},rotation=1}),editor.Scene_Error.None)
    testing.expect_value(t,material_sampling_gesture_cancel(&owner,&gesture),editor.Scene_Error.None)
    surface,_=ecs.get_component(&owner.world,entity,Surface_Material); testing.expect(t,surface.sampling.albedo.uv.rotation==.75 && len(owner.agent.session.actions)==1)
}
