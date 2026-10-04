#+test
//! Surface edits prepare native candidates and retain authored and transient state through history.
package app
import ecs "../ecs"
import editor "../editor"
import agent "../agent"
import km "../math"
import "core:testing"
import "core:mem"
import "core:encoding/json"

@(private="file")
Surface_Test_Participant :: struct { reject:bool,commits:int }
@(private="file")
surface_test_prepare :: proc(state:rawptr,owner:^Authoring,ids:[]ecs.Entity_Id,mode:Scene_Preparation_Mode)->(rawptr,editor.Scene_Error) { participant:=cast(^Surface_Test_Participant)state; return participant,.Invalid_Operation if participant.reject else .None }
@(private="file")
surface_test_finish :: proc(state,token:rawptr,committed:bool) { if committed { (cast(^Surface_Test_Participant)state).commits+=1 } }

@(test)
test_material_surface_native_gate_exact_history_and_wire_units :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:Authoring; authoring_init(&owner); testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None)
    ids:=[2]ecs.Entity_Id{ecs.create_entity(&owner.world),ecs.create_entity(&owner.world)}
    original:=Surface_Material{roughness=.4,ao=.8,has_sampling=true,sampling=material_sampling_default()}; original.sampling.normal.uv={tex_coord=1,offset={.1,.2},rotation=.3,scale={-2,0}}
    for id in ids { ecs.add_component(&owner.world,id,original); ecs.add_component(&owner.world,id,Scene_Transform{km.TRANSFORM_IDENTITY}) }
    participant:=Surface_Test_Participant{reject=true}; ecs.insert_resource(&owner.world,Scene_Participant{&participant,surface_test_prepare,surface_test_finish})
    op:=agent.Material_Set{entities=ids[:],fields={.Emissive_Factor,.Normal_Scale,.Occlusion_Strength,.Alpha_Mode,.Alpha_Cutoff,.Double_Sided},values={emissive_factor={3,2,1},normal_scale= -2,occlusion_strength=.25,alpha_mode=.Blend,alpha_cutoff=2,double_sided=true}}
    result,group:=material_execute(&owner,op); testing.expect_value(t,result.error,editor.Scene_Error.Invalid_Operation); testing.expect(t,group.state==nil && participant.commits==0); editor.tool_result_destroy(&result)
    for id in ids { testing.expect_value(t,ecs.get_component_mut(&owner.world,id,Surface_Material)^,original) }
    participant.reject=false; result,group=material_execute(&owner,op); testing.expect_value(t,result.error,editor.Scene_Error.None); testing.expect_value(t,participant.commits,1)
    tree,error:=json.parse(result.data); testing.expect(t,error==nil)
    object:=tree.(json.Object); testing.expect(t,object["base_color_space"].(string)=="srgb" && object["emissive_color_space"].(string)=="linear")
    capabilities:=object["capabilities"].(json.Object); testing.expect(t,capabilities["batch_atomic"].(bool) && !capabilities["alpha_changes_render_mode"].(bool))
    outputs:=object["materials"].(json.Array); values:=outputs[0].(json.Object)["values"].(json.Object); testing.expect_value(t,values["alpha_mode"].(string),"blend"); json.destroy_value(tree)
    ecs.get_component_mut(&owner.world,ids[0],Scene_Transform).local.position={4,5,6}
    participant.reject=true; testing.expect_value(t,editor.undo_group(&owner.world,&owner.registry,&group),editor.Scene_Error.Invalid_Operation)
    participant.reject=false; testing.expect_value(t,editor.undo_group(&owner.world,&owner.registry,&group),editor.Scene_Error.None)
    for id in ids { testing.expect_value(t,ecs.get_component_mut(&owner.world,id,Surface_Material)^,original) }
    testing.expect_value(t,ecs.get_component_mut(&owner.world,ids[0],Scene_Transform).local.position,km.Vec3{4,5,6})
    testing.expect_value(t,editor.redo_group(&owner.world,&owner.registry,&group),editor.Scene_Error.None)
    for id in ids { after:=ecs.get_component_mut(&owner.world,id,Surface_Material); testing.expect(t,after.has_surface && after.surface.alpha_mode==.Blend && after.surface.normal_scale== -2 && after.surface.alpha_cutoff==2 && after.sampling==original.sampling) }
    editor.tool_result_destroy(&result); editor.undo_group_destroy(&group); authoring_destroy(&owner)
    testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
