#+test
package app

import resources "../resources"
import km "../math"
import "core:testing"
import "core:mem"
import "core:fmt"
import "core:os"
import "core:strings"
import "core:encoding/base64"
import ecs "../ecs"
import editor "../editor"

GLTF_RESOURCE_ROOT :: #config(GLTF_RESOURCE_ROOT,"resources")

@(test)
test_gltf_actual_repository_models_geometry_images_and_owned_animation :: proc(t:^testing.T) {
    backing:=context.allocator; tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,backing)
    context.allocator=mem.tracking_allocator(&tracker)
    defer { testing.expect_value(t,len(tracker.allocation_map),0); context.allocator=backing; mem.tracking_allocator_destroy(&tracker) }
    root,root_error:=resources.root_open(GLTF_RESOURCE_ROOT); testing.expect_value(t,root_error,resources.Error.None)
    if root_error!=.None { return }; defer resources.root_destroy(&root)
    for path in ([]string{"models/Box.gltf","models/Box.glb","models/BoxInterleaved.glb","models/DamagedHelmet.glb","models/Avocado.glb","models/Lantern.glb","models/Fox.glb","models/FoxFixed.glb","models/FoxBlender.glb","models/Tiger.glb","models/Regular_plane.glb"}) {
        model,error:=gltf_load(&root,path); defer gltf_model_destroy(&model)
        if path=="models/FoxBlender.glb" { testing.expect_value(t,error,Gltf_Error.Invalid_Skin); continue }
        testing.expect(t,error==.None,fmt.tprintf("%s: %v",path,error))
        if error!=.None { continue }
        testing.expect(t,len(model.primitives)>0 && len(model.nodes)>0)
        for primitive in model.primitives {
            testing.expect(t,len(primitive.geometry.vertices)>0 && len(primitive.geometry.indices)>0 && len(primitive.geometry.indices)%3==0)
            for index in primitive.geometry.indices { testing.expect(t,int(index)<len(primitive.geometry.vertices)) }
        }
        for image in model.images { testing.expect(t,len(image.encoded)>0 && (image.mime=="image/png" || image.mime=="image/jpeg")) }
        if path=="models/DamagedHelmet.glb" { testing.expect(t,len(model.images)>=5 && len(model.materials)>0 && model.materials[0].normal_texture.texture>=0 && model.materials[0].metallic_roughness_texture.texture>=0) }
        if path=="models/Tiger.glb" { testing.expect(t,len(model.materials)>0 && model.materials[0].workflow==.Specular_Glossiness && model.materials[0].diffuse_texture.texture>=0) }
        if path=="models/Fox.glb" {
            testing.expect(t,len(model.skins)>0 && len(model.animation.clips)>=3 && animation_model_valid(&model.animation))
            player:=animation_player_stopped(); player.clip=model.animation.clips[0].name; player.time=model.animation.clips[0].duration*0.5
            world,world_error:=gltf_world_matrices(&model,&player); defer delete(world)
            testing.expect(t,world_error==.None)
            deformed:=false
            for node,i in model.nodes {
                if node.skin<0 { continue }
                for primitive,j in model.primitives {
                    if primitive.mesh!=u32(node.mesh) { continue }
                    geometry,geometry_error:=gltf_deform_geometry(&model,u32(j),u32(i),world); defer mesh_geometry_destroy(&geometry)
                    testing.expect(t,geometry_error==.None)
                    if geometry_error!=.None { continue }
                    for vertex,k in geometry.vertices { if km.length_squared(vertex.position-primitive.geometry.vertices[k].position)>0.001 { deformed=true; break } }
                }
            }
            testing.expect(t,deformed,"Actual Fox animation must deform imported mesh vertices")
        }
    }
}

@(test)
test_gltf_confined_dependency_uris_and_percent_encoding :: proc(t:^testing.T) {
    for example in ([]struct{uri,path:string,expected:Gltf_Error}{
        {"Box_data.bin","models/Box_data.bin",.None},
        {"../textures/coat%20color.png","textures/coat color.png",.None},
        {"../../escape.bin","",.Invalid_Path},
        {"%2foutside.bin","",.Invalid_Path},
        {"%00bad.bin","",.Invalid_Path},
        {"bad%GG.bin","",.Invalid_Path},
        {"https://example.com/mesh.bin","",.Invalid_Path},
    }) {
        path,error:=gltf_dependency_path("models/Box.gltf",example.uri); defer delete(path)
        testing.expect_value(t,error,example.expected); testing.expect_value(t,path,example.path)
    }
}

