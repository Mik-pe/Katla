//! Imported model components persist source identity and prepare complete owned revisions before publication.
package app

import ecs "../ecs"
import editor "../editor"
import "core:mem"
import "core:strings"
import "core:encoding/json"

Gltf_Source :: struct { path:string, root:Mesh_Path_Root }
/// One glTF source and its owned immutable CPU revision; native resources remain renderer-owned.
Scene_Model :: struct { source:Gltf_Source `inspect:"skip"`, model:Gltf_Model `inspect:"skip"` }
/// Releases prepared model/source ownership after ECS or history relinquishes this revision.
scene_model_destroy :: proc(value:rawptr) {
    model:=cast(^Scene_Model)value; allocator:=model.model.allocator
    if allocator.procedure==nil { allocator=context.allocator }
    delete(model.source.path,allocator); gltf_model_destroy(&model.model); model^={}
}
@(private="package")
scene_model_clone :: proc(dst,src:rawptr) {
    source:=cast(^Scene_Model)src; target:=cast(^Scene_Model)dst
    target^={source={path=strings.clone(source.source.path),root=source.source.root},model=gltf_model_clone(&source.model)}
}
/// Loads all resources, geometry, skins and clips before any entity or native owner changes.
scene_model_prepare :: proc(app:^Authoring,source:Gltf_Source)->(Scene_Model,Gltf_Error) {
    context.allocator=app.world.allocator
    roots:=ecs.get_resource_mut(&app.world,Asset_Roots)
    if roots==nil || (!strings.has_suffix(source.path,".glb") && !strings.has_suffix(source.path,".gltf")) { return {},.Invalid_Path }
    root:=&roots.resource; if source.root==.Project { root=&roots.project }
    model,error:=gltf_load(root,source.path,app.world.allocator)
    if error!=.None { return {},error }
    return {source={path=strings.clone(source.path),root=source.root},model=model},.None
}
@(private="package")
scene_model_encode :: proc(state,value:rawptr,allocator:mem.Allocator)->([]byte,bool) {
    source:=(cast(^Scene_Model)value).source
    root:="resource"; if source.root==.Project { root="project" }
    bytes,error:=json.marshal(struct {path,root:string}{source.path,root},allocator=allocator)
    return bytes,error==nil
}
@(private="package")
scene_model_decode :: proc(state:rawptr,bytes:[]byte,allocator:mem.Allocator)->(rawptr,bool) {
    context.allocator=allocator; owner:=cast(^Authoring)state
    result:=new(Scene_Model,allocator); result.model.allocator=allocator
    tree,error:=json.parse(bytes,spec=.JSON,parse_integers=true,allocator=allocator)
    if error!=nil { return result,false }; defer json.destroy_value(tree)
    object,is_object:=tree.(json.Object)
    if !is_object || !recipe_keys(object,{"path","root"}) { return result,false }
    path,is_path:=object["path"].(string); if !is_path { return result,false }
    source:=Gltf_Source{path=path}
    if value,present:=object["root"]; present {
        root,is_root:=value.(string); if !is_root { return result,false }
        switch root {
        case "resource": source.root=.Resource
        case "project": source.root=.Project
        case: return result,false
        }
    }
    prepared,prepare_error:=scene_model_prepare(owner,source)
    if prepare_error!=.None { return result,false }; result^=prepared; return result,true
}
/// Registers source-only persistence and deep history cloning on the stationary authoring owner.
scene_model_register :: proc(app:^Authoring) {
    editor.editor_register(&app.world,&app.registry,"SceneModel",Scene_Model{},ecs.Value_Ops{scene_model_destroy,scene_model_clone},spawn_default=false)
    entry:=app.registry.entries["SceneModel"]; entry.value_state=app; entry.encode_owned=scene_model_encode; entry.decode_owned=scene_model_decode
}
