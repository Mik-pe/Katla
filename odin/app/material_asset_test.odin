#+test
#+build darwin, linux
package app

import agent "../agent"
import gfx "../gfx"
import image "../image"
import ecs "../ecs"
import editor "../editor"
import resources "../resources"
import "core:encoding/json"
import "core:testing"
import "core:os"
import "core:strings"

@(private="file")
material_asset_test_entity :: proc(owner:^Authoring,key:u64)->ecs.Entity_Id {
    geometry,error:=mesh_cube({1,1,1}); assert(error==.None)
    source:=Mesh_Source{kind=.Geometry,geometry=transmute([]byte)strings.clone(`{"kind":"cube","size":[1,1,1]}`)}
    id:=ecs.create_entity(&owner.world)
    ecs.add_component(&owner.world,id,Scene_Transform{}); ecs.get_component_mut(&owner.world,id,Scene_Transform).local.scale={1,1,1}; ecs.get_component_mut(&owner.world,id,Scene_Transform).local.rotation={0,0,0,1}
    ecs.add_component(&owner.world,id,Scene_Key{key}); ecs.add_component(&owner.world,id,Scene_Mesh{source=source,geometry=geometry})
    ecs.add_component(&owner.world,id,Surface_Material{roughness=.5,ao=1})
    return id
}
@(test)
test_material_asset_real_image_independent_copy_history_and_scene_origin :: proc(t:^testing.T) {
    directory,error:=os.make_directory_temp("","katla-material-asset-*",context.allocator); if !testing.expect(t,error==nil) { return }; defer { os.remove_all(directory); delete(directory) }
    root:=strings.concatenate({directory,"/resources"}); defer delete(root); testing.expect_value(t,os.make_directory(root),os.Error(nil))
    for folder in ([1]string{"images"}) { name:=strings.concatenate({root,"/",folder}); testing.expect_value(t,os.make_directory(name),os.Error(nil)); delete(name) }
    path:=strings.concatenate({root,"/images/source.png"}); defer delete(path)
    bytes:=#load("render/texture_image_fixtures/rgba.png",[]byte); testing.expect_value(t,os.write_entire_file(path,bytes),os.Error(nil))
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner)
    if owner.registry.entries["MaterialImages"]==nil { material_images_register(&owner) }
    testing.expect_value(t,asset_resources_init(&owner,directory,root),resources.Error.None)
    testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None)
    a:=material_asset_test_entity(&owner,1); b:=material_asset_test_entity(&owner,2)
    probe,probe_error:=material_image_prepare(&owner,{kind=.File,path="images/source.png",root=.Resource}); testing.expect_value(t,probe_error,editor.Scene_Error.None)
    delete(probe.source.path); image.texture_image_destroy(&probe.image)
    result,assignment:=material_texture_execute(&owner,{entities={a},role=.Albedo,source={kind=.File,path="images/source.png",root=.Resource}})
    if !testing.expect_value(t,result.error,editor.Scene_Error.None) { editor.tool_result_destroy(&result); editor.undo_group_destroy(&assignment); return }; editor.tool_result_destroy(&result); defer editor.undo_group_destroy(&assignment)
    first,_:=ecs.get_component(&owner.world,a,Material_Images); testing.expect(t,first.roles[0].image.width==2 && first.roles[0].image.height==2)
    captured,empty:=material_asset_execute(&owner,{action=.Capture,path="resources/materials/captured.katmat",entity=a}); editor.undo_group_destroy(&empty); defer editor.tool_result_destroy(&captured)
    if !testing.expect(t,captured.error==.None && strings.contains(string(captured.data),`"saved":true`)) { return }
    applied,history:=material_asset_execute(&owner,{action=.Apply,path="resources/materials/captured.katmat",entities={b}}); defer editor.tool_result_destroy(&applied); defer editor.undo_group_destroy(&history)
    if !testing.expect(t,applied.error==.None) { return }
    copy,_:=ecs.get_component(&owner.world,b,Material_Images); testing.expect(t,copy.roles[0].digest==first.roles[0].digest && raw_data(copy.roles[0].image.pixels)!=raw_data(first.roles[0].image.pixels))
    testing.expect_value(t,os.write_entire_file(path,"corrupt revision"),os.Error(nil))
    testing.expect_value(t,editor.undo_group(&owner.world,&owner.registry,&history),editor.Scene_Error.None)
    testing.expect_value(t,editor.redo_group(&owner.world,&owner.registry,&history),editor.Scene_Error.None)
    restored,_:=ecs.get_component(&owner.world,b,Material_Images); testing.expect(t,restored.roles[0].digest==first.roles[0].digest)
    changed,change_history:=material_execute(&owner,agent.Material_Set{entities={b},fields={.Roughness},values={roughness=.1}}); defer editor.tool_result_destroy(&changed); defer editor.undo_group_destroy(&change_history)
    original,_:=ecs.get_component(&owner.world,a,Surface_Material); edited,_:=ecs.get_component(&owner.world,b,Surface_Material); testing.expect(t,original.roughness==.5 && edited.roughness==.1)
    invalid,invalid_history:=material_asset_execute(&owner,{action=.Apply,path="resources/materials/captured.katmat",entities={a,b}}); defer editor.tool_result_destroy(&invalid); defer editor.undo_group_destroy(&invalid_history); testing.expect(t,invalid.error!=.None)
    after,_:=ecs.get_component(&owner.world,b,Surface_Material); testing.expect(t,after.roughness==.1)
    replacement:=#load("../examples/texture_reload/fixtures/green.png",[]byte)
    testing.expect_value(t,os.write_entire_file(path,replacement),os.Error(nil))
    refreshed,refresh_history:=material_texture_execute(&owner,{entities={a},role=.Albedo,source={kind=.File,path="images/source.png",root=.Resource}}); defer editor.tool_result_destroy(&refreshed); defer editor.undo_group_destroy(&refresh_history)
    testing.expect_value(t,refreshed.error,editor.Scene_Error.None)
    revision,_:=ecs.get_component(&owner.world,a,Material_Images); testing.expect(t,revision.roles[0].digest!=first.roles[0].digest)
    testing.expect_value(t,editor.undo_group(&owner.world,&owner.registry,&refresh_history),editor.Scene_Error.None)
    previous,_:=ecs.get_component(&owner.world,a,Material_Images); testing.expect(t,previous.roles[0].digest==first.roles[0].digest)
    testing.expect_value(t,os.write_entire_file(path,bytes),os.Error(nil))
    snapshot,capture_error:=scene_snapshot_capture(&owner); if !testing.expect(t,capture_error==.None) { return }; defer scene_snapshot_destroy(&snapshot)
    document,encode_error:=scene_document_encode(&owner,&snapshot,"Materials","outside.katla"); if !testing.expect(t,encode_error==.None) { return }; defer json.destroy_value(document)
    text,marshal_error:=json.marshal(document); defer delete(text); testing.expect(t,marshal_error==nil && strings.contains(string(text),`"textures"`) && !strings.contains(string(text),"MaterialImages"))
    read,read_error:=scene_document_decode(&owner,document,"outside.katla"); if !testing.expect(t,read_error==.None) { return }; defer scene_snapshot_destroy(&read)
    testing.expect_value(t,scene_snapshot_restore(&owner,&read),editor.Scene_Error.None)
}

