#+test
package editor_app
import app ".."
import editor "../../editor"
import ecs "../../ecs"
import "core:testing"
import "core:strings"

Inspector_Unsigned :: struct { count,untouched:u64 }
@(test)
test_optional_components_are_addable_and_exact_uint_inspector_operation_preserves_history :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner); app.authoring_services_init(&owner)
    editor.editor_register(&owner.world,&owner.registry,"ExactUnsigned",Inspector_Unsigned{},spawn_default=false)
    id:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,id,Inspector_Unsigned{max(u64)-1,max(u64)})
    state:State; state_init(&state,&owner); defer state_destroy(&state); selection_set(&state,id)
    snapshot,error:=inspector_read(&state); testing.expect_value(t,error,editor.Scene_Error.None); defer inspector_destroy(&snapshot)
    lights,audio,physics:=false,false,false
    for name in snapshot.available { if name=="PointLight" { lights=true }; if name=="AudioEmitter" { audio=true }; if name=="PhysicsBody" { physics=true }; testing.expect(t,name!="SceneKey" && name!="SceneMesh" && name!="AnimationModel") }
    testing.expect(t,lights && audio && physics)
    exact:=false
    for component in snapshot.components { if component.name=="ExactUnsigned" { for field in component.fields { if field.path=="/count" { exact=string(field.value)==`"18446744073709551614"` } } } }; testing.expect(t,exact)
    wire:string=`"18446744073709551615"`
    operation,op_error:=inspector_operation(&state,id,"ExactUnsigned","/count",transmute([]byte)wire); defer inspector_operation_destroy(&operation,state.allocator)
    testing.expect_value(t,op_error,editor.Scene_Error.None); testing.expect(t,strings.contains(string(operation.value),"18446744073709551615"))
    testing.expect_value(t,execute(&state,operation),editor.Scene_Error.None)
    value,_:=ecs.get_component(&owner.world,id,Inspector_Unsigned); testing.expect(t,value.count==max(u64) && value.untouched==max(u64))
    testing.expect_value(t,history_apply(&state,false),editor.Scene_Error.None)
    restored,_:=ecs.get_component(&owner.world,id,Inspector_Unsigned); testing.expect(t,restored.count==max(u64)-1 && restored.untouched==max(u64))
    bad:string=`"18446744073709551616"`; rejected,reject_error:=inspector_operation(&state,id,"ExactUnsigned","/count",transmute([]byte)bad); defer inspector_operation_destroy(&rejected,state.allocator); testing.expect_value(t,reject_error,editor.Scene_Error.Invalid_Field_Value)
    testing.expect_value(t,execute(&state,{kind=.Add_Component,entity=id,component="PointLight"}),editor.Scene_Error.None)
    light,present:=ecs.get_component(&owner.world,id,app.Scene_Point_Light); testing.expect(t,present && light.range==10)
    added,read_error:=inspector_read(&state); defer inspector_destroy(&added); testing.expect_value(t,read_error,editor.Scene_Error.None)
    removable:=false; for component in added.components { if component.name=="PointLight" { removable=component.removable } }; testing.expect(t,removable)
    testing.expect_value(t,execute(&state,{kind=.Remove_Component,entity=id,component="PointLight"}),editor.Scene_Error.None)
    testing.expect_value(t,history_apply(&state,false),editor.Scene_Error.None); _,retained:=ecs.get_component(&owner.world,id,app.Scene_Point_Light); testing.expect(t,retained)
}
