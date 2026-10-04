#+test
#+build darwin, linux
package app
import agent "../agent"
import editor "../editor"
import ecs "../ecs"
import resources "../resources"
import ron "../encoding/ron"
import "core:encoding/json"
import "core:testing"
import "core:os"
import "core:strings"
import "core:math"

@(test)
test_generated_particle_keywords_are_actual_runtime_descriptors :: proc(t:^testing.T) {
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner); testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None)
    cases:=[9]struct{word:string,rate,lifetime_low,lifetime_high,velocity_y,scale_end:f32}{{"FIRE",150,.3,1.5,3,.02},{"rain",500,.3,.8,-8,.01},{"snow",80,2,5,-1,.03},{"spark",200,.2,.8,2,.01},{"steam",40,1,4,1.5,.8},{"sand",60,1,3,.3,.06},{"mystic",120,.5,2,2,.02},{"burst",300,.1,.6,0,.02},{"unknown",100,.5,2,1,.02}}
    for item in cases {
        bytes,valid:=resource_generation_content(&owner,{kind=.Particle_System,description=item.word}); testing.expect(t,valid); defer delete(bytes)
        tree,parse_error:=json.parse(bytes,parse_integers=false); testing.expect(t,parse_error==nil); if parse_error!=nil { continue }; defer json.destroy_value(tree)
        descriptor,okay:=particle_decode(tree); testing.expect(t,okay); defer delete(descriptor.burst_queue)
        testing.expect_value(t,descriptor.emit_rate,item.rate); testing.expect(t,math.abs(descriptor.base_lifetime*(1-descriptor.lifetime_variation)-item.lifetime_low)<1e-5 && math.abs(descriptor.base_lifetime*(1+descriptor.lifetime_variation)-item.lifetime_high)<1e-5)
        testing.expect(t,math.abs(descriptor.velocity_direction[1]*descriptor.velocity_magnitude-item.velocity_y)<1e-5 && math.abs(descriptor.base_scale*descriptor.scale_end-item.scale_end)<1e-5)
    }
}

@(test)
test_generation_real_confined_files_scene_load_and_exclusive_failure :: proc(t:^testing.T) {
    directory,error:=os.make_directory_temp("","katla-resource-generation-*",context.allocator); testing.expect(t,error==nil); if error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    resource:=strings.concatenate({directory,"/resources"}); defer delete(resource); testing.expect(t,os.make_directory(resource)==nil)
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner); testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None); testing.expect_value(t,asset_resources_init(&owner,directory,resource),resources.Error.None)
    request:=agent.Resource_Generation_Request{kind=.Scene,path="assets/scenes/generated.katla",description="Night"}
    generated,group:=resource_generation_execute(&owner,request); defer editor.tool_result_destroy(&generated); defer editor.undo_group_destroy(&group); testing.expect_value(t,generated.error,editor.Scene_Error.None); testing.expect(t,group.state==nil && len(owner.agent.session.actions)==0)
    roots:=ecs.get_resource_mut(&owner.world,Asset_Roots); bytes,read_error:=resources.read_text(&roots.project,request.path); testing.expect_value(t,read_error,resources.Error.None); defer delete(bytes)
    tree,parse_error:=ron.parse(string(bytes)); testing.expect(t,parse_error.kind==.None); defer json.destroy_value(tree)
    snapshot,decode_error:=scene_document_decode(&owner,tree,request.path); testing.expect_value(t,decode_error,editor.Scene_Error.None); defer scene_snapshot_destroy(&snapshot); testing.expect(t,len(snapshot.entities)==1 && snapshot.next_entity_id==2)
    loaded,load_group:=scene_file_execute(&owner,{action=.Load,path=request.path,has_path=true}); defer editor.tool_result_destroy(&loaded); defer editor.undo_group_destroy(&load_group); testing.expect_value(t,loaded.error,editor.Scene_Error.None)
    ids:=ecs.entity_ids(&owner.world); defer delete(ids); light:^Scene_Directional_Light; for id in ids { candidate:=ecs.get_component_mut(&owner.world,id,Scene_Directional_Light); if candidate!=nil { light=candidate } }; testing.expect(t,light!=nil && light.intensity==.1 && light.color==([3]f32{.05,.05,.1}))
    duplicate,duplicate_group:=resource_generation_execute(&owner,request); defer editor.tool_result_destroy(&duplicate); defer editor.undo_group_destroy(&duplicate_group); testing.expect_value(t,duplicate.error,editor.Scene_Error.Invalid_Operation)
    preserved,preserved_error:=resources.read_text(&roots.project,request.path); testing.expect_value(t,preserved_error,resources.Error.None); testing.expect_value(t,string(preserved),string(bytes)); delete(preserved)
    for path in ([2]string{"../escape.json","/absolute.json"}) { invalid,bad_group:=resource_generation_execute(&owner,{kind=.Particle_System,path=path}); defer editor.tool_result_destroy(&invalid); defer editor.undo_group_destroy(&bad_group); testing.expect_value(t,invalid.error,editor.Scene_Error.Invalid_Operation) }
    owner.mode=.Playing; rejected,rejected_group:=resource_generation_execute(&owner,{kind=.Particle_System,path="playing.json"}); defer editor.tool_result_destroy(&rejected); defer editor.undo_group_destroy(&rejected_group); testing.expect_value(t,rejected.error,editor.Scene_Error.Editing_Required)
}
