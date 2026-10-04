//! Current katmesh recipes validate all parts and combined budgets before geometry generation.
package app

import km "../math"
import resources "../resources"
import ron "../encoding/ron"
import "core:encoding/json"
import "core:mem"
import "core:strings"

@(private="package")
Recipe_Part :: struct { geometry:json.Object,transform:km.Transform,vertices,indices:int }
@(private="package")
recipe_keys :: proc(object:json.Object,keys:[]string)->bool { for key in object { found:=false; for allowed in keys { if key==allowed { found=true; break } }; if !found { return false } }; return true }
@(private="package")
recipe_number :: proc(value:json.Value)->(f32,bool) {
    number:f64
    #partial switch n in value {
    case json.Integer: number=f64(n)
    case json.Float: number=f64(n)
    case: return 0,false
    }
    converted:=f32(number); return converted,mesh_finite(converted)
}
@(private="package")
recipe_integer :: proc(value:json.Value)->(int,bool) { number,ok:=value.(json.Integer); return int(number),ok && number>=0 && number<=MAX_MESH_INDICES }
@(private="package")
recipe_vector :: proc(value:json.Value,$N:int)->([N]f32,bool) {
    array,ok:=value.(json.Array); result:[N]f32
    if !ok || len(array)!=N { return result,false }
    for element,i in array { number,valid:=recipe_number(element); if !valid { return result,false }; result[i]=number }
    return result,true
}
@(private="package")
recipe_transform :: proc(value:json.Value)->(km.Transform,bool) {
    result:=km.TRANSFORM_IDENTITY
    if value==nil { return result,true }
    object,ok:=value.(json.Object); if !ok || !recipe_keys(object,{"position","rotation","scale"}) { return {},false }
    if v,present:=object["position"]; present { vector,valid:=recipe_vector(v,3); if !valid { return {},false }; result.position=vector }
    if v,present:=object["scale"]; present { vector,valid:=recipe_vector(v,3); if !valid { return {},false }; for scale in vector { if scale==0 { return {},false } }; result.scale=vector }
    if v,present:=object["rotation"]; present { vector,valid:=recipe_vector(v,4); if !valid || !km.quat_is_normalized(km.Quat(vector)) { return {},false }; result.rotation=km.Quat(vector) }
    return result,true
}
@(private="package")
recipe_geometry_budget :: proc(g:json.Object)->(int,int,bool) {
    kind,is_kind:=g["kind"].(string); if !is_kind { return 0,0,false }
    radius,valid_radius:=recipe_number(g["radius"]); height,valid_height:=recipe_number(g["height"])
    segments,valid_segments:=recipe_integer(g["segments"])
    switch kind {
    case "cube":
        size,valid:=recipe_vector(g["size"],3)
        return 24,36,valid && size[0]>0 && size[1]>0 && size[2]>0 && recipe_keys(g,{"kind","size"})
    case "plane":
        width,valid_width:=recipe_number(g["width"])
        return 4,6,valid_width && width>0 && valid_height && height>0 && recipe_keys(g,{"kind","width","height"})
    case "sphere":
        rings,valid_rings:=recipe_integer(g["rings"])
        if !recipe_keys(g,{"kind","radius","segments","rings"}) || !valid_radius || radius<=0 || !valid_segments || segments<3 || !valid_rings || rings<3 { return 0,0,false }
        vertices,indices:=u64(segments+1)*u64(rings+1),u64(segments)*u64(rings-1)*6
        return int(vertices),int(indices),vertices<=MAX_MESH_VERTICES && indices<=MAX_MESH_INDICES
    case "cylinder","cone":
        if !recipe_keys(g,{"kind","radius","height","segments"}) || !valid_radius || radius<=0 || !valid_height || height<=0 || !valid_segments || segments<3 { return 0,0,false }
        count:=segments*12; if kind=="cone" { count=segments*6 }
        return count,count,count<=MAX_MESH_VERTICES && count<=MAX_MESH_INDICES
    case "torus":
        tube_radius,valid_tube_radius:=recipe_number(g["tube_radius"]); tube_segments,valid_tube_segments:=recipe_integer(g["tube_segments"])
        if !recipe_keys(g,{"kind","radius","tube_radius","segments","tube_segments"}) || !valid_radius || radius<=0 || !valid_tube_radius || tube_radius<=0 || tube_radius>=radius || !valid_segments || segments<3 || !valid_tube_segments || tube_segments<3 { return 0,0,false }
        vertices,indices:=u64(segments+1)*u64(tube_segments+1),u64(segments)*u64(tube_segments)*6
        return int(vertices),int(indices),vertices<=MAX_MESH_VERTICES && indices<=MAX_MESH_INDICES
    case "triangles":
        if !recipe_keys(g,{"kind","positions","indices","normals","uvs"}) { return 0,0,false }
        positions,is_positions:=g["positions"].(json.Array); indices,is_indices:=g["indices"].(json.Array)
        if !is_positions || !is_indices || len(positions)<3 || len(positions)>MAX_MESH_VERTICES || len(indices)==0 || len(indices)>MAX_MESH_INDICES || len(indices)%3!=0 { return 0,0,false }
        for position in positions { if _,valid:=recipe_vector(position,3); !valid { return 0,0,false } }
        for value in indices { if index,valid:=recipe_integer(value); !valid || index>=len(positions) { return 0,0,false } }
        for name in ([2]string{"normals","uvs"}) {
            if value,present:=g[name]; present {
                if _,is_null:=value.(json.Null); is_null { continue }
                array,is_array:=value.(json.Array); if !is_array || len(array)!=len(positions) { return 0,0,false }
                for element in array {
                    if name=="normals" { normal,valid:=recipe_vector(element,3); if !valid || km.length_squared(normal)<0.000000000001 { return 0,0,false } }
                    else { if _,valid:=recipe_vector(element,2); !valid { return 0,0,false } }
                }
            }
        }
        return len(positions),len(indices),true
    }
    return 0,0,false
}
@(private="package")
recipe_geometry_compile :: proc(g:json.Object,allocator:mem.Allocator)->(Mesh_Geometry,Mesh_Error) {
    kind:=g["kind"].(string)
    radius,_:=recipe_number(g["radius"]); height,_:=recipe_number(g["height"]); segments,_:=recipe_integer(g["segments"])
    switch kind {
    case "cube": size,_:=recipe_vector(g["size"],3); return mesh_cube(size,allocator)
    case "plane": width,_:=recipe_number(g["width"]); return mesh_plane({width,height},allocator)
    case "sphere": rings,_:=recipe_integer(g["rings"]); return mesh_sphere(radius,segments,rings,allocator)
    case "cylinder": return mesh_cylinder(radius,height,segments,allocator)
    case "cone": return mesh_cone(radius,height,segments,allocator)
    case "torus": tube_radius,_:=recipe_number(g["tube_radius"]); tube_segments,_:=recipe_integer(g["tube_segments"]); return mesh_torus(radius,tube_radius,segments,tube_segments,allocator)
    case "triangles":
        position_values:=g["positions"].(json.Array); index_values:=g["indices"].(json.Array)
        positions:=make([]km.Vec3,len(position_values),allocator); defer delete(positions,allocator)
        indices:=make([]u32,len(index_values),allocator); defer delete(indices,allocator)
        normals:[]km.Vec3; uvs:[]km.Vec2; defer delete(normals,allocator); defer delete(uvs,allocator)
        for value,i in position_values { positions[i],_=recipe_vector(value,3) }
        for value,i in index_values { index,_:=recipe_integer(value); indices[i]=u32(index) }
        if values,provided:=g["normals"].(json.Array); provided { normals=make([]km.Vec3,len(values),allocator); for value,i in values { normals[i],_=recipe_vector(value,3) } }
        if values,provided:=g["uvs"].(json.Array); provided { uvs=make([]km.Vec2,len(values),allocator); for value,i in values { uvs[i],_=recipe_vector(value,2) } }
        return mesh_triangles(positions,indices,normals,uvs,allocator)
    }
    return {},.Invalid_Geometry
}

