//! Actual glTF scene primitives become owned shader-compatible CPU streams.
package render

import app ".."
import ecs "../../ecs"
import editor "../../editor"
import gfx "../../gfx"
import km "../../math"
import "core:mem"
import "core:slice"

/// Five UV roles preserve the imported texture-coordinate selection and transform.
Model_Vertex :: struct { position,normal,tangent,color:km.Vec4, uvs:[5]km.Vec4 }
/// Authored PBR factors share the canonical model WGSL storage ABI.
Model_Object :: struct { model,normal_model:km.Mat4, base_color,factors,emissive,specular_glossiness:km.Vec4, flags:[4]u32 }
/// Stable source identities require native preparation when topology or texture ownership changes.
Model_Entry :: struct {
    entity:ecs.Entity_Id,
    node,primitive,first_vertex,vertex_count,object_index:u32,
    local_bounds:km.AABB,
    world_center:km.Vec3,
    camera_depth:f32,
    material:app.Gltf_Material,
    views:[5]app.Gltf_Texture_View,
    samplers:[5]gfx.Sampler_Desc,
    has_sampling,primitive_mesh:bool,
    material_sources:[5]app.Texture_Source_Kind,
    material_digests:[5][32]byte,
    source_nodes,source_primitives,source_vertices,source_indices,source_images,source_textures,source_samplers:rawptr,
}
Model_Batch_Error_Kind :: enum { None, Rebuild_Required, Invalid_Scene, Invalid_Geometry, Invalid_Material, Limit }
Model_Batch_Error :: struct { kind:Model_Batch_Error_Kind, scene:editor.Scene_Error, gltf:app.Gltf_Error }
/// Owns expanded triangle vertices and objects; no resource owner survives through borrowed model pointers.
Model_Batch :: struct {
    vertices:[]Model_Vertex,
    objects:[]Model_Object,
    entries:[]Model_Entry,
    ids:[]ecs.Entity_Id,
    allocator:mem.Allocator,
    streaming,geometry_changed,all_entities:bool,
    geometry_revision:u64,
}
/// Prepares all visible model components in the authored world.
model_batch_prepare :: proc(owner:^app.Authoring,allocator:=context.allocator)->(Model_Batch,Model_Batch_Error) {
    ids:=ecs.entity_ids(&owner.world); defer delete(ids)
    result,error:=model_batch_prepare_entities(owner,ids[:],allocator)
    if error.kind==.None { result.all_entities=true }
    return result,error
}
/// Prepares explicit staged entities before publication, including active roots and their descendants.
model_batch_prepare_entities :: proc(owner:^app.Authoring,ids:[]ecs.Entity_Id,allocator:=context.allocator)->(Model_Batch,Model_Batch_Error) {
    result:=Model_Batch{allocator=allocator,geometry_revision=1,geometry_changed=true}
    success:=false; defer { if !success { model_batch_destroy(&result) } }
    context.allocator=allocator
    vertices:=make([dynamic]Model_Vertex,allocator); defer delete(vertices)
    objects:=make([dynamic]Model_Object,allocator); defer delete(objects)
    entries:=make([dynamic]Model_Entry,allocator); defer delete(entries)
    for id,i in ids {
        if !ecs.entity_exists(&owner.world,id) { return {},{kind=.Invalid_Scene,scene=.Entity_Not_Found} }
        for previous in ids[:i] { if previous==id { return {},{kind=.Invalid_Scene,scene=.Invalid_Operation} } }
        if _,hidden:=ecs.get_component(&owner.world,id,app.Editor_Hidden); hidden { continue }
        component:=ecs.get_component_mut(&owner.world,id,app.Scene_Model)
        if component==nil {
            mesh,present:=ecs.get_component(&owner.world,id,app.Scene_Mesh);if !present || mesh.source.kind==.Empty { continue }
            entry,object,stream,error:=model_mesh_prepare(owner,id,&mesh,allocator)
            if error.kind!=.None { return {},error };defer delete(stream,allocator)
            if u64(len(vertices))+u64(len(stream))>u64(max(u32)) || u64(len(entries))>=u64(max(u32)) { return {},{kind=.Limit} }
            entry.first_vertex=u32(len(vertices));entry.object_index=u32(len(objects));entry.vertex_count=u32(len(stream))
            append(&vertices,..stream);append(&objects,object);append(&entries,entry);continue
        }
        if component.source.kind==.Group { continue }
        selected_primitive:^app.Gltf_Primitive
        if component.source.kind==.Primitive { selected,valid:=app.scene_model_selected_primitive(component);if !valid { return {},{kind=.Invalid_Geometry} };selected_primitive=selected }
        model:=&component.model
        player:=app.scene_model_animation_player(owner,id)
        result.streaming=result.streaming || len(model.animation.clips)>0
        world,world_error:=app.gltf_world_matrices(model,player,allocator); if world_error!=.None { return {},{kind=.Invalid_Scene,scene=world_error} }; defer delete(world,allocator)
        active,active_error:=app.gltf_active_nodes(model,allocator); if active_error!=.None { return {},{kind=.Invalid_Scene,scene=active_error} }; defer delete(active,allocator)
        entity_world,entity_error:=app.scene_world_matrix(owner,id); if entity_error!=.None { return {},{kind=.Invalid_Scene,scene=entity_error} }
        surface,surface_present:=ecs.get_component(&owner.world,id,app.Surface_Material); if !surface_present { surface={metallic=1,roughness=1,ao=1} }
        if (surface.has_surface && !app.material_surface_valid(surface.surface)) || (surface.has_sampling && !app.material_sampling_valid(surface.sampling)) { return {},{kind=.Invalid_Material} }
        for node,n in model.nodes {
            if !active[n] || node.mesh<0 || (selected_primitive!=nil && u32(n)!=component.source.node_index) { continue }
            weights,weight_error:=app.animation_sample_weights(&model.animation,player,u32(n),node.weights,allocator)
            if weight_error!=.None { return {},{kind=.Invalid_Scene,scene=weight_error} }; defer delete(weights,allocator)
            for primitive,p in model.primitives {
                if primitive.mesh!=u32(node.mesh) || (selected_primitive!=nil && selected_primitive!=&model.primitives[p]) { continue }
                if u64(len(vertices))+u64(len(primitive.geometry.indices))>u64(max(u32)) || u64(len(entries))>=u64(max(u32)) { return {},{kind=.Limit} }
                material,material_error:=model_material(model,primitive.material); if material_error.kind!=.None { return {},material_error }
                material=model_material_surface(material,surface)
                views,samplers:=model_material_sampling(material,surface)
                object,object_error:=model_object(km.matrix_mul(entity_world,world[n]),surface,material)
                if object_error.kind!=.None { return {},object_error }
                if primitive.tangent_generated && !model_uv_equal(views[1],primitive.tangent_uv) { object.flags[3]|=2 }
                geometry,geometry_error:=app.gltf_deform_geometry(model,u32(p),u32(n),world,weights,allocator)
                if geometry_error!=.None { return {},{kind=.Invalid_Geometry,gltf=geometry_error} }
                entry:=Model_Entry{entity=id,node=u32(n),primitive=u32(p),first_vertex=u32(len(vertices)),vertex_count=u32(len(geometry.indices)),object_index=u32(len(objects)),material=material,views=views,samplers=samplers,has_sampling=surface.has_sampling,source_nodes=raw_data(model.nodes),source_primitives=raw_data(model.primitives),source_vertices=raw_data(primitive.geometry.vertices),source_indices=raw_data(primitive.geometry.indices),source_images=raw_data(model.images),source_textures=raw_data(model.textures),source_samplers=raw_data(model.samplers)}
                entry.material.name=""
                source_error:=model_entry_images(owner,&entry,&object);if source_error.kind!=.None { app.mesh_geometry_destroy(&geometry);return {},source_error }
                if len(geometry.indices)==0 { app.mesh_geometry_destroy(&geometry); return {},{kind=.Invalid_Geometry} }
                low,high:km.Vec3
                for index,stream_index in geometry.indices {
                    if int(index)>=len(geometry.vertices) { app.mesh_geometry_destroy(&geometry); return {},{kind=.Invalid_Geometry} }
                    source:=geometry.vertices[index]
                    if stream_index==0 { low=source.position; high=source.position }
                    else { for axis in 0..<3 { low[axis]=min(low[axis],source.position[axis]); high[axis]=max(high[axis],source.position[axis]) } }
                    vertex:=Model_Vertex{position=km.vec4(source.position,1),normal=km.vec4(source.normal,0),tangent=source.tangent,color={1,1,1,1}}
                    if len(primitive.colors)>0 { if int(index)>=len(primitive.colors) { app.mesh_geometry_destroy(&geometry); return {},{kind=.Invalid_Geometry} }; vertex.color=primitive.colors[index] }
                    for view,v in views {
                        if entry.material_sources[v]==.Neutral { continue }
                        uv,uv_error:=model_vertex_uv(primitive,index,view,entry.material_sources[v]==.File || entry.material_sources[v]==.GltfImage || view.texture>=0)
                        if uv_error.kind!=.None { app.mesh_geometry_destroy(&geometry); return {},uv_error }
                        vertex.uvs[v]={uv[0],uv[1],0,0}
                    }
                    vertex.uvs[3][2]=material.occlusion_texture.scale
                    append(&vertices,vertex)
                }
                entry.local_bounds=km.aabb_from_min_max(low,high)
                entry.world_center=km.xyz(km.matrix_vector(object.model,km.vec4(entry.local_bounds.center,1)))
                app.mesh_geometry_destroy(&geometry)
                append(&entries,entry); append(&objects,object)
            }
        }
    }
    result.vertices=slice.clone(vertices[:],allocator); result.objects=slice.clone(objects[:],allocator); result.entries=slice.clone(entries[:],allocator); result.ids=slice.clone(ids,allocator)
    success=true; return result,{}
}
/// Refreshes actual sampled geometry and factors atomically; immutable native identities require rebuilding.
model_batch_refresh :: proc(batch:^Model_Batch,owner:^app.Authoring)->Model_Batch_Error {
    candidate:Model_Batch; error:Model_Batch_Error
    if batch.all_entities { candidate,error=model_batch_prepare(owner,batch.allocator) }
    else { candidate,error=model_batch_prepare_entities(owner,batch.ids,batch.allocator) }
    if error.kind!=.None {
        if error.kind==.Invalid_Scene && error.scene==.Entity_Not_Found { return {kind=.Rebuild_Required} }
        return error
    }; defer model_batch_destroy(&candidate)
    if len(candidate.entries)!=len(batch.entries) || len(candidate.vertices)!=len(batch.vertices) { return {kind=.Rebuild_Required} }
    for entry,i in candidate.entries {
        previous:=batch.entries[i]
        if entry.material_sources!=previous.material_sources || entry.material_digests!=previous.material_digests || entry.primitive_mesh!=previous.primitive_mesh || entry.entity!=previous.entity || entry.node!=previous.node || entry.primitive!=previous.primitive || entry.first_vertex!=previous.first_vertex || entry.vertex_count!=previous.vertex_count || entry.source_nodes!=previous.source_nodes || entry.source_primitives!=previous.source_primitives || entry.source_vertices!=previous.source_vertices || entry.source_indices!=previous.source_indices || entry.source_images!=previous.source_images || entry.source_textures!=previous.source_textures || entry.source_samplers!=previous.source_samplers || entry.material.double_sided!=previous.material.double_sided || entry.material.alpha_mode!=previous.material.alpha_mode || entry.has_sampling!=previous.has_sampling || entry.samplers!=previous.samplers { return {kind=.Rebuild_Required} }
        for view,v in entry.views { if view.texture!=previous.views[v].texture { return {kind=.Rebuild_Required} } }
    }
    changed:=false
    for vertex,i in candidate.vertices { if vertex!=batch.vertices[i] { changed=true; break } }
    copy(batch.vertices,candidate.vertices); copy(batch.objects,candidate.objects); copy(batch.entries,candidate.entries)
    batch.geometry_changed=changed; batch.streaming=candidate.streaming
    if changed { batch.geometry_revision+=1 }
    return {}
}
/// Releases only CPU arrays using the allocator captured during successful preparation.
model_batch_destroy :: proc(batch:^Model_Batch) {
    delete(batch.vertices,batch.allocator); delete(batch.objects,batch.allocator); delete(batch.entries,batch.allocator); delete(batch.ids,batch.allocator); batch^={}
}

/// Stores camera depth from the perspective frame's homogeneous clip W.
model_batch_update_depths :: proc(batch:^Model_Batch,view_projection:km.Mat4) {
    for &entry in batch.entries { entry.camera_depth=km.matrix_vector(view_projection,km.vec4(entry.world_center,1))[3] }
}
