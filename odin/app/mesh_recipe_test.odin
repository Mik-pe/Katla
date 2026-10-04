#+test
#+build darwin, linux
package app

import resources "../resources"
import ron "../encoding/ron"
import "core:testing"
import "core:encoding/json"

@(test)
test_current_chair_recipe_compiles_real_file_geometry_and_bounds :: proc(t:^testing.T) {
    root,open_error:=resources.root_open("resources"); testing.expect_value(t,open_error,resources.Error.None); defer resources.root_destroy(&root)
    mesh,err:=mesh_recipe_load(&root,"meshes/chair-frame.katmesh"); defer mesh_geometry_destroy(&mesh)
    testing.expect(t,err==.None && len(mesh.vertices)==144 && len(mesh.indices)==216)
    testing.expect(t,abs((mesh.bounds.center-mesh.bounds.extent)[0]+0.4)<0.00001 && abs((mesh.bounds.center+mesh.bounds.extent)[1]-1.35)<0.00001)
    source:string=`(version:1,name:"Bad",parts:[(id:"part",geometry:(kind:"cube",size:(1,1,1))),(id:"part",geometry:(kind:"cube",size:(1,1,1)))])`
    tree,parse_error:=ron.parse(source); defer json.destroy_value(tree); testing.expect_value(t,parse_error.kind,ron.Error_Kind.None)
    rejected,duplicate_error:=mesh_recipe_compile(tree); defer mesh_geometry_destroy(&rejected); testing.expect_value(t,duplicate_error,Mesh_Error.Invalid_Geometry)
    outside,outside_error:=mesh_recipe_load(&root,"../resources/meshes/chair-frame.katmesh"); defer mesh_geometry_destroy(&outside); testing.expect_value(t,outside_error,Mesh_Error.Invalid_Geometry)
}

@(test)
test_recipe_all_current_shapes_and_budget_preflight :: proc(t:^testing.T) {
    source:string=`(version:1,name:"Shapes",parts:[
        (id:"plane",geometry:(kind:"plane",width:2,height:2)),
        (id:"sphere",geometry:(kind:"sphere",radius:1,segments:8,rings:4)),
        (id:"cylinder",geometry:(kind:"cylinder",radius:1,height:2,segments:8)),
        (id:"cone",geometry:(kind:"cone",radius:1,height:2,segments:8)),
        (id:"torus",geometry:(kind:"torus",radius:2,tube_radius:0.5,segments:8,tube_segments:4)),
        (id:"triangle",geometry:(kind:"triangles",positions:[(0,0,0),(1,0,0),(0,1,0)],indices:[0,1,2])),
    ])`
    tree,err:=ron.parse(source); defer json.destroy_value(tree); testing.expect_value(t,err.kind,ron.Error_Kind.None)
    mesh,compile_error:=mesh_recipe_compile(tree); defer mesh_geometry_destroy(&mesh); testing.expect(t,compile_error==.None && len(mesh.vertices)>100 && len(mesh.indices)>300)
    huge:string=`(version:1,name:"Huge",parts:[(id:"sphere",geometry:(kind:"sphere",radius:1,segments:1000000,rings:1000000))])`
    huge_tree,huge_parse_error:=ron.parse(huge); defer json.destroy_value(huge_tree); testing.expect_value(t,huge_parse_error.kind,ron.Error_Kind.None)
    invalid,budget_error:=mesh_recipe_compile(huge_tree); defer mesh_geometry_destroy(&invalid); testing.expect(t,budget_error!=.None && len(invalid.vertices)==0)
}
