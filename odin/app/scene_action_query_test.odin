#+test
package app

import ecs "../ecs"
import editor "../editor"
import agent "../agent"
import km "../math"
import resources "../resources"
import "core:testing"
import "core:encoding/json"
import "core:os"
import "core:strings"

@(test)
test_query_actual_bounds_world_hierarchy_unicode_and_rich_null_rows :: proc(t:^testing.T) {
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner)
    testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None)
    operation:=editor.Scene_Op{kind=.Spawn,shape="cube",name="ÅNGSTRÖM ΟΣ İSTANBUL",position={10,0,0},scale={20,1,1}}
    action:=editor.agent_execute(&owner.agent.session,&owner.world,&owner.registry,operation,authoring_executor(&owner))
    testing.expect(t,action.result.error==.None && len(action.result.entities)==1)
    big:=action.result.entities[0]
    query:=scene_action_query(&owner,{kind=.Query_Entities,name_filter="ångström",position={0,0,0},radius=0,has_query_position=true,has_radius=true})
    defer editor.tool_result_destroy(&query)
    testing.expect(t,query.error==.None && len(query.entities)==1 && query.entities[0]==big)
    tree,err:=json.parse(query.data,spec=.JSON,parse_integers=true); testing.expect(t,err==nil); if err!=nil { return }; defer json.destroy_value(tree)
    data:=tree.(json.Object); rows:=data["entities"].(json.Array); row:=rows[0].(json.Object)
    testing.expect(t,data["total"].(json.Integer)==1 && !bool(data["truncated"].(json.Boolean)))
    bounds:=row["bounds"].(json.Object); extent:=bounds["extent"].(json.Array)
    testing.expect(t,query_test_number(extent[0])==10 && len(row["components"].(json.Array))>0)
    _,root_parent:=row["parent_id"].(json.Null); testing.expect(t,root_parent && row["entity_id"].(string)=="0")
    for filter in ([]string{"ος","i\u0307stanbul"}) {
        filtered:=scene_action_query(&owner,{kind=.Query_Entities,name_filter=filter}); testing.expect(t,filtered.error==.None && len(filtered.entities)==1); editor.tool_result_destroy(&filtered)
    }
    parent:=ecs.create_entity(&owner.world)
    ecs.add_component(&owner.world,parent,Scene_Transform{km.Transform{position={30,0,0},rotation=km.QUAT_IDENTITY,scale=km.VEC3_ONE}})
    child:=ecs.create_entity(&owner.world)
    ecs.add_component(&owner.world,child,Scene_Transform{km.TRANSFORM_IDENTITY})
    ecs.add_component(&owner.world,child,Scene_Parent{parent})
    by_origin:=scene_action_query(&owner,{kind=.Query_Entities,position={30,0,0},radius=0,has_query_position=true,has_radius=true})
    testing.expect(t,by_origin.error==.None && len(by_origin.entities)==2); editor.tool_result_destroy(&by_origin)
    unplaced:=ecs.create_entity(&owner.world)
    all:=scene_action_query(&owner,{kind=.Query_Entities}); defer editor.tool_result_destroy(&all)
    full,parse_error:=json.parse(all.data,spec=.JSON,parse_integers=true); testing.expect(t,parse_error==nil); if parse_error!=nil { return }; defer json.destroy_value(full)
    full_rows:=full.(json.Object)["entities"].(json.Array)
    testing.expect(t,len(full_rows)==4)
    final:=full_rows[3].(json.Object); _,no_name:=final["name"].(json.Null); _,no_position:=final["position"].(json.Null); _,no_bounds:=final["bounds"].(json.Null)
    unplaced_text:=scene_action_id_text(unplaced,context.allocator); defer delete(unplaced_text)
    testing.expect(t,no_name && no_position && no_bounds && final["entity_id"].(string)==unplaced_text)
    named:=scene_action_query(&owner,{kind=.Query_Entities,has_name_filter=true,name_filter=""}); defer editor.tool_result_destroy(&named)
    testing.expect(t,named.error==.None && len(named.entities)==1 && named.entities[0]==big)
}

