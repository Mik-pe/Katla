#+test
#+build darwin, linux
package app
import asset "../agent/assets"
import ecs "../ecs"
import editor "../editor"
import resources "../resources"
import km "../math"
import "core:testing"
import "core:os"
import "core:strings"
import "core:encoding/json"
import ron "../encoding/ron"

STL_TEST_ASCII :: `solid test
facet normal 0 0 1
outer loop
vertex 0 0 0
vertex 1 0 0
vertex 0 1 0
endloop
endfacet
endsolid test
`
@(test)
test_stl_ascii_binary_normals_dedup_and_nonfinite_triangle_filter :: proc(t:^testing.T) {
    text:string=STL_TEST_ASCII
    mesh,error:=stl_decode(transmute([]byte)text); defer mesh_geometry_destroy(&mesh); testing.expect_value(t,error,Mesh_Error.None)
    testing.expect(t,len(mesh.vertices)==3 && len(mesh.indices)==3 && mesh.vertices[0].normal==km.VEC3_Z && km.aabb_max(mesh.bounds)==km.Vec3{1,1,0})
    bytes:[184]byte; copy(bytes[:],"solid binary header"); bytes[80]=2
    put :: proc(bytes:[]byte,offset:int,number:f32) { bits:=transmute(u32)number; for i in 0..<4 { bytes[offset+i]=byte(bits>>uint(i*8)) } }
    for triangle in 0..<2 {
        record:=84+triangle*50; put(bytes[:],record+8,1)
        positions:=[3]km.Vec3{{0,0,0},{1,0,0},{0,1,0}}
        for vertex,i in positions { for coordinate,axis in vertex { put(bytes[:],record+12+i*12+axis*4,coordinate) } }
    }
    binary,binary_error:=stl_decode(bytes[:]); defer mesh_geometry_destroy(&binary); testing.expect_value(t,binary_error,Mesh_Error.None); testing.expect(t,len(binary.vertices)==3 && len(binary.indices)==6)
    put(bytes[:],134,transmute(f32)u32(0x7fc00000))
    filtered,filtered_error:=stl_decode(bytes[:]); defer mesh_geometry_destroy(&filtered); testing.expect_value(t,filtered_error,Mesh_Error.None); testing.expect_value(t,len(filtered.indices),3)
    bytes[80]=255; rejected,rejected_error:=stl_decode(bytes[:]); defer mesh_geometry_destroy(&rejected); testing.expect_value(t,rejected_error,Mesh_Error.Invalid_Geometry)
    bad:string="solid malformed\nvertex 0 0 0\nendsolid malformed"; bad_mesh,bad_error:=stl_decode(transmute([]byte)bad); defer mesh_geometry_destroy(&bad_mesh); testing.expect_value(t,bad_error,Mesh_Error.Invalid_Geometry)
}

@(test)
test_stl_real_scene_source_save_load_prepares_geometry_and_rejects_atomic_replacement :: proc(t:^testing.T) {
    directory,error:=os.make_directory_temp("","katla-stl-scene-*",context.allocator); testing.expect(t,error==nil); if error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    resource_path:=strings.concatenate({directory,"/resources"}); defer delete(resource_path); testing.expect(t,os.make_directory(resource_path)==nil)
    model_path:=strings.concatenate({resource_path,"/triangle.stl"}); defer delete(model_path); testing.expect(t,os.write_entire_file(model_path,STL_TEST_ASCII)==nil)
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner); authoring_services_init(&owner)
    testing.expect_value(t,asset_resources_init(&owner,directory,resource_path),resources.Error.None)
    wire:string=`(version:3,name:"STL",next_entity_id:2,entities:[(id:1,name:"Triangle",transform:(),source:StlModel(path:Resource("triangle.stl")),drawable:(color:[1.0,0.5,0.25,1.0],metallic:0.0,roughness:0.5,ao:1.0))])`
    document,parse_error:=ron.parse(wire); testing.expect(t,parse_error.kind==.None); defer json.destroy_value(document)
    snapshot,decode_error:=scene_document_decode(&owner,document,"scene.katla"); testing.expect_value(t,decode_error,editor.Scene_Error.None); defer scene_snapshot_destroy(&snapshot); if decode_error!=.None { return }
    restore_error:=scene_snapshot_restore(&owner,&snapshot); testing.expect_value(t,restore_error,editor.Scene_Error.None)
    ids:=ecs.entity_ids(&owner.world); defer delete(ids); testing.expect_value(t,len(ids),1); mesh,present:=ecs.get_component(&owner.world,ids[0],Scene_Mesh); testing.expect(t,present && mesh.source.kind==.Stl && len(mesh.geometry.vertices)==3)
    saved,undo:=scene_file_execute(&owner,{action=.Save,path="scene.katla",has_path=true}); defer editor.tool_result_destroy(&saved); defer editor.undo_group_destroy(&undo); testing.expect_value(t,saved.error,editor.Scene_Error.None)
    roots:=ecs.get_resource_mut(&owner.world,Asset_Roots); bytes,read_error:=resources.read_text(&roots.project,"scene.katla"); defer delete(bytes); testing.expect_value(t,read_error,resources.Error.None)
    testing.expect(t,strings.contains(string(bytes),"StlModel") && strings.contains(string(bytes),"Resource(\"triangle.stl\""))
    testing.expect(t,os.write_entire_file(model_path,"solid broken\nvertex bad\nendsolid broken")==nil)
    failed,_:=scene_file_execute(&owner,{action=.Load,path="scene.katla",has_path=true}); defer editor.tool_result_destroy(&failed); testing.expect(t,failed.error!=.None && ecs.entity_exists(&owner.world,ids[0]))
    retained,_:=ecs.get_component(&owner.world,ids[0],Scene_Mesh); testing.expect_value(t,len(retained.geometry.indices),3)
    testing.expect(t,os.write_entire_file(model_path,STL_TEST_ASCII)==nil)
    loaded,_:=scene_file_execute(&owner,{action=.Load,path="scene.katla",has_path=true}); defer editor.tool_result_destroy(&loaded); testing.expect_value(t,loaded.error,editor.Scene_Error.None); testing.expect(t,!ecs.entity_exists(&owner.world,ids[0]))
    request:=asset.Resource_Write_Request{action=.Write,path="resources/triangle.stl",content=STL_TEST_ASCII}; modified,_:=resource_write_execute(&owner,request); defer editor.tool_result_destroy(&modified); testing.expect_value(t,modified.error,editor.Scene_Error.None)
}
