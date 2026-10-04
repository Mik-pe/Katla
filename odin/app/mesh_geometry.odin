//! Validated CPU geometry belongs to the application; GPU consumers receive ordinary vertex data.
package app

import km "../math"
import "core:mem"
import "core:slice"
import "core:math"

/// Static mesh attributes are authored in local space with CCW triangle winding.
Mesh_Vertex :: struct { position,normal:km.Vec3,uv:km.Vec2,tangent:km.Vec4 }
/// Owns indexed CPU geometry independently from native GPU resource lifetime.
Mesh_Geometry :: struct { vertices:[]Mesh_Vertex,indices:[]u32,bounds:km.AABB,allocator:mem.Allocator }
/// Rejects malformed/degenerate geometry before publication or resource allocation.
Mesh_Error :: enum { None, Invalid_Geometry, Limit, Invalid_Transform }
MAX_MESH_VERTICES :: 1_000_000
MAX_MESH_INDICES :: 6_000_000

/// Releases CPU attributes and indices with their captured allocator.
mesh_geometry_destroy :: proc(mesh:^Mesh_Geometry) { delete(mesh.vertices,mesh.allocator); delete(mesh.indices,mesh.allocator); mesh^={} }
/// Deep copies immutable CPU geometry for ownership transfer.
mesh_geometry_clone :: proc(mesh:^Mesh_Geometry,allocator:=context.allocator)->Mesh_Geometry { return {slice.clone(mesh.vertices,allocator),slice.clone(mesh.indices,allocator),mesh.bounds,allocator} }

@(private="package")
mesh_finite :: proc(value:f32)->bool { return !math.is_nan(value) && !math.is_inf(value) && abs(value)<=1_000_000 }
@(private="package")
mesh_vec_finite :: proc(value:[$N]f32)->bool { for number in value { if !mesh_finite(number) { return false } }; return true }
@(private="package")
mesh_tangent :: proc(normal:km.Vec3)->km.Vec4 {
    axis:=km.VEC3_Y; if abs(normal[1])>0.99 { axis=km.VEC3_X }
    tangent:=km.normalize(km.cross(axis,normal)); return {tangent[0],tangent[1],tangent[2],1}
}