@(test)
test_query_actual_glTF_posed_skin_morph_active_scene_bounds :: proc(t:^testing.T) {
    directory,dir_error:=os.make_directory_temp("","katla-query-pose-*",context.allocator)
    testing.expect(t,dir_error==nil); if dir_error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    filename:=strings.concatenate({directory,"/model.gltf"}); defer delete(filename)
    original:=gltf_sparse_fixture(); defer delete(original)
    nodes,_:=strings.replace_all(original,`"nodes":[{"mesh":0,"skin":0},{"name":"joint"}]`,`"nodes":[{"mesh":0,"skin":0},{"name":"joint"},{"mesh":0,"translation":[100,0,0]}]`); defer delete(nodes)
    source,_:=strings.replace_all(nodes,`"scenes":[{"nodes":[0,1]}]`,`"scenes":[{"nodes":[0,1]},{"nodes":[2]}]`); defer delete(source)
    testing.expect_value(t,os.write_entire_file(filename,source),os.Error(nil))
    root,root_error:=resources.root_open(directory); testing.expect_value(t,root_error,resources.Error.None); if root_error!=.None { return }; defer resources.root_destroy(&root)
    model,load_error:=gltf_load(&root,"model.gltf"); testing.expect_value(t,load_error,Gltf_Error.None); if load_error!=.None { return }
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner)
    scene_components_register(&owner); scene_model_register(&owner); animation_register(&owner.world,&owner.registry)
    entity:=ecs.spawn(&owner.world,struct {model:Scene_Model,transform:Scene_Transform}{ {model=model},{local={position={10,0,0},rotation=km.QUAT_IDENTITY,scale={2,2,2}}} })
    player:=animation_player_stopped(); player.clip=strings.clone("move"); player.time=0.5; ecs.add_component(&owner.world,entity,player)
    bounds,present,bounds_error:=scene_drawable_bounds(&owner,entity)
    testing.expect(t,present && bounds_error==.None)
    testing.expect_value(t,bounds.center,km.Vec3{11,1,2}); testing.expect_value(t,bounds.extent,km.Vec3{1,1,0})
    query:=scene_action_query(&owner,{kind=.Query_Entities,position={10,0,2},radius=0,has_query_position=true,has_radius=true}); defer editor.tool_result_destroy(&query)
    testing.expect(t,query.error==.None && len(query.entities)==1 && query.entities[0]==entity,"Spatial query must use sampled skin/morph bounds, excluding inactive scene geometry")
    far:=scene_action_query(&owner,{kind=.Query_Entities,position={210,0,0},radius=1,has_query_position=true,has_radius=true}); defer editor.tool_result_destroy(&far)
    testing.expect(t,far.error==.None && len(far.entities)==0)
}

@(test)
test_query_full_u64_sort_total_and_default64_truncation :: proc(t:^testing.T) {
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner)
    first:=ecs.create_entity(&owner.world); ecs.destroy_entity(&owner.world,first)
    high:=ecs.create_entity(&owner.world); testing.expect(t,u64(high)>0xffffffff)
    for _ in 0..<69 { ecs.create_entity(&owner.world) }
    query:=scene_action_query(&owner,{kind=.Query_Entities}); defer editor.tool_result_destroy(&query)
    testing.expect(t,query.error==.None && len(query.entities)==64 && query.entities[0]==1)
    tree,err:=json.parse(query.data,spec=.JSON,parse_integers=true); testing.expect(t,err==nil); if err!=nil { return }; defer json.destroy_value(tree)
    data:=tree.(json.Object); testing.expect(t,data["total"].(json.Integer)==70 && bool(data["truncated"].(json.Boolean)))
    all:=scene_action_query(&owner,{kind=.Query_Entities,limit=256}); defer editor.tool_result_destroy(&all)
    testing.expect(t,len(all.entities)==70 && all.entities[len(all.entities)-1]==high)
    mismatch:=scene_action_query(&owner,{kind=.Query_Entities,has_radius=true,radius=1}); defer editor.tool_result_destroy(&mismatch)
    testing.expect_value(t,mismatch.error,editor.Scene_Error.Invalid_Operation)
}

