#+test
package app

import km "../math"
import "core:math"
import "core:testing"

@(test)
test_gltf_weighted_skin_preserves_geometric_normal_and_mirrored_handedness :: proc(t:^testing.T) {
    source_normal:=km.normalize(km.Vec3{1,1,1})
    source_tangent:=km.normalize(km.Vec3{1,-1,0})
    source_bitangent:=km.cross(source_normal,source_tangent)
    vertices:=[3]Mesh_Vertex{
        {position={0,0,0},normal=source_normal,tangent={source_tangent[0],source_tangent[1],source_tangent[2],-1}},
        {position=source_tangent,normal=source_normal,tangent={source_tangent[0],source_tangent[1],source_tangent[2],-1}},
        {position=source_bitangent,normal=source_normal,tangent={source_tangent[0],source_tangent[1],source_tangent[2],-1}},
    }
    indices:=[3]u32{0,1,2}; joints:=[3][4]u16{{0,1,0,0},{0,1,0,0},{0,1,0,0}}
    weights:=[3][4]f32{{0.25,0.75,0,0},{0.25,0.75,0,0},{0.25,0.75,0,0}}
    primitives:=[1]Gltf_Primitive{{mesh=0,geometry={vertices=vertices[:],indices=indices[:]},joints=joints[:],weights=weights[:]}}
    nodes:=[3]Gltf_Node{{mesh=0,skin=0},{mesh=-1,skin=-1},{mesh=-1,skin=-1}}
    joint_nodes:=[2]u32{1,2}; inverse_bind:=[2]km.Mat4{km.identity(km.Mat4),km.identity(km.Mat4)}
    skins:=[1]Gltf_Skin{{joints=joint_nodes[:],inverse_bind=inverse_bind[:]}}
    model:=Gltf_Model{nodes=nodes[:],skins=skins[:],primitives=primitives[:]}
    for mirrored in ([2]bool{false,true}) {
        orientation:f32=1; if mirrored { orientation=-1 }
        world:=[3]km.Mat4{km.identity(km.Mat4),{{10*orientation,0,0,0},{1,1,0,0},{0,0,1,0},{0,0,0,1}},{{orientation,0,0,0},{0,10,0,0},{0,1.5,1,0},{0,0,0,1}}}
        geometry,error:=gltf_deform_geometry(&model,0,0,world[:]); defer mesh_geometry_destroy(&geometry)
        testing.expect_value(t,error,Gltf_Error.None); if error!=.None { continue }
        expected_linear:=km.Mat3{{3.25*orientation,0,0},{0.25,7.75,0},{0,1.125,1}}
        transformed_tangent:=km.matrix_vector(expected_linear,source_tangent)
        transformed_bitangent:=km.matrix_vector(expected_linear,source_bitangent)
        expected_normal:=km.normalize(km.cross(transformed_tangent,transformed_bitangent)*orientation)
        inverse0,_:=km.inverse(km.mat4_to_mat3(world[1])); inverse1,_:=km.inverse(km.mat4_to_mat3(world[2]))
        old_normal:=km.normalize(km.matrix_vector(km.transpose(inverse0),source_normal)*0.25+km.matrix_vector(km.transpose(inverse1),source_normal)*0.75)
        testing.expect(t,km.length_squared(old_normal-expected_normal)>0.01)
        testing.expect(t,km.length_squared(geometry.vertices[1].position-transformed_tangent)<0.0000001)
        testing.expect(t,km.length_squared(geometry.vertices[2].position-transformed_bitangent)<0.0000001)
        for vertex in geometry.vertices {
            tangent:=km.xyz(vertex.tangent)
            testing.expect(t,km.length_squared(vertex.normal-expected_normal)<0.0000001)
            testing.expect(t,math.abs(km.dot(vertex.normal,transformed_tangent))<0.000001)
            testing.expect(t,math.abs(km.dot(vertex.normal,transformed_bitangent))<0.000001)
            testing.expect(t,math.abs(km.dot(vertex.normal,tangent))<0.000001)
            testing.expect(t,math.abs(km.length_squared(tangent)-1)<0.000001)
            testing.expect_value(t,vertex.tangent[3],-orientation)
        }
    }
}