/// Builds genuine indexed triangles, generating area-weighted normals and finite orthogonal tangents.
mesh_triangles :: proc(positions:[]km.Vec3,indices:[]u32,normals:[]km.Vec3=nil,uvs:[]km.Vec2=nil,allocator:=context.allocator)->(Mesh_Geometry,Mesh_Error) {
    if len(positions)<3 || len(indices)==0 || len(indices)%3!=0 { return {},.Invalid_Geometry }
    if len(positions)>MAX_MESH_VERTICES || len(indices)>MAX_MESH_INDICES { return {},.Limit }
    if (normals!=nil && len(normals)!=len(positions)) || (uvs!=nil && len(uvs)!=len(positions)) { return {},.Invalid_Geometry }
    for position in positions { if !mesh_vec_finite(position) { return {},.Invalid_Geometry } }
    for normal in normals { if !mesh_vec_finite(normal) || km.length_squared(normal)<0.000000000001 { return {},.Invalid_Geometry } }
    for uv in uvs { if !mesh_vec_finite(uv) { return {},.Invalid_Geometry } }
    for index in indices { if u64(index)>=u64(len(positions)) { return {},.Invalid_Geometry } }
    mesh:=Mesh_Geometry{vertices=make([]Mesh_Vertex,len(positions),allocator),indices=slice.clone(indices,allocator),allocator=allocator}
    success:=false; defer { if !success { mesh_geometry_destroy(&mesh) } }
    for position,i in positions { mesh.vertices[i].position=position; if normals!=nil { mesh.vertices[i].normal=km.normalize(normals[i]) }; if uvs!=nil { mesh.vertices[i].uv=uvs[i] } }
    tangents:=make([]km.Vec3,len(positions),allocator); defer delete(tangents,allocator)
    bitangents:=make([]km.Vec3,len(positions),allocator); defer delete(bitangents,allocator)
    for triangle:=0;triangle<len(indices);triangle+=3 {
        ia,ib,ic:=indices[triangle],indices[triangle+1],indices[triangle+2]
        a,b,c:=positions[ia],positions[ib],positions[ic]
        weighted:=km.cross(b-a,c-a)
        if km.length_squared(weighted)<0.000000000001 { return {},.Invalid_Geometry }
        if normals==nil { mesh.vertices[ia].normal+=weighted; mesh.vertices[ib].normal+=weighted; mesh.vertices[ic].normal+=weighted }
        if uvs!=nil {
            uv1,uv2:=uvs[ib]-uvs[ia],uvs[ic]-uvs[ia]
            divisor:=uv1[0]*uv2[1]-uv1[1]*uv2[0]
            if abs(divisor)>0.000000000001 {
                tangent:=((b-a)*uv2[1]-(c-a)*uv1[1])/divisor
                bitangent:=((c-a)*uv1[0]-(b-a)*uv2[0])/divisor
                for index in ([3]u32{ia,ib,ic}) { tangents[index]+=tangent; bitangents[index]+=bitangent }
            }
        }
    }
    for &vertex,i in mesh.vertices {
        if km.length_squared(vertex.normal)<0.000000000001 { return {},.Invalid_Geometry }
        vertex.normal=km.normalize(vertex.normal); vertex.tangent=mesh_tangent(vertex.normal)
        tangent:=tangents[i]-vertex.normal*km.dot(vertex.normal,tangents[i])
        if km.length_squared(tangent)>0.000000000001 {
            tangent=km.normalize(tangent); sign:f32=1
            if km.dot(km.cross(vertex.normal,tangent),bitangents[i])<0 { sign=-1 }
            vertex.tangent={tangent[0],tangent[1],tangent[2],sign}
        }
    }
    min_point,max_point:=positions[0],positions[0]
    for point in positions { for axis in 0..<3 { min_point[axis]=min(min_point[axis],point[axis]); max_point[axis]=max(max_point[axis],point[axis]) } }
    mesh.bounds=km.aabb_from_min_max(min_point,max_point); success=true; return mesh,.None
}

/// Generates a six-face cube with independent hard normals and UV seams.
mesh_cube :: proc(size:km.Vec3,allocator:=context.allocator)->(Mesh_Geometry,Mesh_Error) {
    for dimension in size { if !mesh_finite(dimension) || dimension<=0 { return {},.Invalid_Geometry } }
    positions:[24]km.Vec3; normals:[24]km.Vec3; uvs:[24]km.Vec2; indices:[36]u32
    corners:=[8]km.Vec3{{-1,-1,-1},{1,-1,-1},{1,1,-1},{-1,1,-1},{-1,-1,1},{1,-1,1},{1,1,1},{-1,1,1}}
    faces:=[6][4]int{{0,3,2,1},{4,5,6,7},{0,4,7,3},{1,2,6,5},{0,1,5,4},{3,7,6,2}}
    face_normals:=[6]km.Vec3{{0,0,-1},{0,0,1},{-1,0,0},{1,0,0},{0,-1,0},{0,1,0}}
    texcoords:=[4]km.Vec2{{0,0},{1,0},{1,1},{0,1}}
    for face,f in faces {
        for corner,v in face { positions[f*4+v]=corners[corner]*(size*0.5); normals[f*4+v]=face_normals[f]; uvs[f*4+v]=texcoords[v] }
        offset:=u32(f*4); local:=[6]u32{0,1,2,0,2,3}; for index,i in local { indices[f*6+i]=offset+index }
    }
    return mesh_triangles(positions[:],indices[:],normals[:],uvs[:],allocator)
}

