//! Colliders use genuine selected or aggregate bind-pose triangles, including skin and morph deformation.
package app
import km "../math"
import editor "../editor"

@(private="package")
scene_model_collision_geometry :: proc(component:^Scene_Model,allocator:=context.allocator)->(Mesh_Geometry,editor.Scene_Error) {
    context.allocator=allocator
    model:=&component.model
    active,error:=gltf_active_nodes(model,allocator); if error!=.None { return {},error }; defer delete(active,allocator)
    world,pose_error:=gltf_world_matrices(model,nil,allocator); if pose_error!=.None { return {},pose_error }; defer delete(world,allocator)
    positions:=make([dynamic]km.Vec3,allocator); defer delete(positions)
    indices:=make([dynamic]u32,allocator); defer delete(indices)
    for node,node_index in model.nodes {
        if !active[node_index] || node.mesh<0 || (component.source.kind==.Primitive && component.source.node_index!=u32(node_index)) { continue }
        ordinal:u32
        for primitive,primitive_index in model.primitives {
            if primitive.mesh!=u32(node.mesh) { continue }
            local_ordinal:=ordinal; ordinal+=1
            if component.source.kind==.Primitive && component.source.primitive_index!=local_ordinal { continue }
            geometry,geometry_error:=gltf_deform_geometry(model,u32(primitive_index),u32(node_index),world,node.weights,allocator)
            if geometry_error!=.None { return {},.Invalid_Operation }; defer mesh_geometry_destroy(&geometry)
            if len(geometry.vertices)>MAX_MESH_VERTICES-len(positions) || len(geometry.indices)>MAX_MESH_INDICES-len(indices) { return {},.Invalid_Operation }
            base:=u32(len(positions)); for vertex in geometry.vertices { append(&positions,km.transform_point(world[node_index],vertex.position)) }
            mirrored:=km.determinant(world[node_index])<0
            for triangle:=0;triangle<len(geometry.indices);triangle+=3 {
                if triangle+2>=len(geometry.indices) { return {},.Invalid_Field_Value }
                a,b,c:=geometry.indices[triangle],geometry.indices[triangle+1],geometry.indices[triangle+2]
                for index in ([3]u32{a,b,c}) { if int(index)>=len(geometry.vertices) { return {},.Invalid_Field_Value } }
                if mirrored { b,c=c,b }; append(&indices,base+a,base+b,base+c)
            }
        }
    }
    result,mesh_error:=mesh_triangles(positions[:],indices[:],allocator=allocator)
    if mesh_error!=.None { return {},.Invalid_Field_Value }; return result,.None
}
