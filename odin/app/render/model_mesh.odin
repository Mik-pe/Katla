//! Authored primitive meshes share the source-model material stream and native consumer.
package render

import app ".."
import ecs "../../ecs"
import km "../../math"
import "core:mem"

@(private="package")
model_mesh_prepare :: proc(owner:^app.Authoring,id:ecs.Entity_Id,mesh:^app.Scene_Mesh,allocator:mem.Allocator)->(Model_Entry,Model_Object,[]Model_Vertex,Model_Batch_Error) {
    geometry:=&mesh.geometry
    if len(geometry.vertices)==0 || len(geometry.indices)==0 || len(geometry.indices)%3!=0 { return {},{},nil,{kind=.Invalid_Geometry} }
    surface,present:=ecs.get_component(&owner.world,id,app.Surface_Material)
    if !present { return {},{},nil,{kind=.Invalid_Scene,scene=.Component_Not_Found} }
    if (surface.has_surface && !app.material_surface_valid(surface.surface)) || (surface.has_sampling && !app.material_sampling_valid(surface.sampling)) { return {},{},nil,{kind=.Invalid_Material} }
    model,world_error:=app.scene_world_matrix(owner,id);if world_error!=.None { return {},{},nil,{kind=.Invalid_Scene,scene=world_error} }
    default_model:app.Gltf_Model
    material,_:=model_material(&default_model,-1);material=model_material_surface(material,surface)
    views,samplers:=model_material_sampling(material,surface)
    object,object_error:=model_object(model,surface,material);if object_error.kind!=.None { return {},{},nil,object_error }
    entry:=Model_Entry{entity=id,primitive_mesh=true,material=material,views=views,samplers=samplers,has_sampling=surface.has_sampling,source_vertices=raw_data(geometry.vertices),source_indices=raw_data(geometry.indices)}
    image_error:=model_entry_images(owner,&entry,&object);if image_error.kind!=.None { return {},{},nil,image_error }
    if surface.has_sampling && surface.sampling.normal.uv!=app.uv_transform_default() { object.flags[3]|=2 }
    stream:=make([]Model_Vertex,len(geometry.indices),allocator)
    accepted:=false;defer { if !accepted { delete(stream,allocator) } }
    low,high:km.Vec3
    for index,i in geometry.indices {
        if u64(index)>=u64(len(geometry.vertices)) { return {},{},nil,{kind=.Invalid_Geometry} }
        source:=geometry.vertices[index]
        vertex:=Model_Vertex{position=km.vec4(source.position,1),normal=km.vec4(source.normal),tangent=source.tangent,color={1,1,1,1}}
        for view,role in views {
            required:=entry.material_sources[role]==.File || entry.material_sources[role]==.GltfImage
            if view.texcoord!=0 && required { return {},{},nil,{kind=.Invalid_Material} }
            uv:=app.gltf_texture_uv(view,source.uv)
            vertex.uvs[role]={uv[0],uv[1],0,0}
        }
        vertex.uvs[3][2]=material.occlusion_texture.scale
        stream[i]=vertex
        if i==0 { low=source.position;high=source.position } else { for axis in 0..<3 { low[axis]=min(low[axis],source.position[axis]);high[axis]=max(high[axis],source.position[axis]) } }
    }
    entry.local_bounds=km.aabb_from_min_max(low,high)
    entry.world_center=km.xyz(km.matrix_vector(model,km.vec4(entry.local_bounds.center,1)))
    accepted=true;return entry,object,stream,{}
}
