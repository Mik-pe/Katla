//! Queries and editor framing share actual CPU posed drawable bounds and active model scene policy.
package app

import ecs "../ecs"
import editor "../editor"
import km "../math"

/// Chooses default scene roots and their descendants using original glTF node indices.
gltf_active_nodes :: proc(model:^Gltf_Model,allocator:=context.allocator)->([]bool,editor.Scene_Error) {
    active:=make([]bool,len(model.nodes),allocator)
    scene:=model.default_scene; if scene<0 && len(model.scenes)>0 { scene=0 }
    if scene>=0 && int(scene)>=len(model.scenes) { delete(active,allocator); return nil,.Invalid_Operation }
    for i in 0..<len(model.nodes) {
        cursor:=i
        for depth:=0;cursor>=0;depth+=1 {
            if cursor>=len(model.nodes) || depth>=1024 { delete(active,allocator); return nil,.Invalid_Operation }
            if scene>=0 {
                for root in model.scenes[scene].roots { if int(root)>=len(model.nodes) { delete(active,allocator); return nil,.Invalid_Operation }; if u32(cursor)==root { active[i]=true; break } }
            } else if model.nodes[cursor].parent<0 { active[i]=true }
            if active[i] { break }; cursor=int(model.nodes[cursor].parent)
        }
    }
    return active,.None
}
/// Bounds include actual sampled indexed skin/morph geometry in the active imported scene.
/// Empty sources have no bounds; valid transform-only entities use their origin for spatial queries.
scene_drawable_bounds :: proc(owner:^Authoring,id:ecs.Entity_Id)->(km.AABB,bool,editor.Scene_Error) {
    if !ecs.entity_exists(&owner.world,id) { return {},false,.Entity_Not_Found }
    entity_world,world_error:=scene_world_matrix(owner,id); if world_error!=.None { return {},false,world_error }
    for column in entity_world { for value in column { if !mesh_finite(value) { return {},false,.Invalid_Operation } } }
    bounds:km.AABB; has_bounds:bool
    if mesh,present:=ecs.get_component(&owner.world,id,Scene_Mesh); present && len(mesh.geometry.indices)>0 {
        bounds=km.aabb_transform(mesh.geometry.bounds,entity_world); if !scene_bounds_finite(bounds) { return {},false,.Invalid_Operation }; has_bounds=true
    }
    component:=ecs.get_component_mut(&owner.world,id,Scene_Model); if component==nil { return bounds,has_bounds,.None }
    model:=&component.model; allocator:=owner.world.allocator
    player:=ecs.get_component_mut(&owner.world,id,Animation_Player)
    world,pose_error:=gltf_world_matrices(model,player,allocator); if pose_error!=.None { return {},false,pose_error }; defer delete(world,allocator)
    active,active_error:=gltf_active_nodes(model,allocator); if active_error!=.None { return {},false,active_error }; defer delete(active,allocator)
    for node,n in model.nodes {
        if !active[n] || node.mesh<0 { continue }
        weights,weight_error:=animation_sample_weights(&model.animation,player,u32(n),node.weights,allocator)
        if weight_error!=.None { return {},false,weight_error }; defer delete(weights,allocator)
        for primitive,p in model.primitives {
            if primitive.mesh!=u32(node.mesh) { continue }
            geometry,geometry_error:=gltf_deform_geometry(model,u32(p),u32(n),world,weights,allocator)
            if geometry_error!=.None { return {},false,.Invalid_Operation }
            low,high:km.Vec3; indexed:bool
            for index in geometry.indices {
                if int(index)>=len(geometry.vertices) { mesh_geometry_destroy(&geometry); return {},false,.Invalid_Operation }
                position:=geometry.vertices[index].position
                if !indexed { low=position; high=position; indexed=true }
                else { for axis in 0..<3 { low[axis]=min(low[axis],position[axis]); high[axis]=max(high[axis],position[axis]) } }
            }
            mesh_geometry_destroy(&geometry)
            if !indexed { continue }
            transformed:=km.aabb_transform(km.aabb_from_min_max(low,high),km.matrix_mul(entity_world,world[n]))
            if !scene_bounds_finite(transformed) { return {},false,.Invalid_Operation }
            bounds=km.aabb_merge(bounds,transformed) if has_bounds else transformed; has_bounds=true
        }
    }
    return bounds,has_bounds,.None
}
@(private="package")
scene_bounds_finite :: proc(bounds:km.AABB)->bool {
    for value in bounds.center { if !mesh_finite(value) { return false } }
    for value in bounds.extent { if !mesh_finite(value) || value<0 { return false } }
    return true
}
