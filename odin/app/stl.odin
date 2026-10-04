//! ASCII and binary STL bytes compile into the canonical owned mesh stream before scene admission.
package app
import km "../math"
import "core:strings"
import "core:strconv"
import "core:unicode/utf8"
import "core:math"

STL_Triangle :: struct { normal:km.Vec3,vertices:[3]km.Vec3 }
@(private="package")
stl_u32 :: proc(bytes:[]byte)->u32 { return u32(bytes[0])|u32(bytes[1])<<8|u32(bytes[2])<<16|u32(bytes[3])<<24 }
@(private="package")
stl_f32 :: proc(bytes:[]byte)->f32 { return transmute(f32)stl_u32(bytes) }
@(private="package")
stl_vector :: proc(text:string)->(km.Vec3,bool) {
    result:km.Vec3; remaining:=text; count:=0
    for field in strings.fields_iterator(&remaining) {
        if count==3 { return {},false }; number,valid:=strconv.parse_f32(field); if !valid { return {},false }; result[count]=number; count+=1
    }
    return result,count==3
}
@(private="package")
stl_triangles :: proc(bytes:[]byte,triangles:^[dynamic]STL_Triangle)->Mesh_Error {
    binary:=len(bytes)>=84 && u64(stl_u32(bytes[80:84]))<=u64((len(bytes)-84)/50) && 84+u64(stl_u32(bytes[80:84]))*50==u64(len(bytes))
    prefix:=string(bytes[:min(len(bytes),5)])
    if !binary && (prefix=="solid" || prefix=="SOLID") {
        if !utf8.valid_string(string(bytes)) { return .Invalid_Geometry }
        triangle:STL_Triangle; in_loop:=false; count:=0; remaining:=string(bytes)
        for raw_line in strings.split_iterator(&remaining,"\n") {
            line:=strings.to_lower(strings.trim_space(raw_line)); defer delete(line)
            if line=="" || strings.has_prefix(line,"solid") || strings.has_prefix(line,"endsolid") { continue }
            if strings.has_prefix(line,"facet normal") {
                normal,valid:=stl_vector(line[len("facet normal"):]); if !valid { return .Invalid_Geometry }
                triangle.normal=normal; in_loop=false; count=0; continue
            }
            if line=="outer loop" { in_loop=true; count=0; continue }
            if line=="endloop" { in_loop=false; continue }
            if line=="endfacet" {
                if count!=3 { return .Invalid_Geometry }; if len(triangles^)>=MAX_MESH_INDICES/3 { return .Limit }; append(triangles,triangle); in_loop=false; continue
            }
            if strings.has_prefix(line,"vertex") {
                if !in_loop || count>=3 { return .Invalid_Geometry }; vertex,valid:=stl_vector(line[len("vertex"):]); if !valid { return .Invalid_Geometry }; triangle.vertices[count]=vertex; count+=1
            }
        }
        return .None
    }
    if len(bytes)<84 { return .Invalid_Geometry }
    count:=u64(stl_u32(bytes[80:84])); if count>u64(MAX_MESH_INDICES/3) { return .Limit }
    if count>u64((len(bytes)-84)/50) { return .Invalid_Geometry }
    for i in 0..<int(count) {
        record:=bytes[84+i*50:84+(i+1)*50]; triangle:STL_Triangle
        for axis in 0..<3 { triangle.normal[axis]=stl_f32(record[axis*4:axis*4+4]) }
        for vertex in 0..<3 { for axis in 0..<3 { offset:=12+vertex*12+axis*4; triangle.vertices[vertex][axis]=stl_f32(record[offset:offset+4]) } }
        append(triangles,triangle)
    }
    return .None
}

/// Drops nonfinite STL triangles as Rust does, deduplicates position/normal pairs and generates tangents.
stl_decode :: proc(bytes:[]byte,allocator:=context.allocator)->(Mesh_Geometry,Mesh_Error) {
    context.allocator=allocator
    if len(bytes)>64*1024*1024 { return {},.Limit }
    triangles:=make([dynamic]STL_Triangle,allocator); defer delete(triangles)
    if error:=stl_triangles(bytes,&triangles); error!=.None { return {},error }
    positions:=make([dynamic]km.Vec3,allocator); normals:=make([dynamic]km.Vec3,allocator); indices:=make([dynamic]u32,allocator)
    defer { delete(positions); delete(normals); delete(indices) }
    mapping:=make(map[[6]u32]u32,allocator); defer delete(mapping)
    for triangle in triangles {
        finite:=true
        for number in triangle.normal { if math.is_nan(number)||math.is_inf(number) { finite=false } }
        for vertex in triangle.vertices { for number in vertex { if math.is_nan(number)||math.is_inf(number) { finite=false } } }
        if !finite { continue }
        normal:=triangle.normal
        if km.length_squared(normal)==0 { normal=km.cross(triangle.vertices[1]-triangle.vertices[0],triangle.vertices[2]-triangle.vertices[0]) }
        if km.length_squared(normal)==0 { return {},.Invalid_Geometry }
        normal=km.normalize(normal)
        for vertex in triangle.vertices {
            key:[6]u32; for axis in 0..<3 { key[axis]=transmute(u32)vertex[axis]; key[axis+3]=transmute(u32)normal[axis] }
            index,found:=mapping[key]
            if !found {
                if len(positions)>=MAX_MESH_VERTICES { return {},.Limit }
                index=u32(len(positions)); mapping[key]=index; append(&positions,vertex); append(&normals,normal)
            }
            append(&indices,index)
        }
    }
    return mesh_triangles(positions[:],indices[:],normals[:],nil,allocator)
}