@(test)
test_material_asset_strict_document_and_sampler_failures :: proc(t:^testing.T) {
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner)
    owner.mode=.Playing
    blocked,blocked_group:=material_asset_execute(&owner,{action=.Capture,path="blocked.katmat",entity=0}); testing.expect_value(t,blocked.error,editor.Scene_Error.Editing_Required); editor.tool_result_destroy(&blocked); editor.undo_group_destroy(&blocked_group); owner.mode=.Editing
    described,group:=material_asset_execute(&owner,{action=.Describe}); defer editor.tool_result_destroy(&described); defer editor.undo_group_destroy(&group)
    tree,error:=json.parse(described.data,parse_integers=true); if !testing.expect(t,error==nil) { return }; defer json.destroy_value(tree)
    example:=tree.(json.Object)["example"].(json.Object)
    parsed,parse_error:=material_asset_document_decode(&owner,example,"resources/materials/a.katmat"); testing.expect(t,parse_error==.None); if parse_error==.None { material_asset_destroy(&parsed) }
    textures:=example["textures"].(json.Object); albedo:=textures["albedo"].(json.Object)
    old:=albedo["kind"]; albedo["kind"]=string("inherit"); invalid,invalid_error:=material_asset_document_decode(&owner,example,"a.katmat"); testing.expect(t,invalid_error!=.None); if invalid_error==.None { material_asset_destroy(&invalid) }; albedo["kind"]=old
    sampling:=example["sampling"].(json.Object); normal:=sampling["normal"].(json.Object); uv:=normal["uv"].(json.Object); uv["tex_coord"]=json.Integer(2)
    invalid,invalid_error=material_asset_document_decode(&owner,example,"a.katmat"); testing.expect(t,invalid_error!=.None); if invalid_error==.None { material_asset_destroy(&invalid) }
}

@(test)
test_material_document_exact_mip_modes_and_large_finite_values :: proc(t:^testing.T) {
    for mode in ([3]gfx.Mip_Filter{.None,.Nearest,.Linear}) {
        sampling:=material_sampling_default(); sampling.normal.sampler.mip_filter=mode; sampling.normal.sampler.max_lod=0 if mode==.None else 32
        sampling.normal.uv={1,{2e7,-3e7},4e7,{-5e7,0}}
        tree:=material_sampling_encode(sampling); defer json.destroy_value(tree)
        restored,valid:=material_sampling_decode(tree); testing.expect(t,valid && restored.normal==sampling.normal)
    }
    surface:=material_surface_default(); surface.emissive_factor={2e7,0,3e7}; surface.normal_scale= -4e7; surface.alpha_cutoff=5e7
    tree:=material_surface_encode(surface); defer json.destroy_value(tree)
    restored,valid:=material_surface_decode(tree); testing.expect(t,valid && restored==surface)
}

@(test)
test_material_image_cache_identity_includes_shape_and_sample_precision :: proc(t:^testing.T) {
    pixels:=[16]byte{1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16}
    square:=image.Texture_Image{width=2,height=2,pixels=pixels[:],format=.RGBA8}
    strip:=image.Texture_Image{width=4,height=1,pixels=pixels[:],format=.RGBA8}
    precise:=image.Texture_Image{width=2,height=1,pixels=pixels[:],format=.RGBA16}
    testing.expect(t,material_image_digest(&square)!=material_image_digest(&strip) && material_image_digest(&strip)!=material_image_digest(&precise) && material_image_digest(&square)==material_image_digest(&square))
}