@(private="package")
gltf_test_u32 :: proc(bytes:[]byte,offset:int,value:u32) { for i in 0..<4 { bytes[offset+i]=byte(value>>u32(i*8)) } }
@(private="package")
gltf_test_f32 :: proc(bytes:[]byte,offset:int,value:f32) { gltf_test_u32(bytes,offset,transmute(u32)value) }
@(private="package")
gltf_sparse_fixture :: proc()->string {
    bytes:[200]byte
    bytes[0]=0; bytes[1]=1; bytes[2]=2; bytes[40]=0; bytes[41]=1; bytes[42]=2
    positions:=[9]f32{-0.5,-0.5,0,0.5,-0.5,0,0,0.5,0}
    for value,i in positions { gltf_test_f32(bytes[:],4+i*4,value) }
    bytes[48]=255; bytes[49]=255; bytes[54]=255; bytes[55]=255
    for i in 0..<3 { bytes[68+i*4]=255; gltf_test_f32(bytes[:],104+i*12,1); gltf_test_f32(bytes[:],140+i*12+4,1) }
    gltf_test_f32(bytes[:],84,1); gltf_test_f32(bytes[:],92,1); gltf_test_f32(bytes[:],96,1); gltf_test_f32(bytes[:],196,2)
    encoded,_:=base64.encode(bytes[:]); defer delete(encoded)
    template:=`{
"asset":{"version":"2.0"},"buffers":[{"byteLength":200,"uri":"data:application/octet-stream;base64,%s"}],
"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":3},{"buffer":0,"byteOffset":4,"byteLength":36},{"buffer":0,"byteOffset":40,"byteLength":3},{"buffer":0,"byteOffset":44,"byteLength":12},{"buffer":0,"byteOffset":56,"byteLength":12},{"buffer":0,"byteOffset":68,"byteLength":12},{"buffer":0,"byteOffset":80,"byteLength":8},{"buffer":0,"byteOffset":88,"byteLength":16},{"buffer":0,"byteOffset":104,"byteLength":36},{"buffer":0,"byteOffset":140,"byteLength":36},{"buffer":0,"byteOffset":176,"byteLength":24}],
"accessors":[{"componentType":5126,"count":3,"type":"VEC3","sparse":{"count":3,"indices":{"bufferView":0,"componentType":5121},"values":{"bufferView":1}}},{"bufferView":2,"componentType":5121,"count":3,"type":"SCALAR"},{"bufferView":3,"componentType":5123,"normalized":true,"count":3,"type":"VEC2"},{"bufferView":4,"componentType":5121,"count":3,"type":"VEC4"},{"bufferView":5,"componentType":5121,"normalized":true,"count":3,"type":"VEC4"},{"bufferView":6,"componentType":5126,"count":2,"type":"SCALAR"},{"bufferView":7,"componentType":5126,"count":4,"type":"SCALAR"},{"bufferView":8,"componentType":5126,"count":3,"type":"VEC3"},{"bufferView":9,"componentType":5126,"count":3,"type":"VEC3"},{"bufferView":10,"componentType":5126,"count":2,"type":"VEC3"}],
"meshes":[{"weights":[0,1],"primitives":[{"attributes":{"POSITION":0,"TEXCOORD_0":2,"JOINTS_0":3,"WEIGHTS_0":4},"indices":1,"targets":[{"POSITION":7},{"POSITION":8}]}]}],
"nodes":[{"mesh":0,"skin":0},{"name":"joint"}],"skins":[{"joints":[1]}],"scenes":[{"nodes":[0,1]}],"scene":0,
"animations":[{"name":"move","samplers":[{"input":5,"output":9,"interpolation":"LINEAR"},{"input":5,"output":6,"interpolation":"LINEAR"}],"channels":[{"sampler":0,"target":{"node":1,"path":"translation"}},{"sampler":1,"target":{"node":0,"path":"weights"}}]}]
}`
    output,_:=strings.replace_all(template,"%s",encoded); return output
}

