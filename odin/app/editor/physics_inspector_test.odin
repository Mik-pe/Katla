#+test
//! Actual registered physics controls edit owned scene values through common admission and history.
package editor_app
import app ".."
import ecs "../../ecs"
import editor "../../editor"
import km "../../math"
import "core:testing"
import "core:mem"

@(test)
test_physics_inspector_add_defaults_choices_dimensions_filters_and_history :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:app.Authoring; app.authoring_init(&owner); testing.expect_value(t,app.authoring_services_init(&owner),editor.Scene_Error.None)
    id:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,id,app.Scene_Transform{km.TRANSFORM_IDENTITY})
    state:State; state_init(&state,&owner); selection_set(&state,id)
    testing.expect_value(t,execute(&state,{kind=.Add_Component,entity=id,component="PhysicsBody"}),editor.Scene_Error.None)
    body,present:=ecs.get_component(&owner.world,id,app.Physics_Body)
    testing.expect(t,present && body.has_rigid_body && body.has_collider && body.has_material && body.has_filter && body.body_type==.Dynamic && body.shape.kind==.Box && body.shape.half_extents==([3]f32{.5,.5,.5}) && app.physics_body_valid(body))
    snapshot,error:=inspector_read(&state); testing.expect_value(t,error,editor.Scene_Error.None)
    paths:=make(map[string]bool); choices:=0
    for component in snapshot.components { if component.name!="PhysicsBody" { continue }
        for field in component.fields {
            paths[field.path]=true
            if field.path=="/body_type" || field.path=="/shape/kind" { choices+=1; testing.expect(t,field.kind==.Enum && len(field.variants)>0) }
        }
    }
    for path in ([14]string{"/has_rigid_body","/has_collider","/has_material","/has_filter","/body_type","/shape/kind","/shape/half_extents/0","/shape/radius","/gravity_scale","/linear_velocity/0","/friction","/restitution","/density","/mask"}) { testing.expect(t,paths[path]) }
    testing.expect(t,choices==2 && !paths["/shape/heights/0"] && !paths["/shape/rows"] && !paths["/shape/cols"])
    delete(paths); inspector_destroy(&snapshot)
    for edit in ([4]struct {path,value:string}{{"/body_type",`"Kinematic"`},{"/shape/half_extents/0","2"},{"/friction","0.75"},{"/mask",`"2147483649"`}}) { testing.expect_value(t,inspector_set(&state,id,"PhysicsBody",edit.path,transmute([]byte)edit.value),editor.Scene_Error.None) }
    updated,_:=ecs.get_component(&owner.world,id,app.Physics_Body)
    testing.expect(t,updated.body_type==.Kinematic && updated.shape.half_extents[0]==2 && updated.friction==.75 && updated.mask==2_147_483_649)
    testing.expect_value(t,history_apply(&state,false),editor.Scene_Error.None)
    restored,_:=ecs.get_component(&owner.world,id,app.Physics_Body); testing.expect(t,restored.mask==max(u32) && restored.friction==.75 && restored.shape.half_extents[0]==2)
    testing.expect_value(t,history_apply(&state,true),editor.Scene_Error.None)
    testing.expect_value(t,inspector_set(&state,id,"PhysicsBody","/shape/half_extents/0",transmute([]byte)string("0")),editor.Scene_Error.Invalid_Field_Value)
    unchanged,_:=ecs.get_component(&owner.world,id,app.Physics_Body); testing.expect(t,unchanged.shape.half_extents[0]==2 && unchanged.mask==2_147_483_649)
    state_destroy(&state); app.authoring_destroy(&owner); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}

@(private="file")
Physics_Inspector_Sparse :: enum i32 { Negative=-7, Large=42 }
@(private="file")
Physics_Inspector_Choice :: struct { choice:Physics_Inspector_Sparse }
@(test)
test_inspector_enum_metadata_preserves_sparse_and_negative_values :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    testing.expect_value(t,app.authoring_services_init(&owner),editor.Scene_Error.None)
    editor.editor_register(&owner.world,&owner.registry,"SparseChoice",Physics_Inspector_Choice{.Negative},spawn_default=false)
    id:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,id,Physics_Inspector_Choice{.Negative})
    state:State; state_init(&state,&owner); defer state_destroy(&state); selection_set(&state,id)
    snapshot,error:=inspector_read(&state); defer inspector_destroy(&snapshot); testing.expect_value(t,error,editor.Scene_Error.None)
    found:=false
    for component in snapshot.components { if component.name!="SparseChoice" { continue }; for field in component.fields {
        if field.path!="/choice" { continue }; found=true
        testing.expect(t,field.kind==.Enum && len(field.variants)==2 && len(field.variant_values)==2 && field.variants[0]=="Negative" && field.variant_values[0]==-7 && field.variants[1]=="Large" && field.variant_values[1]==42 && string(field.value)=="-7")
    } }
    testing.expect(t,found)
    testing.expect_value(t,inspector_set(&state,id,"SparseChoice","/choice",transmute([]byte)string("42")),editor.Scene_Error.None)
    actual,_:=ecs.get_component(&owner.world,id,Physics_Inspector_Choice); testing.expect(t,actual.choice==.Large)
    testing.expect_value(t,history_apply(&state,false),editor.Scene_Error.None)
    before,_:=ecs.get_component(&owner.world,id,Physics_Inspector_Choice); testing.expect(t,before.choice==.Negative)
}
