#+test
package app

import editor "../editor"
import ecs "../ecs"
import ron "../encoding/ron"
import km "../math"
import "core:testing"
import "core:encoding/json"

@(test)
test_scene_perspective_document_projection_and_atomic_rejection :: proc(t:^testing.T) {
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner); testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None)
    text:=`(version:3,name:"Camera",next_entity_id:2,entities:[(id:1,name:"Camera",transform:(position:(2,3,4)),source:Empty,perspective:(fov:90,near:0.25,aspect_ratio:2))])`
    tree,parse_error:=ron.parse(text); testing.expect(t,parse_error.kind==.None); defer json.destroy_value(tree)
    snapshot,error:=scene_document_prepare(&owner,tree); testing.expect_value(t,error,editor.Scene_Error.None); defer scene_snapshot_destroy(&snapshot)
    testing.expect_value(t,scene_snapshot_restore(&owner,&snapshot),editor.Scene_Error.None)
    ids:=ecs.entity_ids(&owner.world); defer delete(ids); testing.expect_value(t,len(ids),1); if len(ids)!=1 { return }
    perspective,present:=ecs.get_component(&owner.world,ids[0],Scene_Perspective); testing.expect(t,present && perspective.fov==90 && perspective.near==0.25 && perspective.aspect_ratio==2)
    projection,valid:=perspective_projection(perspective); testing.expect(t,valid)
    projected:=km.matrix_vector(projection,km.Vec4{0,0,-perspective.near,1}); testing.expect(t,abs(projected[2]/projected[3]-1)<0.00001)
    captured,capture_error:=scene_snapshot_capture(&owner); testing.expect_value(t,capture_error,editor.Scene_Error.None); defer scene_snapshot_destroy(&captured)
    document,encode_error:=scene_document_encode(&owner,&captured,"Camera",""); testing.expect_value(t,encode_error,editor.Scene_Error.None); defer json.destroy_value(document)
    fields:=document.(json.Object)["entities"].(json.Array)[0].(json.Object); testing.expect(t,fields["perspective"]!=nil)
    if components,is_components:=fields["components"].(json.Object); is_components { _,duplicate:=components["Perspective"]; testing.expect(t,!duplicate) }
    entry:=owner.registry.entries["Perspective"]; testing.expect(t,entry.inspector_add && entry.inspector_remove && !entry.spawn_default)
    camera_fields:=fields["perspective"].(json.Object); camera_fields["fov"]=json.Integer(180)
    rejected,rejection:=scene_document_prepare(&owner,document); defer scene_snapshot_destroy(&rejected); testing.expect_value(t,rejection,editor.Scene_Error.Invalid_Field_Value)
    testing.expect(t,ecs.entity_exists(&owner.world,ids[0]) && owner.world.live_count==1)
}