@(test)
test_gltf_sparse_normalized_skin_and_arbitrary_morph_channels :: proc(t:^testing.T) {
    directory,dir_error:=os.make_directory_temp("","katla-gltf-sparse-*",context.allocator)
    testing.expect(t,dir_error==nil); if dir_error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    filename:=strings.concatenate({directory,"/model.gltf"}); defer delete(filename)
    source:=gltf_sparse_fixture(); defer delete(source)
    testing.expect_value(t,os.write_entire_file(filename,source),os.Error(nil))
    root,root_error:=resources.root_open(directory); testing.expect_value(t,root_error,resources.Error.None); if root_error!=.None { return }; defer resources.root_destroy(&root)
    original,error:=gltf_load(&root,"model.gltf"); testing.expect_value(t,error,Gltf_Error.None); if error!=.None { return }
    model:=gltf_model_clone(&original); defer gltf_model_destroy(&model)
    testing.expect(t,raw_data(model.primitives[0].geometry.vertices)!=raw_data(original.primitives[0].geometry.vertices) && raw_data(model.animation.clips[0].channels[1].weight_values)!=raw_data(original.animation.clips[0].channels[1].weight_values))
    gltf_model_destroy(&original)
    primitive:=&model.primitives[0]
    testing.expect_value(t,primitive.geometry.vertices[0].position,km.Vec3{-0.5,-0.5,0})
    testing.expect_value(t,primitive.geometry.vertices[1].uv,km.Vec2{1,0})
    testing.expect_value(t,primitive.geometry.vertices[2].uv,km.Vec2{0,1})
    testing.expect_value(t,primitive.weights[0],([4]f32{1,0,0,0}))
    player:=animation_player_stopped(); player.clip="move"; player.time=0.5
    world,world_error:=gltf_world_matrices(&model,&player); defer delete(world); testing.expect_value(t,world_error,editor.Scene_Error.None)
    weights,weight_error:=animation_sample_weights(&model.animation,&player,0,model.nodes[0].weights); defer delete(weights); testing.expect_value(t,weight_error,editor.Scene_Error.None)
    testing.expect(t,len(weights)==2 && weights[0]==0.5 && weights[1]==0.5)
    geometry,geometry_error:=gltf_deform_geometry(&model,0,0,world,weights); defer mesh_geometry_destroy(&geometry); testing.expect_value(t,geometry_error,Gltf_Error.None)
    if geometry_error==.None { testing.expect_value(t,geometry.vertices[0].position,km.Vec3{0,0,1}); testing.expect(t,len(geometry.indices)==3 && geometry.indices[0]==0 && geometry.indices[1]==1 && geometry.indices[2]==2) }
    malformed,_:=strings.replace_all(source,`"byteOffset":4,`, `"byteOffset":18446744073709551600,`); defer delete(malformed)
    testing.expect_value(t,os.write_entire_file(filename,malformed),os.Error(nil))
    rejected,rejected_error:=gltf_load(&root,"model.gltf"); defer gltf_model_destroy(&rejected); testing.expect_value(t,rejected_error,Gltf_Error.Invalid_Accessor)
}

@(test)
test_gltf_source_codec_snapshot_history_and_failed_revision_preserve_scene :: proc(t:^testing.T) {
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner)
    scene_components_register(&owner); scene_model_register(&owner)
    project_path:=strings.concatenate({GLTF_RESOURCE_ROOT,"/.."}); defer delete(project_path)
    testing.expect_value(t,asset_resources_init(&owner,project_path,GLTF_RESOURCE_ROOT),resources.Error.None)
    prepared,error:=scene_model_prepare(&owner,{path="models/Fox.glb"}); testing.expect_value(t,error,Gltf_Error.None); if error!=.None { return }
    entity:=ecs.spawn(&owner.world,struct {model:Scene_Model,transform:Scene_Transform}{prepared,{local=km.TRANSFORM_IDENTITY}})
    copied,history:=editor.scene_execute(&owner.world,&owner.registry,{kind=.Duplicate,entity=entity}); defer editor.tool_result_destroy(&copied); defer editor.undo_group_destroy(&history)
    testing.expect_value(t,copied.error,editor.Scene_Error.None)
    if copied.error==.None { original,_:=ecs.get_component(&owner.world,entity,Scene_Model); duplicate,_:=ecs.get_component(&owner.world,copied.entities[0],Scene_Model); testing.expect(t,raw_data(original.model.images[0].encoded)!=raw_data(duplicate.model.images[0].encoded) && len(duplicate.model.animation.clips)>0) }
    snapshot,capture_error:=scene_snapshot_capture(&owner); defer scene_snapshot_destroy(&snapshot); testing.expect_value(t,capture_error,editor.Scene_Error.None)
    testing.expect_value(t,scene_snapshot_restore(&owner,&snapshot),editor.Scene_Error.None)
    entry:=owner.registry.entries["SceneModel"]
    missing_source:string=`{"path":"models/absent.glb","root":"resource"}`
    rejected,accepted:=entry.decode_owned(entry.value_state,transmute([]byte)missing_source,owner.world.allocator)
    testing.expect(t,!accepted); scene_model_destroy(rejected); free(rejected,owner.world.allocator)
    ids:=ecs.entity_ids(&owner.world); defer delete(ids)
    testing.expect_value(t,len(ids),2)
    for id in ids { restored,present:=ecs.get_component(&owner.world,id,Scene_Model); testing.expect(t,present && restored.source.path=="models/Fox.glb" && len(restored.model.skins)>0 && len(restored.model.animation.clips)>0) }
}