/// Generates an XZ plane facing +Y with authored dimensions.
mesh_plane :: proc(size:km.Vec2,allocator:=context.allocator)->(Mesh_Geometry,Mesh_Error) {
    for dimension in size { if !mesh_finite(dimension) || dimension<=0 { return {},.Invalid_Geometry } }
    x,z:=size[0]*0.5,size[1]*0.5
    positions:=[4]km.Vec3{{-x,0,-z},{-x,0,z},{x,0,z},{x,0,-z}}
    indices:=[6]u32{0,1,2,0,2,3}; normals:=[4]km.Vec3{{0,1,0},{0,1,0},{0,1,0},{0,1,0}}; uvs:=[4]km.Vec2{{0,0},{0,1},{1,1},{1,0}}
    return mesh_triangles(positions[:],indices[:],normals[:],uvs[:],allocator)
}

/// Bakes exact affine positions, inverse-transpose normals and reflected winding into owned geometry.
mesh_transform :: proc(mesh:^Mesh_Geometry,transform:km.Transform)->Mesh_Error {
    if !mesh_vec_finite(transform.position) || !mesh_vec_finite(transform.scale) || !mesh_vec_finite(km.Vec4(transform.rotation)) || !km.quat_is_normalized(transform.rotation) { return .Invalid_Transform }
    for dimension in transform.scale { if dimension==0 { return .Invalid_Transform } }
    affine:=km.transform_to_mat4(transform); inverse,ok:=km.inverse(affine); if !ok { return .Invalid_Transform }
    normal_matrix:=km.transpose(inverse)
    positions:=make([]km.Vec3,len(mesh.vertices),mesh.allocator); defer delete(positions,mesh.allocator)
    for vertex,i in mesh.vertices {
        point:=km.matrix_vector(affine,km.Vec4{vertex.position[0],vertex.position[1],vertex.position[2],1})
        positions[i]=km.xyz(point); if !mesh_vec_finite(positions[i]) { return .Invalid_Transform }
    }
    reflected:=transform.scale[0]*transform.scale[1]*transform.scale[2]<0
    for &vertex,i in mesh.vertices {
        normal:=km.matrix_vector(normal_matrix,km.Vec4{vertex.normal[0],vertex.normal[1],vertex.normal[2],0})
        old_tangent:=km.matrix_vector(affine,km.Vec4{vertex.tangent[0],vertex.tangent[1],vertex.tangent[2],0})
        vertex.position=positions[i]; vertex.normal=km.normalize(km.xyz(normal))
        tangent:=km.normalize(km.xyz(old_tangent)-vertex.normal*km.dot(vertex.normal,km.xyz(old_tangent)))
        sign:=vertex.tangent[3]; if reflected { sign=-sign }; vertex.tangent={tangent[0],tangent[1],tangent[2],sign}
    }
    if reflected { for i:=0;i<len(mesh.indices);i+=3 { mesh.indices[i+1],mesh.indices[i+2]=mesh.indices[i+2],mesh.indices[i+1] } }
    low,high:=positions[0],positions[0]; for point in positions { for axis in 0..<3 { low[axis]=min(low[axis],point[axis]); high[axis]=max(high[axis],point[axis]) } }; mesh.bounds=km.aabb_from_min_max(low,high)
    return .None
}

