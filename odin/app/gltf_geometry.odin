//! Checked accessor extraction preserves indexed geometry, skin attributes and morph deltas.
package app

import km "../math"
import cgltf "../deps/cgltf"
import "core:math"

@(private="package")
gltf_floats :: proc(accessor:^cgltf.accessor,components:uint,max_count:=MAX_MESH_VERTICES)->([]f32,Gltf_Error) {
    if accessor==nil || accessor.count==0 || accessor.count>uint(max_count) || cgltf.num_components(accessor.type)!=components { return nil,.Invalid_Accessor }
    if accessor.buffer_view!=nil && accessor.buffer_view.has_meshopt_compression { return nil,.Unsupported }
    count:=accessor.count*components
    values:=make([]f32,int(count))
    if cgltf.accessor_unpack_floats(accessor,raw_data(values),count)!=count { delete(values); return nil,.Invalid_Accessor }
    for value in values { if math.is_nan(value) || math.is_inf(value) { delete(values); return nil,.Invalid_Accessor } }
    return values,.None
}
@(private="package")
gltf_attribute :: proc(attributes:[]cgltf.attribute,kind:cgltf.attribute_type,index:=0)->^cgltf.accessor {
    for attribute in attributes { if attribute.type==kind && int(attribute.index)==index { return attribute.data } }
    return nil
}
@(private="package")
gltf_vec3 :: proc(accessor:^cgltf.accessor,count:int)->([]km.Vec3,Gltf_Error) {
    if accessor==nil { return nil,.None }
    values,error:=gltf_floats(accessor,3); if error!=.None { return nil,error }; defer delete(values)
    if len(values)!=count*3 { return nil,.Invalid_Accessor }
    result:=make([]km.Vec3,count)
    for &vector,i in result { vector={values[i*3],values[i*3+1],values[i*3+2]} }
    return result,.None
}
/// Applies the authored texture transform to a selected texture-coordinate set.
gltf_texture_uv :: proc(view:Gltf_Texture_View,uv:km.Vec2)->km.Vec2 {
    scaled:=uv*view.uv_scale
    sine,cosine:=math.sin(view.rotation),math.cos(view.rotation)
    return view.offset+km.Vec2{cosine*scaled[0]-sine*scaled[1],sine*scaled[0]+cosine*scaled[1]}
}
@(private="package")
gltf_primitive :: proc(data:^cgltf.data,source:^cgltf.primitive,target:^Gltf_Primitive)->Gltf_Error {
    if source.has_draco_mesh_compression { return .Unsupported }
    positions_accessor:=gltf_attribute(source.attributes,.position)
    if positions_accessor==nil { return .Invalid_Geometry }
    count:=int(positions_accessor.count)
    positions,position_error:=gltf_vec3(positions_accessor,count); if position_error!=.None { return position_error }; defer delete(positions)
    normals,normal_error:=gltf_vec3(gltf_attribute(source.attributes,.normal),count); if normal_error!=.None { return normal_error }; defer delete(normals)
    uv_count:=0
    for attribute in source.attributes {
        if attribute.index<0 { return .Invalid_Accessor }
        if attribute.type==.texcoord { if attribute.index>=32 { return .Limit }; uv_count=max(uv_count,int(attribute.index)+1) }
        if (attribute.type==.joints || attribute.type==.weights || attribute.type==.color) && attribute.index!=0 { return .Unsupported }
    }
    target.uv_sets=make([][]km.Vec2,uv_count)
    for &uvs,index in target.uv_sets {
        accessor:=gltf_attribute(source.attributes,.texcoord,index); if accessor==nil { continue }
        values,error:=gltf_floats(accessor,2); if error!=.None { return error }; defer delete(values)
        if len(values)!=count*2 { return .Invalid_Accessor }
        uvs=make([]km.Vec2,count); for &uv,i in uvs { uv={values[i*2],values[i*2+1]} }
    }
    indices:=make([dynamic]u32); defer delete(indices)
    if source.indices!=nil {
        if source.indices.type!=.scalar || source.indices.normalized || source.indices.component_type not_in (bit_set[cgltf.component_type]{.r_8u,.r_16u,.r_32u}) { return .Invalid_Accessor }
        values,error:=gltf_floats(source.indices,1,MAX_MESH_INDICES); if error!=.None { return error }; defer delete(values)
        for index in values { if index<0 || index>=f32(count) || math.floor(index)!=index { return .Invalid_Accessor }; append(&indices,u32(index)) }
    } else { for index in 0..<count { append(&indices,u32(index)) } }
    if source.type!=.triangles {
        if source.type!=.triangle_strip && source.type!=.triangle_fan { return .Unsupported }
        converted:=make([dynamic]u32); defer delete(converted)
        if len(indices)<3 || len(indices)>MAX_MESH_INDICES/3+2 { return .Invalid_Geometry }
        for i in 2..<len(indices) {
            a,b,c:=indices[i-2],indices[i-1],indices[i]
            if source.type==.triangle_fan { a=indices[0] } else if i%2==1 { a,b=b,a }
            if a==b || b==c || c==a { continue }
            append(&converted,a,b,c)
        }
        resize(&indices,len(converted)); copy(indices[:],converted[:])
    }
    uvs:[]km.Vec2; if len(target.uv_sets)>0 { uvs=target.uv_sets[0] }
    tangent_uvs:=uvs
    transformed_uvs:[]km.Vec2; defer delete(transformed_uvs)
    if source.material!=nil && source.material.normal_texture.texture!=nil && gltf_attribute(source.attributes,.tangent)==nil {
        view,view_error:=gltf_texture_view(data,source.material.normal_texture); if view_error!=.None { return view_error }
        if view.texcoord<0 || int(view.texcoord)>=len(target.uv_sets) || len(target.uv_sets[view.texcoord])!=count { return .Invalid_Accessor }
        transformed_uvs=make([]km.Vec2,count)
        for uv,i in target.uv_sets[view.texcoord] { transformed_uvs[i]=gltf_texture_uv(view,uv) }
        tangent_uvs=transformed_uvs
    }
    geometry,geometry_error:=mesh_triangles(positions,indices[:],normals,tangent_uvs)
    if geometry_error!=.None { return .Limit if geometry_error==.Limit else .Invalid_Geometry }; target.geometry=geometry
    for &vertex,i in target.geometry.vertices { vertex.uv={}; if len(uvs)>0 { vertex.uv=uvs[i] } }
    if source.material!=nil { target.material=i32(cgltf.material_index(data,source.material)) }
    if accessor:=gltf_attribute(source.attributes,.tangent); accessor!=nil {
        values,error:=gltf_floats(accessor,4); if error!=.None { return error }; defer delete(values)
        if len(values)!=count*4 { return .Invalid_Accessor }
        for &vertex,i in target.geometry.vertices {
            tangent:=km.Vec4{values[i*4],values[i*4+1],values[i*4+2],values[i*4+3]}
            if abs(km.length_squared(km.xyz(tangent))-1)>0.01 || abs(abs(tangent[3])-1)>0.001 || abs(km.dot(vertex.normal,km.xyz(tangent)))>0.01 { return .Invalid_Accessor }
            vertex.tangent=tangent
        }
    }
    if accessor:=gltf_attribute(source.attributes,.color); accessor!=nil {
        components:=cgltf.num_components(accessor.type); if components!=3 && components!=4 { return .Invalid_Accessor }
        values,error:=gltf_floats(accessor,components); if error!=.None { return error }; defer delete(values)
        if len(values)!=count*int(components) { return .Invalid_Accessor }
        target.colors=make([]km.Vec4,count)
        for &color,i in target.colors { offset:=i*int(components); color={values[offset],values[offset+1],values[offset+2],1}; if components==4 { color[3]=values[offset+3] }; for value in color { if value<0 || value>1 { return .Invalid_Accessor } } }
    }
    joints,weights:=gltf_attribute(source.attributes,.joints),gltf_attribute(source.attributes,.weights)
    if (joints==nil)!=(weights==nil) { return .Invalid_Skin }
    if joints!=nil {
        if joints.normalized || joints.component_type not_in (bit_set[cgltf.component_type]{.r_8u,.r_16u}) { return .Invalid_Skin }
        joint_values,joint_error:=gltf_floats(joints,4); if joint_error!=.None { return joint_error }; defer delete(joint_values)
        weight_values,weight_error:=gltf_floats(weights,4); if weight_error!=.None { return weight_error }; defer delete(weight_values)
        if len(joint_values)!=count*4 || len(weight_values)!=count*4 { return .Invalid_Skin }
        target.joints=make([][4]u16,count); target.weights=make([][4]f32,count)
        for &joint,i in target.joints {
            total:f32
            for component in 0..<4 {
                index,weight:=joint_values[i*4+component],weight_values[i*4+component]
                if index<0 || index>65535 || math.floor(index)!=index || weight<0 { return .Invalid_Skin }
                joint[component]=u16(index); target.weights[i][component]=weight; total+=weight
            }
            if total<=0 { return .Invalid_Skin }; target.weights[i]/=total
        }
    }
    target.morphs=make([]Gltf_Morph,len(source.targets))
    for morph,i in source.targets {
        output:=&target.morphs[i]
        position,error:=gltf_vec3(gltf_attribute(morph.attributes,.position),count); if error!=.None { return error }; output.position=position
        normal,morph_normal_error:=gltf_vec3(gltf_attribute(morph.attributes,.normal),count); if morph_normal_error!=.None { return morph_normal_error }; output.normal=normal
        tangent,tangent_error:=gltf_vec3(gltf_attribute(morph.attributes,.tangent),count); if tangent_error!=.None { return tangent_error }; output.tangent=tangent
    }
    return .None
}
@(private="package")
gltf_extract_geometry :: proc(data:^cgltf.data,model:^Gltf_Model,budget:^int)->Gltf_Error {
    count:=0
    for mesh in data.meshes { if len(mesh.primitives)>MAX_GLTF_PRIMITIVES-count { return .Limit }; count+=len(mesh.primitives) }
    model.primitives=make([]Gltf_Primitive,count)
    vertex_budget,index_budget:=MAX_MESH_VERTICES,MAX_MESH_INDICES
    index:=0
    for &mesh,mesh_index in data.meshes {
        for &source in mesh.primitives {
            target:=&model.primitives[index]; target.mesh=u32(mesh_index); target.material= -1; index+=1
            vertices:=gltf_attribute(source.attributes,.position)
            if vertices==nil { return .Invalid_Geometry }
            if len(source.targets)>4096 { return .Limit }
            cost:=u64(vertices.count)*(u64(size_of(Mesh_Vertex))+u64(len(source.attributes))*16+u64(len(source.targets))*36)
            if source.indices!=nil { cost+=u64(source.indices.count)*12 } else { cost+=u64(vertices.count)*12 }
            if cost>u64(budget^) { return .Limit }; budget^-=int(cost)
            if error:=gltf_primitive(data,&source,target); error!=.None { return error }
            vertex_budget-=len(target.geometry.vertices); index_budget-=len(target.geometry.indices)
            if vertex_budget<0 || index_budget<0 { return .Limit }
        }
    }
    return .None
}