@(test)
test_imported_small_unit_triangles_preserve_authored_geometry :: proc(t:^testing.T) {
    positions:=[3]km.Vec3{{0,0,0},{0.0001,0,0},{0,0.0001,0}}
    geometry,error:=mesh_triangles(positions[:],{0,1,2}); defer mesh_geometry_destroy(&geometry)
    testing.expect_value(t,error,Mesh_Error.None)
    if error==.None { testing.expect_value(t,geometry.vertices[0].normal,km.Vec3{0,0,1}); testing.expect_value(t,geometry.vertices[1].position,positions[1]) }
}

@(test)
test_gltf_exact_affine_matrices_survive_static_and_animated_nodes :: proc(t:^testing.T) {
    directory,dir_error:=os.make_directory_temp("","katla-gltf-matrix-*",context.allocator)
    testing.expect(t,dir_error==nil); if dir_error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    filename:=strings.concatenate({directory,"/model.gltf"}); defer delete(filename)
    original:=gltf_sparse_fixture(); defer delete(original)
    source,_:=strings.replace_all(original,`{"name":"joint"}],"skins"`,`{"name":"joint"},{"matrix":[-1,0,0,0,0.25,1,0,0,0,0,1,0,3,4,5,1],"children":[1]},{"matrix":[0,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1]}],"skins"`); defer delete(source)
    scene_source,_:=strings.replace_all(source,`"nodes":[0,1]`,`"nodes":[0,2,3]`); defer delete(scene_source)
    testing.expect_value(t,os.write_entire_file(filename,scene_source),os.Error(nil))
    root,root_error:=resources.root_open(directory); testing.expect_value(t,root_error,resources.Error.None); if root_error!=.None { return }; defer resources.root_destroy(&root)
    model,error:=gltf_load(&root,"model.gltf"); defer gltf_model_destroy(&model)
    testing.expect_value(t,error,Gltf_Error.None); if error!=.None { return }
    static,static_error:=gltf_world_matrices(&model,nil); defer delete(static)
    testing.expect_value(t,static_error,editor.Scene_Error.None)
    for node,i in model.nodes { testing.expect_value(t,static[i],node.world_matrix) }
    testing.expect_value(t,static[2][1][0],f32(0.25)); testing.expect_value(t,static[2][0][0],f32(-1)); testing.expect_value(t,static[3][0][0],f32(0))
    player:=animation_player_stopped(); player.clip="move"; player.time=0.5
    animated,animated_error:=gltf_world_matrices(&model,&player); defer delete(animated)
    testing.expect_value(t,animated_error,editor.Scene_Error.None)
    testing.expect_value(t,animated[2],static[2]); testing.expect_value(t,animated[3],static[3])
    testing.expect_value(t,km.xyz(animated[1][3]),km.Vec3{3,4,6})
    invalid_source,_:=strings.replace_all(scene_source,`"node":1,"path":"translation"`,`"node":2,"path":"translation"`); defer delete(invalid_source)
    testing.expect_value(t,os.write_entire_file(filename,invalid_source),os.Error(nil))
    rejected,rejected_error:=gltf_load(&root,"model.gltf"); defer gltf_model_destroy(&rejected)
    testing.expect_value(t,rejected_error,Gltf_Error.Invalid_Animation)
}
