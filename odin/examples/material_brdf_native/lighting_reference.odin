//! Independent affine-frame and complete lighting reference; production skinning supplies real geometry.
package main

import app "../../app"
import render "../../app/render"
import km "../../math"
import "core:math"

Lighting_Case :: struct { metallic,roughness,scale,ao:f32,emission:[3]f32,points,shadowed,textured:bool }
lighting_cases :: proc()->[10]Lighting_Case { return {
    {0,.1,1,1,{},false,false,false},{0,.4,1,1,{},false,false,false},{1,1,1,1,{},false,false,false},
    {.5,.4,1,.3,{},true,false,false},{.5,.4,1,.3,{},true,true,false},{.2,.7,0,1,{},false,false,true},
    {.2,.7,2,1,{},true,false,true},{.2,.7,-1,1,{},false,false,true},{0,1,1,0,{4,.5,2},false,true,false},{0,.4,0,1,{},false,false,false},
} }
lighting_transforms :: proc()->[3]km.Mat4 { return {km.identity(km.Mat4),{{2,0,0,0},{.4,.5,0,0},{.3,-.2,1.5,0},{0,0,0,1}},{{-2,0,0,0},{.4,.5,0,0},{.3,-.2,1.5,0},{0,0,0,1}}} }
cross3 :: proc(a,b:Vec3d)->Vec3d { return {a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]} }
frame_reference :: proc(affine:km.Mat4,scale:f32,textured:bool)->Vec3d {
    columns:[3]Vec3d;for &column,i in columns { for &value,j in column { value=f64(affine[i][j]) } }
    n0:=unit3({1,1,1});t0:=unit3({1,-1,0});cofactors:=[3]Vec3d{cross3(columns[1],columns[2]),cross3(columns[2],columns[0]),cross3(columns[0],columns[1])}
    sign:f64=1;if dot3(columns[0],cofactors[0])<0 { sign= -1 }
    normal,tangent:Vec3d
    for row in 0..<3 { for col in 0..<3 { normal[row]+=cofactors[col][row]*n0[col]*sign;tangent[row]+=columns[col][row]*t0[col] } }
    normal=unit3(normal);projection:=dot3(normal,tangent);for &value,row in tangent { value-=normal[row]*projection };tangent=unit3(tangent)
    mapped_normal:=Vec3d{0,0,1};if textured { mapped_normal=unit3({.5*f64(scale),0,1}) }
    result:Vec3d;for &value,row in result { value=tangent[row]*mapped_normal[0]+normal[row]*mapped_normal[2] };return unit3(result)
}
light_reference :: proc(normal,view,light,base:Vec3d,metal,rough:f64)->Vec3d {
    nv,nl:=clamp(dot3(normal,view),0,1),clamp(dot3(normal,light),0,1);if nv<=0 || nl<=0 { return {} }
    half:=unit3({view[0]+light[0],view[1]+light[1],view[2]+light[2]});nh,vh:=clamp(dot3(normal,half),0,1),clamp(dot3(view,half),0,1)
    a:=clamp(rough,f64(f32(.04)),1);a=a*a*a*a;denominator:=(1-nh*nh)+nh*nh*a
    distribution:=a/(math.PI*denominator*denominator);visibility:=.5/(nl*math.sqrt(nv*nv*(1-a)+a)+nv*math.sqrt(nl*nl*(1-a)+a))
    result:Vec3d;for &value,i in result { f0:=.04*(1-metal)+base[i]*metal;f:=f0+(1-f0)*math.pow(1-vh,5);value=((1-f)*(1-metal)*base[i]/math.PI+distribution*visibility*f)*nl };return result
}
complete_reference :: proc(affine:km.Mat4,sample:Lighting_Case)->Vec3d {
    normal:=frame_reference(affine,sample.scale,sample.textured);base:=Vec3d{f64(f32(.6)),f64(f32(.3)),f64(f32(.15))};sun:=unit3({.5,.25,1});result:Vec3d
    for &value,i in result { value=f64(f32(.15))*base[i]*f64(sample.ao)+f64(sample.emission[i]) }
    colors:=[3]f32{1,.8,.6}
    if !sample.shadowed { direct:=light_reference(normal,{0,0,1},sun,base,f64(sample.metallic),f64(sample.roughness));for &value,i in result { value+=direct[i]*f64(colors[i])*f64(f32(1.7)) } }
    if sample.points {
        lights:=[2]render.Point_Light_GPU{{{2,1,4.5},8,{.8,.3,.1},2},{{-1,2,3.5},6,{.1,.4,.9},1.5}}
        for light in lights { delta:=Vec3d{f64(light.position[0]),f64(light.position[1]),f64(light.position[2])};distance:=math.sqrt(dot3(delta,delta));attenuation:=math.pow(max(1-distance/f64(light.range),0),2);direct:=light_reference(normal,{0,0,1},unit3(delta),base,f64(sample.metallic),f64(sample.roughness));for &value,i in result { value+=direct[i]*f64(light.color[i])*f64(light.intensity)*attenuation } }
    };return result
}
lighting_geometry :: proc(transform:km.Mat4,path:int)->([3]render.Model_Vertex,km.Mat4) {
    identity:=km.identity(km.Mat4);object:=identity;source:[3]app.Mesh_Vertex
    positions:=[3]km.Vec3{{-4,-4,0},{4,-4,0},{0,4,0}};normal:=km.normalize(km.Vec3{1,1,1});tangent:=km.normalize(km.Vec3{1,-1,0})
    for &vertex,i in source { vertex={positions[i],normal,{.25,.25},{tangent[0],tangent[1],tangent[2],-1}} }
    output:=source
    if path==1 { object=transform }
    else if path==0 {
        inverse,ok:=km.inverse(km.mat4_to_mat3(transform));assert(ok);normal_matrix:=km.transpose(inverse);orientation:f32=1;if km.determinant(km.mat4_to_mat3(transform))<0 { orientation= -1 }
        for &vertex in output { vertex.position=km.xyz(km.matrix_vector(transform,km.vec4(vertex.position,1)));vertex.normal=km.normalize(km.matrix_vector(normal_matrix,vertex.normal));direction:=km.matrix_vector(km.mat4_to_mat3(transform),km.xyz(vertex.tangent));direction=km.normalize(direction-vertex.normal*km.dot(vertex.normal,direction));vertex.tangent={direction[0],direction[1],direction[2],vertex.tangent[3]*orientation} }
    } else {
        // These paths use the real imported skin deformer, including model/joint composition.
        joints:=[3][4]u16{{0,1,0,0},{0,1,0,0},{0,1,0,0}};weights:=[3][4]f32{{.25,.75,0,0},{.25,.75,0,0},{.25,.75,0,0}};indices:=[3]u32{0,1,2};joint_ids:=[2]u32{1,2};bind:=[2]km.Mat4{identity,identity}
        nodes:=[3]app.Gltf_Node{{mesh=0,skin=0},{mesh= -1,skin= -1},{mesh= -1,skin= -1}};skin:=[1]app.Gltf_Skin{{joints=joint_ids[:],inverse_bind=bind[:]}}
        primitive:=[1]app.Gltf_Primitive{{mesh=0,geometry={vertices=source[:],indices=indices[:],allocator=context.allocator},joints=joints[:],weights=weights[:]}}
        model:=app.Gltf_Model{primitives=primitive[:],nodes=nodes[:],skins=skin[:]};world:=[3]km.Mat4{identity,transform,transform}
        world[1][1][0]+=.3;world[2][1][0]-=.1
        if path==3 { object={{.5,0,0,0},{0,2,0,0},{0,0,1,0},{0,0,0,1}};world[0]=object }
        deformed,error:=app.gltf_deform_geometry(&model,0,0,world[:]);assert(error==.None);defer app.mesh_geometry_destroy(&deformed)
        for vertex,i in deformed.vertices { output[i]=vertex }
    }
    // Mirrored submitted geometry must preserve the authored front face.
    order:=[3]int{0,1,2};if km.determinant(km.mat4_to_mat3(transform))<0 { order={0,2,1} }
    vertices:[3]render.Model_Vertex;for &vertex,i in vertices { value:=output[order[i]];vertex={position=km.vec4(value.position,1),normal=km.vec4(value.normal),tangent=value.tangent,color={1,1,1,1}};for &uv in vertex.uvs { uv={.25,.25,1,0} } }
    return vertices,object
}