/// Generates a UV sphere with separate seams and nondegenerate pole triangles.
mesh_sphere :: proc(radius:f32,segments,rings:int,allocator:=context.allocator)->(Mesh_Geometry,Mesh_Error) {
    if !mesh_finite(radius) || radius<=0 || segments<3 || rings<3 { return {},.Invalid_Geometry }
    if segments>MAX_MESH_VERTICES || rings>MAX_MESH_VERTICES || u64(segments+1)*u64(rings+1)>MAX_MESH_VERTICES || u64(segments)*u64(rings-1)*6>MAX_MESH_INDICES { return {},.Limit }
    count:=(segments+1)*(rings+1)
    positions:=make([]km.Vec3,count,allocator); defer delete(positions,allocator)
    normals:=make([]km.Vec3,count,allocator); defer delete(normals,allocator)
    uvs:=make([]km.Vec2,count,allocator); defer delete(uvs,allocator)
    indices:=make([dynamic]u32,0,segments*(rings-1)*6,allocator); defer delete(indices)
    for ring in 0..=rings {
        v:=f32(ring)/f32(rings); phi:=f32(math.PI)*v
        for segment in 0..=segments {
            u:=f32(segment)/f32(segments); theta:=f32(2*math.PI)*u
            index:=ring*(segments+1)+segment
            normal:=km.Vec3{math.sin(phi)*math.cos(theta),math.cos(phi),math.sin(phi)*math.sin(theta)}
            if ring==0 { normal={0,1,0} } else if ring==rings { normal={0,-1,0} }
            normals[index]=normal; positions[index]=normal*radius; uvs[index]={u,v}
        }
    }
    for ring in 0..<rings { for segment in 0..<segments {
        a:=u32(ring*(segments+1)+segment); b:=a+1; c:=a+u32(segments+1); d:=c+1
        if ring!=0 { append(&indices,a,b,c) }
        if ring!=rings-1 { append(&indices,b,d,c) }
    } }
    return mesh_triangles(positions,indices[:],normals,uvs,allocator)
}

/// Generates a capped cylinder with smooth radial sides and separate cap normals.
mesh_cylinder :: proc(radius,height:f32,segments:int,allocator:=context.allocator)->(Mesh_Geometry,Mesh_Error) {
    return mesh_round_solid(radius,height,segments,false,allocator)
}
/// Generates a capped cone with side normals following its authored slope.
mesh_cone :: proc(radius,height:f32,segments:int,allocator:=context.allocator)->(Mesh_Geometry,Mesh_Error) {
    return mesh_round_solid(radius,height,segments,true,allocator)
}
@(private="package")
mesh_round_solid :: proc(radius,height:f32,segments:int,cone:bool,allocator:mem.Allocator)->(Mesh_Geometry,Mesh_Error) {
    if !mesh_finite(radius) || !mesh_finite(height) || radius<=0 || height<=0 || segments<3 { return {},.Invalid_Geometry }
    per_segment:=12; if cone { per_segment=6 }
    if segments>MAX_MESH_VERTICES/per_segment || segments>MAX_MESH_INDICES/per_segment { return {},.Limit }
    positions:=make([dynamic]km.Vec3,0,segments*per_segment,allocator); defer delete(positions)
    normals:=make([dynamic]km.Vec3,0,segments*per_segment,allocator); defer delete(normals)
    uvs:=make([dynamic]km.Vec2,0,segments*per_segment,allocator); defer delete(uvs)
    indices:=make([dynamic]u32,0,segments*per_segment,allocator); defer delete(indices)
    for segment in 0..<segments {
        a:=f32(2*math.PI)*f32(segment)/f32(segments); b:=f32(2*math.PI)*f32(segment+1)/f32(segments)
        bottom_a:=km.Vec3{radius*math.cos(a),-height*0.5,radius*math.sin(a)}; bottom_b:=km.Vec3{radius*math.cos(b),-height*0.5,radius*math.sin(b)}
        top_a,top_b:=bottom_a,bottom_b; top_a[1]=height*0.5; top_b[1]=height*0.5
        na,nb:=km.Vec3{math.cos(a),0,math.sin(a)},km.Vec3{math.cos(b),0,math.sin(b)}
        ua,ub:=f32(segment)/f32(segments),f32(segment+1)/f32(segments)
        if cone {
            apex:=km.Vec3{0,height*0.5,0}; slope:=radius/height
            na=km.normalize(km.Vec3{na[0],slope,na[2]}); nb=km.normalize(km.Vec3{nb[0],slope,nb[2]})
            append(&positions,bottom_a,apex,bottom_b); append(&normals,na,km.normalize(na+nb),nb); append(&uvs,km.Vec2{ua,0},km.Vec2{(ua+ub)*0.5,1},km.Vec2{ub,0})
        } else {
            append(&positions,bottom_a,top_a,bottom_b,bottom_b,top_a,top_b); append(&normals,na,na,nb,nb,na,nb)
            append(&uvs,km.Vec2{ua,0},km.Vec2{ua,1},km.Vec2{ub,0},km.Vec2{ub,0},km.Vec2{ua,1},km.Vec2{ub,1})
            append(&positions,km.Vec3{0,height*0.5,0},top_b,top_a); append(&normals,km.Vec3{0,1,0},km.Vec3{0,1,0},km.Vec3{0,1,0})
            append(&uvs,km.Vec2{0.5,0.5},km.Vec2{math.cos(b)*0.5+0.5,math.sin(b)*0.5+0.5},km.Vec2{math.cos(a)*0.5+0.5,math.sin(a)*0.5+0.5})
        }
        append(&positions,km.Vec3{0,-height*0.5,0},bottom_a,bottom_b); append(&normals,km.Vec3{0,-1,0},km.Vec3{0,-1,0},km.Vec3{0,-1,0})
        append(&uvs,km.Vec2{0.5,0.5},km.Vec2{math.cos(a)*0.5+0.5,math.sin(a)*0.5+0.5},km.Vec2{math.cos(b)*0.5+0.5,math.sin(b)*0.5+0.5})
    }
    for i in 0..<len(positions) { append(&indices,u32(i)) }
    return mesh_triangles(positions[:],indices[:],normals[:],uvs[:],allocator)
}

