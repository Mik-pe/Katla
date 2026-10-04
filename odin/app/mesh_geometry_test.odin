#+test
package app

import km "../math"
import "core:testing"

@(test)
test_static_mesh_cube_actual_faces_uvs_bounds_and_reflections :: proc(t:^testing.T) {
    mesh,err:=mesh_cube({2,4,6}); defer mesh_geometry_destroy(&mesh)
    testing.expect(t,err==.None && len(mesh.vertices)==24 && len(mesh.indices)==36)
    testing.expect_value(t,mesh.bounds,km.AABB{{0,0,0},{1,2,3}})
    for i:=0;i<len(mesh.indices);i+=3 {
        a,b,c:=mesh.vertices[mesh.indices[i]],mesh.vertices[mesh.indices[i+1]],mesh.vertices[mesh.indices[i+2]]
        testing.expect(t,km.dot(km.cross(b.position-a.position,c.position-a.position),a.normal)>0)
    }
    testing.expect_value(t,mesh_transform(&mesh,km.transform(position={2,0,0},scale={-2,1,1})),Mesh_Error.None)
    testing.expect_value(t,mesh.bounds,km.AABB{{2,0,0},{2,2,3}})
    for i:=0;i<len(mesh.indices);i+=3 {
        a,b,c:=mesh.vertices[mesh.indices[i]],mesh.vertices[mesh.indices[i+1]],mesh.vertices[mesh.indices[i+2]]
        testing.expect(t,km.dot(km.cross(b.position-a.position,c.position-a.position),a.normal)>0)
    }
    for vertex in mesh.vertices { testing.expect(t,km.is_normalized(vertex.normal) && abs(km.dot(vertex.normal,km.xyz(vertex.tangent)))<0.00001) }
    saved:=mesh.vertices[0]
    testing.expect_value(t,mesh_transform(&mesh,km.transform(scale={0,1,1})),Mesh_Error.Invalid_Transform)
    testing.expect_value(t,mesh.vertices[0],saved)
}

@(test)
test_static_triangles_generate_normals_and_preflight_errors :: proc(t:^testing.T) {
    positions:=[3]km.Vec3{{0,0,0},{1,0,0},{0,1,0}}; indices:=[3]u32{0,1,2}; uvs:=[3]km.Vec2{{0,0},{1,0},{0,1}}
    mesh,err:=mesh_triangles(positions[:],indices[:],uvs=uvs[:]); defer mesh_geometry_destroy(&mesh)
    testing.expect_value(t,err,Mesh_Error.None)
    for vertex in mesh.vertices { testing.expect_value(t,vertex.normal,km.Vec3{0,0,1}); testing.expect_value(t,vertex.tangent,km.Vec4{1,0,0,1}) }
    indices[2]=3
    invalid,index_error:=mesh_triangles(positions[:],indices[:]); defer mesh_geometry_destroy(&invalid); testing.expect_value(t,index_error,Mesh_Error.Invalid_Geometry)
    indices[2]=2; positions[2]={2,0,0}
    degenerate,degenerate_error:=mesh_triangles(positions[:],indices[:]); defer mesh_geometry_destroy(&degenerate); testing.expect_value(t,degenerate_error,Mesh_Error.Invalid_Geometry)
}

@(test)
test_static_sphere_has_non_degenerate_outward_poles_and_normals :: proc(t:^testing.T) {
    mesh,err:=mesh_sphere(1,24,12); defer mesh_geometry_destroy(&mesh)
    testing.expect(t,err==.None && len(mesh.vertices)==325 && len(mesh.indices)==1584)
    for vertex in mesh.vertices { testing.expect(t,abs(km.length(vertex.position)-1)<0.00001 && abs(km.length_squared(vertex.normal)-1)<0.00001) }
    for i:=0;i<len(mesh.indices);i+=3 {
        a,b,c:=mesh.vertices[mesh.indices[i]],mesh.vertices[mesh.indices[i+1]],mesh.vertices[mesh.indices[i+2]]
        testing.expect(t,km.dot(km.cross(b.position-a.position,c.position-a.position),a.normal+b.normal+c.normal)>0)
    }
}

@(test)
test_static_round_primitives_outward_surfaces_and_caps :: proc(t:^testing.T) {
    cylinder,cylinder_error:=mesh_cylinder(1,2,16); defer mesh_geometry_destroy(&cylinder)
    cone,cone_error:=mesh_cone(1,2,16); defer mesh_geometry_destroy(&cone)
    torus,torus_error:=mesh_torus(2,0.5,24,12); defer mesh_geometry_destroy(&torus)
    testing.expect(t,cylinder_error==.None && cone_error==.None && torus_error==.None)
    for mesh in ([3]Mesh_Geometry{cylinder,cone,torus}) {
        for i:=0;i<len(mesh.indices);i+=3 {
            a,b,c:=mesh.vertices[mesh.indices[i]],mesh.vertices[mesh.indices[i+1]],mesh.vertices[mesh.indices[i+2]]
            testing.expect(t,km.dot(km.cross(b.position-a.position,c.position-a.position),a.normal+b.normal+c.normal)>0)
        }
    }
}
