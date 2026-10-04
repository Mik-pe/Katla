//! Scene factors, geometry and camera values are application-authored GPU inputs.
package render

import app ".."
import km "../../math"
import gfx "../../gfx"
import ecs "../../ecs"
import editor "../../editor"
import "core:mem"
import m "core:math"

/// Shader-compatible vertex pulling retains actual geometry without core mesh policy.
Vertex :: struct { position,normal,uv:km.Vec4 }
/// Immutable triangle-stream geometry is owned by the application asset consumer.
Geometry :: struct { vertices:[]Vertex, allocator:mem.Allocator }
/// One per-slot frame block; ambient.w adapts clip Y without transposing matrices.
Frame_Data :: struct { view_projection:km.Mat4, camera_position,light_direction,light_color,ambient:km.Vec4 }
/// Per-object storage preserves exact authored linear factors and model-space normals.
Object_Data :: struct { model,normal_model:km.Mat4, linear_color,factors:km.Vec4 }
/// Camera policy belongs to the app, with a finite perspective and world-space target.
Camera :: struct { position,target,up:km.Vec3, fov_degrees,near,far:f32 }
/// Input failures precede any native allocation or scene mutation.
Scene_Error :: enum { None, Invalid_Geometry, Invalid_Camera, Invalid_Transform, Invalid_Material }
/// Explicit identity defaults for an ordinary orbit-style scene camera.
camera_default :: proc()->Camera { return {{0,0,4},{0,0,0},{0,1,0},55,0.05,1000} }
/// Builds a finite Vulkan-depth camera; Metal adaptation is one explicit clip-space sign.
frame_data :: proc(camera:Camera,width,height:u32,clip_y_down:bool)->(Frame_Data,Scene_Error) {
    if m.is_nan(camera.fov_degrees)||m.is_inf(camera.fov_degrees)||m.is_nan(camera.near)||m.is_inf(camera.near)||m.is_nan(camera.far)||m.is_inf(camera.far) { return {},.Invalid_Camera }
    if width==0 || height==0 || !(camera.fov_degrees>0 && camera.fov_degrees<180) || !(camera.near>0 && camera.far>camera.near) { return {},.Invalid_Camera }
    for value in camera.position { if m.is_nan(value)||m.is_inf(value) { return {},.Invalid_Camera } }
    for value in camera.target { if m.is_nan(value)||m.is_inf(value) { return {},.Invalid_Camera } }
    for value in camera.up { if m.is_nan(value)||m.is_inf(value) { return {},.Invalid_Camera } }
    forward:=camera.target-camera.position
    if km.length_squared(forward)<1e-10 || km.length_squared(km.cross(forward,camera.up))<1e-10 { return {},.Invalid_Camera }
    view,ok:=km.inverse(km.mat4_lookat(camera.position,camera.target,camera.up)); if !ok { return {},.Invalid_Camera }
    projection:=km.mat4_perspective(camera.fov_degrees,f32(width)/f32(height),camera.near,camera.far)
    sign:f32=1 if clip_y_down else -1
    return {km.matrix_mul(projection,view),km.vec4(camera.position,1),{-0.4,-0.7,-1,0},{1,0.95,0.9,3},{0.07,0.07,0.07,sign}},.None
}
/// Validates affine inputs and computes inverse-transpose normals for nonuniform scaling.
object_data :: proc(transform:km.Transform,surface:app.Surface_Material)->(Object_Data,Scene_Error) {
    for value in transform.position { if m.is_nan(value)||m.is_inf(value) { return {},.Invalid_Transform } }
    for value in transform.scale { if m.is_nan(value)||m.is_inf(value)||value==0 { return {},.Invalid_Transform } }
    for value in transform.rotation { if m.is_nan(value)||m.is_inf(value) { return {},.Invalid_Transform } }
    if abs(km.quat_dot(transform.rotation,transform.rotation)-1)>1e-4 { return {},.Invalid_Transform }
    return object_data_matrix(km.transform_to_mat4(transform),surface)
}
/// Retains exact hierarchy shear and uses the mathematical normal transform without decomposition.
object_data_matrix :: proc(model:km.Mat4,surface:app.Surface_Material)->(Object_Data,Scene_Error) {
    for column in model { for value in column { if m.is_nan(value)||m.is_inf(value) { return {},.Invalid_Transform } } }
    if model[0][3]!=0 || model[1][3]!=0 || model[2][3]!=0 || model[3][3]!=1 { return {},.Invalid_Transform }
    if !(surface.metallic>=0 && surface.metallic<=1 && surface.roughness>=0 && surface.roughness<=1 && surface.ao>=0 && surface.ao<=1) { return {},.Invalid_Material }
    color:=km.COLOR_WHITE
    if surface.has_tint { color=surface.linear_color; if !km.color_is_valid(color) { return {},.Invalid_Material } }
    inverse,ok:=km.inverse(model); if !ok { return {},.Invalid_Transform }
    // Normal vectors require the mathematical inverse transpose under nonuniform scale.
    normal:=km.transpose(inverse); normal[3]={0,0,0,1}
    return {model,normal,km.color_to_array(color),{surface.metallic,surface.roughness,surface.ao,0}},.None
}
/// Reads the current authored surface and exact composed scene placement on its owner thread.
scene_object_data :: proc(owner:^app.Authoring,entity:ecs.Entity_Id)->(Object_Data,editor.Scene_Error) {
    if !ecs.entity_exists(&owner.world,entity) { return {},.Entity_Not_Found }
    surface,ok:=ecs.get_component(&owner.world,entity,app.Surface_Material); if !ok { return {},.Component_Not_Found }
    model,error:=app.scene_world_matrix(owner,entity); if error!=.None { return {},error }
    data,scene_error:=object_data_matrix(model,surface)
    if scene_error!=.None { return {},.Invalid_Operation }
    return data,.None
}
/// Releases authored vertex storage using its captured allocator.
geometry_destroy :: proc(geometry:^Geometry) { delete(geometry.vertices,geometry.allocator); geometry^={} }
/// Expands indexed data into one owned triangle stream after preflighting every index.
geometry_from_indexed :: proc(positions,normals:[]km.Vec3,uvs:[]km.Vec2,indices:[]u32,allocator:=context.allocator)->(Geometry,Scene_Error) {
    if len(positions)==0 || len(normals)!=len(positions) || (len(uvs)>0 && len(uvs)!=len(positions)) || len(indices)==0 || len(indices)%3!=0 { return {},.Invalid_Geometry }
    for index in indices { if u64(index)>=u64(len(positions)) { return {},.Invalid_Geometry } }
    for p in positions { for value in p { if m.is_nan(value)||m.is_inf(value) { return {},.Invalid_Geometry } } }
    for n in normals { for value in n { if m.is_nan(value)||m.is_inf(value) { return {},.Invalid_Geometry } }; if km.length_squared(n)<1e-10 { return {},.Invalid_Geometry } }
    for uv in uvs { for value in uv { if m.is_nan(value)||m.is_inf(value) { return {},.Invalid_Geometry } } }
    geometry:=Geometry{make([]Vertex,len(indices),allocator),allocator}
    for index,i in indices {
        uv:=km.Vec2{}; if len(uvs)>0 { uv=uvs[index] }
        geometry.vertices[i]={km.vec4(positions[index],1),km.vec4(km.normalize(normals[index])),{uv[0],uv[1],0,0}}
    }
    return geometry,.None
}
/// Converts the canonical application's indexed mesh into the shader's triangle-stream ABI.
geometry_from_mesh :: proc(mesh:^app.Mesh_Geometry,allocator:=context.allocator)->(Geometry,Scene_Error) {
    if len(mesh.vertices)==0 || len(mesh.indices)==0 { return {},.Invalid_Geometry }
    positions:=make([]km.Vec3,len(mesh.vertices),allocator); defer delete(positions,allocator)
    normals:=make([]km.Vec3,len(mesh.vertices),allocator); defer delete(normals,allocator)
    uvs:=make([]km.Vec2,len(mesh.vertices),allocator); defer delete(uvs,allocator)
    for vertex,i in mesh.vertices { positions[i]=vertex.position; normals[i]=vertex.normal; uvs[i]=vertex.uv }
    return geometry_from_indexed(positions,normals,uvs,mesh.indices,allocator)
}
/// Builds GPU input from the application's single sphere implementation.
geometry_sphere :: proc(longitudes,latitudes:u32,allocator:=context.allocator)->(Geometry,Scene_Error) {
    mesh,error:=app.mesh_sphere(0.5,int(longitudes),int(latitudes),allocator)
    if error!=.None { return {},.Invalid_Geometry }; defer app.mesh_geometry_destroy(&mesh)
    return geometry_from_mesh(&mesh,allocator)
}
/// Describes the real buffers used by the canonical surface shader.
scene_buffer_descs :: proc(vertex_count,object_count:int)->(frame,objects,geometry:gfx.Buffer_Desc,ok:bool) {
    if vertex_count<=0 || object_count<=0 || u64(vertex_count)>max(u64)/u64(size_of(Vertex)) || u64(object_count)>max(u64)/u64(size_of(Object_Data)) { return {},{},{},false }
    return {size=u64(size_of(Frame_Data)),usage={.Uniform},memory=.CPU_Visible},{size=u64(object_count)*u64(size_of(Object_Data)),usage={.Storage},memory=.CPU_Visible},{size=u64(vertex_count)*u64(size_of(Vertex)),usage={.Storage},memory=.GPU_Private},true
}
