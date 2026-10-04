#+test
//! Real imported children share a source revision while independent material and hierarchy history remains exact.
package app
import ecs "../ecs"
import editor "../editor"
import resources "../resources"
import agent "../agent"
import "core:encoding/json"
import "core:mem"
import "core:os"
import "core:strings"
import "core:testing"

@(private="file")
gltf_entities_fixture :: proc()->[]byte {
    original:=gltf_sparse_fixture(); defer delete(original)
    value,error:=json.parse(transmute([]byte)original); assert(error==nil); defer json.destroy_value(value)
    object:=value.(json.Object); mesh:=object["meshes"].(json.Array)[0].(json.Object); previous:=mesh["primitives"].(json.Array)
    second,valid:=scene_value_clone(previous[0]); assert(valid)
    primitives:=make(json.Array,2); primitives[0]=previous[0]; primitives[1]=second; delete(previous); mesh["primitives"]=primitives
    first_object:=primitives[0].(json.Object); second_object:=primitives[1].(json.Object); scene_json_put(&first_object,"material",json.Integer(0)); scene_json_put(&second_object,"material",json.Integer(1))
    materials,material_error:=json.parse(`[{"pbrMetallicRoughness":{"baseColorFactor":[0.2,0.3,0.4,1],"metallicFactor":0,"roughnessFactor":0.3},"emissiveFactor":[2,1,0],"alphaMode":"MASK","alphaCutoff":0.7,"doubleSided":true},{"pbrMetallicRoughness":{"baseColorFactor":[0.8,0.1,0.2,1],"metallicFactor":0.9,"roughnessFactor":0.6}}]`); assert(material_error==nil); scene_json_put(&object,"materials",materials)
    bytes,marshal_error:=json.marshal(value); assert(marshal_error==nil); return bytes
}
@(test)
test_gltf_spawn_independent_primitives_shared_revision_material_and_snapshot :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    defer testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
    directory,error:=os.make_directory_temp("","katla-gltf-entities-*",context.allocator); testing.expect(t,error==nil); if error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    resource_directory:=strings.concatenate({directory,"/resources"}); defer delete(resource_directory); testing.expect(t,os.make_directory(resource_directory)==nil)
    filename:=strings.concatenate({resource_directory,"/source.gltf"}); defer delete(filename)
    bytes:=gltf_entities_fixture(); testing.expect(t,os.write_entire_file(filename,bytes)==nil); delete(bytes)
    owner:Authoring; authoring_init(&owner); testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None); testing.expect_value(t,asset_resources_init(&owner,directory,resource_directory),resources.Error.None)
    op:=editor.Scene_Op{kind=.Spawn_Model,path="source.gltf",scale={1,1,1},position={2,3,4}}
    result,group:=scene_action_execute(&owner,op); testing.expect_value(t,result.error,editor.Scene_Error.None); testing.expect_value(t,len(result.entities),3)
    if result.error==.None && len(result.entities)==3 {
        root,a,b:=result.entities[0],result.entities[1],result.entities[2]
        controller:=ecs.get_component_mut(&owner.world,root,Scene_Model); first:=ecs.get_component_mut(&owner.world,a,Scene_Model); second:=ecs.get_component_mut(&owner.world,b,Scene_Model)
        testing.expect(t,controller.source.kind==.Group && first.source.kind==.Primitive && second.source.kind==.Primitive && first.source.primitive_index==0 && second.source.primitive_index==1)
        testing.expect(t,controller.revision!=nil && controller.revision==first.revision && first.revision==second.revision)
        _,has_surface:=ecs.get_component(&owner.world,root,Surface_Material); testing.expect(t,!has_surface && ecs.get_component_mut(&owner.world,a,Scene_Parent).entity==root)
        _,has_player:=ecs.get_component(&owner.world,a,Animation_Player); testing.expect(t,!has_player)
        testing.expect(t,scene_model_animation_player(&owner,a)==ecs.get_component_mut(&owner.world,root,Animation_Player))
        values,read_error:=material_entity_values(&owner,a); testing.expect(t,read_error==.None && values.metallic==0 && values.emissive_factor==[3]f32{2,1,0} && values.alpha_mode==.Mask && values.double_sided)
        changed,undo:=material_execute(&owner,agent.Material_Set{entities={a},fields={.Metallic},values={metallic=.5}}); testing.expect_value(t,changed.error,editor.Scene_Error.None)
        unaffected,_:=material_entity_values(&owner,b); testing.expect_value(t,unaffected.metallic,f32(.9)); editor.tool_result_destroy(&changed); editor.undo_group_destroy(&undo)
        snapshot,capture_error:=scene_snapshot_capture(&owner); testing.expect_value(t,capture_error,editor.Scene_Error.None)
        document,export_error:=scene_document_encode(&owner,&snapshot,"Independent primitives","scene.katla"); testing.expect_value(t,export_error,editor.Scene_Error.None)
        durable,decode_error:=scene_document_decode(&owner,document); testing.expect_value(t,decode_error,editor.Scene_Error.None); json.destroy_value(document)
        testing.expect_value(t,scene_snapshot_restore(&owner,&durable),editor.Scene_Error.None); scene_snapshot_destroy(&durable)
        durable_ids:=ecs.entity_ids(&owner.world); revision:^Scene_Model_Revision; durable_primitives:=0
        for entity in durable_ids { if current:=ecs.get_component_mut(&owner.world,entity,Scene_Model); current!=nil {
            if revision==nil { revision=current.revision }; testing.expect(t,current.revision==revision)
            if current.source.kind==.Primitive { durable_primitives+=1; if current.source.primitive_index==0 { current_values,values_error:=material_entity_values(&owner,entity); testing.expect(t,values_error==.None && current_values.metallic==.5 && current_values.emissive_factor==[3]f32{2,1,0}) } }
        } }; delete(durable_ids); testing.expect_value(t,durable_primitives,2)
        testing.expect(t,os.remove(filename)==nil)
        testing.expect_value(t,scene_snapshot_restore(&owner,&snapshot),editor.Scene_Error.None); scene_snapshot_destroy(&snapshot)
        testing.expect(t,!ecs.entity_exists(&owner.world,root) && owner.world.live_count==3)
    }
    editor.tool_result_destroy(&result); editor.undo_group_destroy(&group); authoring_destroy(&owner)
}