@(test)
test_query_nullable_protocol_default_clamp_and_paired_spatial_arguments :: proc(t:^testing.T) {
    for text in ([]string{`{}`,`{"component_filter":null,"name_filter":null,"position":null,"radius":null,"limit":null}`}) {
        call,err:=agent.decode_call({"query","query_entities",transmute([]byte)text}); testing.expect(t,err==.None && call.operation.limit==64 && !call.operation.has_radius); agent.decoded_call_destroy(&call)
    }
    for text in ([]string{`{"position":[0,0,0]}`,`{"radius":1}`,`{"position":[0,0,0],"radius":-1}`,`{"limit":18446744073709551616}`,`{"limit":1.0}`,`{"limit":-1}`}) {
        call,err:=agent.decode_call({"query","query_entities",transmute([]byte)text}); testing.expect(t,err!=.None); if err==.None { agent.decoded_call_destroy(&call) }
    }
    for text in ([]string{`{"limit":0}`,`{"limit":999}`,`{"limit":18446744073709551615}`,`{"l\u0069mit":18446744073709551615}`}) {
        call,err:=agent.decode_call({"query","query_entities",transmute([]byte)text}); testing.expect(t,err==.None && (call.operation.limit==1 || call.operation.limit==256)); agent.decoded_call_destroy(&call)
    }
    named_arguments:string=`{"name_filter":""}`
    named,name_error:=agent.decode_call({"query","query_entities",transmute([]byte)named_arguments}); testing.expect(t,name_error==.None && named.operation.has_name_filter); agent.decoded_call_destroy(&named)
}

@(test)
test_query_transformed_bounds_overflow_rejected_without_partial_result :: proc(t:^testing.T) {
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner)
    testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None)
    action:=editor.agent_execute(&owner.agent.session,&owner.world,&owner.registry,{kind=.Spawn,shape="cube",scale={1,1,1}},authoring_executor(&owner))
    testing.expect(t,action.result.error==.None && len(action.result.entities)==1)
    if action.result.error!=.None || len(action.result.entities)!=1 { return }
    entity:=action.result.entities[0]
    parent:=ecs.create_entity(&owner.world)
    ecs.add_component(&owner.world,parent,Scene_Transform{local={rotation=km.QUAT_IDENTITY,scale={max(f32),max(f32),max(f32)}}})
    ecs.add_component(&owner.world,entity,Scene_Parent{parent})
    ecs.add_component(&owner.world,entity,Scene_Transform{local={rotation=km.QUAT_IDENTITY,scale={2,2,2}}})
    bounds,has_bounds,bounds_error:=scene_drawable_bounds(&owner,entity)
    testing.expect(t,!has_bounds && bounds_error==.Invalid_Operation && bounds==km.AABB{})
    query:=scene_action_query(&owner,{kind=.Query_Entities}); defer editor.tool_result_destroy(&query)
    testing.expect(t,query.error==.Invalid_Operation && len(query.entities)==0)
}

@(test)
test_unicode_default_full_lowercase_context_and_expansion :: proc(t:^testing.T) {
    for pair in ([][2]string{{"ÅNGSTRÖM","ångström"},{"İSTANBUL","i\u0307stanbul"},{"ΟΣ","ος"},{"ΟΣΑ","οσα"},{"Α\u0301Σ","α\u0301ς"},{"ΟΣ\u0301","ος\u0301"}}) {
        lowered:=scene_query_lower(pair[0]); testing.expect_value(t,lowered,pair[1]); delete(lowered)
    }
}

query_test_number :: proc(value:json.Value)->f64 {
    #partial switch number in value {
    case json.Integer: return f64(number)
    case json.Float: return f64(number)
    }
    return -1
}
