//! Asset insertion prepares owned geometry before publishing a new authored entity.
package app

import asset "../agent/assets"
import editor "../editor"
import ecs "../ecs"
import km "../math"
import "core:strings"
import "core:encoding/json"
import "core:fmt"

/// Instantiates one prepared mesh revision using canonical component ownership and history.
asset_instantiate_mesh :: proc(app:^Authoring,request:asset.Prefab_Request)->(editor.Tool_Result,editor.Undo_Group) {
    w:=&app.world; context.allocator=w.allocator; result:=error_result(w,.None)
    if app.mode!=.Editing { result.error=.Editing_Required; return result,{} }
    identity:=ecs.get_resource_mut(w,Scene_Identity)
    if identity==nil || identity.next_entity_id==0 || identity.next_entity_id==max(u64) { result.error=.Invalid_Operation; return result,{} }
    for name in ([5]string{"SceneMesh","SceneName","SceneTransform","SceneKey","SurfaceMaterial"}) {
        if app.registry.entries[name]==nil { result.error=.Component_Not_Found; return result,{} }
    }
    placement:=km.Transform{position=request.position,rotation=km.Quat(request.rotation),scale=request.scale}
    if !km.quat_is_normalized(placement.rotation) { result.error=.Invalid_Operation; return result,{} }
    for value in placement.position { if !mesh_finite(value) { result.error=.Invalid_Operation; return result,{} } }
    for value in placement.scale { if !mesh_finite(value) || value==0 { result.error=.Invalid_Operation; return result,{} } }
    prepared,prepare_error:=scene_mesh_prepare(app,{kind=.Recipe,path=request.path,root=.Project})
    if prepare_error!=.None { result.error=.Invalid_Operation; return result,{} }
    transferred:=false; defer { if !transferred { scene_mesh_destroy(&prepared) } }
    edit,edit_error:=editor.entity_edit_begin(w,&app.registry,0,false)
    if edit_error!=.None { result.error=edit_error; return result,{} }; defer editor.entity_edit_destroy(&edit)
    entity:=ecs.create_entity(w); published:=false; defer { if !published { ecs.destroy_entity(w,entity) } }
    label:=request.name
    if len(label)==0 {
        label=request.path
        if separator:=strings.last_index_byte(label,'/'); separator>=0 { label=label[separator+1:] }
        label=strings.trim_suffix(label,".katmesh")
    }
    ecs.add_component(w,entity,Scene_Name{strings.clone(label,w.allocator)})
    ecs.add_component(w,entity,Scene_Transform{placement})
    ecs.add_component(w,entity,Scene_Key{identity.next_entity_id})
    ecs.add_component(w,entity,Surface_Material{roughness=0.5,ao=1})
    ecs.add_component(w,entity,prepared); transferred=true
    text_id:=fmt.aprintf("%d",u64(entity)); defer delete(text_id,w.allocator)
    data,marshal_error:=json.marshal(struct {
        entity_id,path,name:string,scene_key:u64,vertices,triangles:int,bounds_min,bounds_max:km.Vec3,
    }{text_id,request.path,label,identity.next_entity_id,len(prepared.geometry.vertices),len(prepared.geometry.indices)/3,km.aabb_min(prepared.geometry.bounds),km.aabb_max(prepared.geometry.bounds)},allocator=w.allocator)
    if marshal_error!=nil { result.error=.Decode_Failed; return result,{} }
    result.data=data; append(&result.entities,entity)
    preparation,native_error:=scene_prepare_begin(app,{entity},.Insert)
    if native_error!=.None { clear(&result.entities); result.error=native_error; return result,{} }; defer scene_prepare_finish(&preparation,published)
    group:=editor.entity_edit_finish(&edit,w,entity)
    identity.next_entity_id+=1; published=true
    return result,group
}
