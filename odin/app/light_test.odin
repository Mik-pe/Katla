#+test
package app

import editor "../editor"
import ecs "../ecs"
import ron "../encoding/ron"
import "core:testing"
import "core:encoding/json"
import "core:strings"

@(test)
test_authored_lights_retain_source_independent_components_and_invalid_replacement_rolls_back :: proc(t:^testing.T) {
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner); testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None)
    source:string=`(version:3,name:"Lights",next_entity_id:4,entities:[(id:1,transform:(),source:Light,directional_light:(direction:(0,-2,0),color:(1,0.8,0.6),intensity:2)),(id:2,transform:(position:(1,2,3)),source:Empty,point_light:(color:(0.2,0.5,1),intensity:4,range:8)),(id:3,transform:(),source:Light)])`
    tree,parse_error:=ron.parse(source); defer json.destroy_value(tree); testing.expect_value(t,parse_error.kind,ron.Error_Kind.None)
    snapshot,decode_error:=scene_document_decode(&owner,tree); defer scene_snapshot_destroy(&snapshot); testing.expect_value(t,decode_error,editor.Scene_Error.None)
    testing.expect_value(t,scene_snapshot_restore(&owner,&snapshot),editor.Scene_Error.None)
    ids:=ecs.entity_ids(&owner.world); defer delete(ids); testing.expect_value(t,len(ids),3)
    sun_count,point_count,icon_count:=0,0,0
    for id in ids {
        if light,present:=ecs.get_component(&owner.world,id,Scene_Directional_Light); present { sun_count+=1; testing.expect(t,light.direction==[3]f32{0,-1,0} && light.intensity==2) }
        if light,present:=ecs.get_component(&owner.world,id,Scene_Point_Light); present { point_count+=1; testing.expect(t,light.range==8 && light.intensity==4) }
        if _,present:=ecs.get_component(&owner.world,id,Scene_Builtin_Source); present { icon_count+=1 }
    }
    testing.expect(t,sun_count==1 && point_count==1 && icon_count==2)
    captured,capture_error:=scene_snapshot_capture(&owner); defer scene_snapshot_destroy(&captured); testing.expect_value(t,capture_error,editor.Scene_Error.None)
    document,encode_error:=scene_document_encode(&owner,&captured,"Lights",""); defer json.destroy_value(document); testing.expect_value(t,encode_error,editor.Scene_Error.None)
    wire,write_error:=ron.write(document); defer delete(wire); testing.expect(t,write_error.kind==.None && strings.contains(string(wire),"point_light:") && strings.contains(string(wire),"directional_light:") && strings.contains(string(wire),"source:\"Light\""))
    for malformed in ([3]string{`(color:(1,1,1),intensity:1,range:0)`,`(color:(2,1,1),intensity:1,range:2)`,`(color:(1,1,1),range:2)`}) {
        descriptor,descriptor_error:=ron.parse(malformed); defer json.destroy_value(descriptor); testing.expect_value(t,descriptor_error.kind,ron.Error_Kind.None)
        fields:=make(json.Object); defer delete(fields); fields["point_light"]=descriptor
        row:=Scene_Entity{components=make([dynamic]Scene_Component)}; defer { for component in row.components { delete(component.name); delete(component.data) }; delete(row.components) }
        testing.expect(t,light_scene_decode(&owner,&row,fields)!=.None && owner.world.live_count==3 && ecs.entity_exists(&owner.world,ids[0]))
    }
}
