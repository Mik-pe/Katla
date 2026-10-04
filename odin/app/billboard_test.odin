#+test
package app

import "core:testing"
import "core:encoding/json"
import ecs "../ecs"
import km "../math"
import editor "../editor"

@(test)
test_authored_billboard_extension_roundtrip_and_invalid_descriptor :: proc(t:^testing.T) {
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner); testing.expect(t,authoring_services_init(&owner)==.None)
    entity:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,entity,Scene_Transform{km.TRANSFORM_IDENTITY}); ecs.add_component(&owner.world,entity,Scene_Billboard{.Fire,{.2,.4,.6,.8},1.5})
    snapshot,error:=scene_snapshot_capture(&owner); defer scene_snapshot_destroy(&snapshot); testing.expect(t,error==.None)
    document,document_error:=scene_document_encode(&owner,&snapshot,"Billboard",""); defer json.destroy_value(document); testing.expect_value(t,document_error,editor.Scene_Error.None); if document_error!=.None { return }
    rows:=document.(json.Object)["entities"].(json.Array); extensions:=rows[0].(json.Object)["components"].(json.Object); testing.expect(t,extensions["Billboard"]!=nil)
    prepared,prepare_error:=scene_document_prepare(&owner,document); defer scene_snapshot_destroy(&prepared); testing.expect(t,prepare_error==.None && scene_snapshot_restore(&owner,&prepared)==.None)
    ids:=ecs.entity_ids(&owner.world); defer delete(ids); testing.expect(t,len(ids)==1); restored,present:=ecs.get_component(&owner.world,ids[0],Scene_Billboard); testing.expect(t,present && restored.icon==.Fire && restored.color==[4]f32{.2,.4,.6,.8} && restored.size==1.5)
    testing.expect(t,!billboard_valid(Scene_Billboard{.Fire,{1,1,1,1},0}) && !billboard_valid(Scene_Billboard{Billboard_Icon(9),{1,1,1,1},1}) && !billboard_valid(Scene_Billboard{.Fire,{1,1,2,1},1}))
    testing.expect_value(t,billboard_scene_validate(&owner,ids[:]),editor.Scene_Error.None)
}
