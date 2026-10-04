package editor_app
import render "../render"
import "core:testing"
import "core:slice"
import "core:strings"
import "core:encoding/json"
import app ".."
import ecs "../../ecs"
import km "../../math"

@(test)
test_capture_png_native_padding_bgra_alpha :: proc(t:^testing.T) {
    bytes:=[24]byte{0,0,255,255,0,255,0,128,55,55,55,55,255,0,0,64,255,255,255,0,66,66,66,66}
    snapshot:=render.Picking_Snapshot{metadata={width=2,height=2},color={bytes=bytes[:],row_pitch=12,source={desc={format=.BGRA8_Unorm}}}}
    png,valid:=capture_png(&snapshot); defer delete(png); testing.expect(t,valid)
    decoded,error:=render.texture_image_decode(png); defer render.texture_image_destroy(&decoded)
    testing.expect(t,error==.None && decoded.width==2 && decoded.height==2)
    expected:=[16]byte{255,0,0,255,0,255,0,128,0,0,255,64,255,255,255,0}
    testing.expect(t,slice.equal(decoded.pixels,expected[:]))
    snapshot.color.row_pitch=3; _,valid=capture_png(&snapshot); testing.expect(t,!valid)
}
@(test)
test_capture_context_is_owned_and_frustum_bound :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    app.scene_components_register(&owner)
    geometry,error:=app.mesh_cube({1,1,1}); defer app.mesh_geometry_destroy(&geometry); testing.expect(t,error==.None)
    entity:=ecs.spawn(&owner.world,struct{mesh:app.Scene_Mesh,transform:app.Scene_Transform,name:app.Scene_Name}{{geometry=geometry},{km.transform()},{strings.clone("Before")}})
    ecs.add_component(&owner.world,entity,app.Surface_Material{roughness=0.5,ao=1})
    state:State; state_init(&state,&owner); defer state_destroy(&state); selection_set(&state,entity,.Replace)
    shell:=Shell{state=&state,allocator=context.allocator}; viewport_grid_init(&shell.viewports)
    encoded,valid:=capture_context(&shell,0,{frame=3,serial=7,width=100,height=100},{{encoded=1,entity=entity}},context.allocator); defer delete(encoded); testing.expect(t,valid)
    ecs.remove_component(&owner.world,entity,app.Surface_Material)
    name:=ecs.get_component_mut(&owner.world,entity,app.Scene_Name); delete(name.name); name.name=strings.clone("After")
    tree,parse_error:=json.parse(encoded,parse_integers=true); testing.expect(t,parse_error==nil); defer json.destroy_value(tree)
    value:=tree.(json.Object); rows:=value["frustum_candidates"].(json.Array)
    testing.expect(t,value["frame_id"].(string)=="3" && value["capture_serial"].(string)=="7" && len(value["selected_entities"].(json.Array))==1 && len(rows)==1 && rows[0].(json.Object)["name"].(string)=="Before" && rows[0].(json.Object)["material_editable"].(bool))
    testing.expect(t,!capture_bounds_visible(camera_view_projection(&shell.viewports.slots[0].camera,1),km.AABB{{0,0,100},{1,1,1}}))
}

@(test)
test_capture_candidate_near_clipped_projection_and_sorted_owned_primary_context :: proc(t:^testing.T) {
    projection:=km.mat4_perspective(60,1,0.1,100)
    left:=km.AABB{{-2,0,-5},{0.5,0.5,0.5}}; right:=km.AABB{{2,0,-5},{0.5,0.5,0.5}}
    left_rect,left_valid:=capture_bounds_project(projection,left,0.1).([4]f32); right_rect,right_valid:=capture_bounds_project(projection,right,0.1).([4]f32)
    testing.expect(t,left_valid && right_valid && left_rect[2]<0.5 && right_rect[0]>0.5)
    partial:=km.AABB{{4,0,-5},{3,0.5,0.5}}; partial_rect,partial_valid:=capture_bounds_project(projection,partial,0.1).([4]f32)
    testing.expect(t,capture_bounds_visible(projection,partial) && !capture_bounds_fully_visible(projection,partial) && partial_valid && partial_rect[2]==1)
    crossing:=km.AABB{{0,0,-0.15},{0.2,0.2,0.2}}; crossing_rect,crossing_valid:=capture_bounds_project(projection,crossing,0.1).([4]f32); testing.expect(t,crossing_valid && crossing_rect==[4]f32{0,0,1,1})
    _,behind_valid:=capture_bounds_project(projection,km.AABB{{0,0,5},{1,1,1}},0.1).([4]f32); testing.expect(t,!behind_valid)
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner); app.scene_components_register(&owner)
    geometry,error:=app.mesh_cube({1,1,1}); defer app.mesh_geometry_destroy(&geometry); testing.expect(t,error==.None)
    first:=ecs.spawn(&owner.world,struct{mesh:app.Scene_Mesh,transform:app.Scene_Transform}{{geometry=geometry},{km.transform()}})
    second:=ecs.spawn(&owner.world,struct{mesh:app.Scene_Mesh,transform:app.Scene_Transform,parent:app.Scene_Parent}{{geometry=geometry},{km.transform(position={2,0,0})},{first}})
    state:State; state_init(&state,&owner); defer state_destroy(&state); shell:=Shell{state=&state}; viewport_grid_init(&shell.viewports)
    encoded,valid:=capture_context(&shell,0,{frame=3,serial=7,width=100,height=100},{{encoded=2,entity=second},{encoded=1,entity=first}},context.allocator); defer delete(encoded); testing.expect(t,valid)
    tree,parse_error:=json.parse(encoded,parse_integers=true); testing.expect(t,parse_error==nil); defer json.destroy_value(tree)
    object:=tree.(json.Object); rows:=object["frustum_candidates"].(json.Array); first_row:=rows[0].(json.Object); second_row:=rows[1].(json.Object)
    testing.expect(t,len(rows)==2 && !first_row["material_editable"].(bool) && !second_row["material_editable"].(bool) && first_row["entity_id"].(string)=="0" && second_row["parent_id"].(string)=="0")
    _,name_null:=first_row["name"].(json.Null); testing.expect(t,name_null && !object["undo_available"].(bool) && !object["redo_available"].(bool))
    camera:=object["camera"].(json.Object); testing.expect(t,len(camera["view_matrix"].(json.Array))==4 && first_row["visibility"].(string)=="frustum_candidate_occlusion_unknown")
}