/// Compiles the current JSON/RON value schema into one checked geometry stream.
mesh_recipe_compile :: proc(document:json.Value,allocator:=context.allocator)->(Mesh_Geometry,Mesh_Error) {
    context.allocator=allocator
    object,ok:=document.(json.Object); if !ok || !recipe_keys(object,{"version","name","parts"}) { return {},.Invalid_Geometry }
    version,is_version:=object["version"].(json.Integer); name,is_name:=object["name"].(string); parts,is_parts:=object["parts"].(json.Array)
    if !is_version || version!=1 || !is_name || len(strings.trim_space(name))==0 || len(name)>256 || !is_parts || len(parts)==0 || len(parts)>1024 { return {},.Invalid_Geometry }
    recipes:=make([]Recipe_Part,len(parts),allocator); defer delete(recipes,allocator)
    ids:=make(map[string]bool,allocator); defer delete(ids)
    total_vertices,total_indices:=0,0
    for value,i in parts {
        part,is_part:=value.(json.Object); if !is_part || !recipe_keys(part,{"id","transform","geometry"}) { return {},.Invalid_Geometry }
        id,is_id:=part["id"].(string); if !is_id || len(strings.trim_space(id))==0 || len(id)>128 || ids[id] { return {},.Invalid_Geometry }; ids[id]=true
        transform,valid_transform:=recipe_transform(part["transform"]); geometry,is_geometry:=part["geometry"].(json.Object)
        if !valid_transform || !is_geometry { return {},.Invalid_Geometry }
        vertices,indices,valid_geometry:=recipe_geometry_budget(geometry); if !valid_geometry { return {},.Invalid_Geometry }
        total_vertices+=vertices; total_indices+=indices
        if total_vertices>MAX_MESH_VERTICES || total_indices>MAX_MESH_INDICES { return {},.Limit }
        recipes[i]={geometry,transform,vertices,indices}
    }
    result:=Mesh_Geometry{vertices=make([]Mesh_Vertex,total_vertices,allocator),indices=make([]u32,total_indices,allocator),allocator=allocator}
    success:=false; defer { if !success { mesh_geometry_destroy(&result) } }
    vertex_offset,index_offset:=0,0
    for recipe in recipes {
        mesh,err:=recipe_geometry_compile(recipe.geometry,allocator); if err!=.None { return {},err }; defer mesh_geometry_destroy(&mesh)
        if err=mesh_transform(&mesh,recipe.transform); err!=.None { return {},err }
        copy(result.vertices[vertex_offset:],mesh.vertices)
        for index,i in mesh.indices { result.indices[index_offset+i]=index+u32(vertex_offset) }
        vertex_offset+=len(mesh.vertices); index_offset+=len(mesh.indices)
    }
    low,high:=result.vertices[0].position,result.vertices[0].position
    for vertex in result.vertices { for axis in 0..<3 { low[axis]=min(low[axis],vertex.position[axis]); high[axis]=max(high[axis],vertex.position[axis]) } }; result.bounds=km.aabb_from_min_max(low,high)
    success=true; return result,.None
}

/// Reads and compiles a real resource-relative katmesh file through the confined root.
mesh_recipe_load :: proc(root:^resources.Root,path:string)->(Mesh_Geometry,Mesh_Error) {
    data,read_error:=resources.read_text(root,path); if read_error!=.None { return {},.Invalid_Geometry }; defer delete(data,root.allocator)
    tree,parse_error:=ron.parse(string(data),root.allocator); if parse_error.kind!=.None { return {},.Invalid_Geometry }; defer json.destroy_value(tree)
    return mesh_recipe_compile(tree,root.allocator)
}