/// Generates a UV torus around the Y axis with separate seams in both directions.
mesh_torus :: proc(radius,tube_radius:f32,segments,tube_segments:int,allocator:=context.allocator)->(Mesh_Geometry,Mesh_Error) {
    if !mesh_finite(radius) || !mesh_finite(tube_radius) || radius<=0 || tube_radius<=0 || tube_radius>=radius || segments<3 || tube_segments<3 { return {},.Invalid_Geometry }
    if segments>MAX_MESH_VERTICES || tube_segments>MAX_MESH_VERTICES || u64(segments+1)*u64(tube_segments+1)>MAX_MESH_VERTICES || u64(segments)*u64(tube_segments)*6>MAX_MESH_INDICES { return {},.Limit }
    count:=(segments+1)*(tube_segments+1)
    positions:=make([]km.Vec3,count,allocator); defer delete(positions,allocator)
    normals:=make([]km.Vec3,count,allocator); defer delete(normals,allocator)
    uvs:=make([]km.Vec2,count,allocator); defer delete(uvs,allocator)
    indices:=make([dynamic]u32,0,segments*tube_segments*6,allocator); defer delete(indices)
    for segment in 0..=segments { for tube in 0..=tube_segments {
        u,v:=f32(segment)/f32(segments),f32(tube)/f32(tube_segments); theta,phi:=f32(2*math.PI)*u,f32(2*math.PI)*v
        normal:=km.Vec3{math.cos(phi)*math.cos(theta),math.sin(phi),math.cos(phi)*math.sin(theta)}
        center:=km.Vec3{radius*math.cos(theta),0,radius*math.sin(theta)}; i:=segment*(tube_segments+1)+tube
        normals[i]=normal; positions[i]=center+normal*tube_radius; uvs[i]={u,v}
    } }
    for segment in 0..<segments { for tube in 0..<tube_segments {
        a:=u32(segment*(tube_segments+1)+tube); b:=a+u32(tube_segments+1); c:=a+1; d:=b+1
        append(&indices,a,c,b,b,c,d)
    } }
    return mesh_triangles(positions,indices[:],normals,uvs,allocator)
}
