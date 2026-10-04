//! Imported model components persist source identity and prepare complete owned revisions before publication.
package app

import ecs "../ecs"
import editor "../editor"
import "core:mem"
import "core:strings"
import "core:encoding/json"

Gltf_Source_Kind :: enum { Model,Group,Primitive }
Gltf_Source :: struct { path:string, root:Mesh_Path_Root,kind:Gltf_Source_Kind,node_index,primitive_index:u32 }
@(private="package")
Scene_Model_Revision :: struct { model:Gltf_Model,references:u32,allocator:mem.Allocator }
/// One glTF source and its owned immutable CPU revision; native resources remain renderer-owned.
Scene_Model :: struct { source:Gltf_Source `inspect:"skip"`, model:Gltf_Model `inspect:"skip"`,revision:^Scene_Model_Revision `inspect:"skip"`,allocator:mem.Allocator `inspect:"skip"` }
/// Releases prepared model/source ownership after ECS or history relinquishes this revision.
scene_model_destroy :: proc(value:rawptr) {
    model:=cast(^Scene_Model)value; allocator:=model.allocator; if allocator.procedure==nil { allocator=model.model.allocator }
    if allocator.procedure==nil { allocator=context.allocator }
    delete(model.source.path,allocator)
    if model.revision!=nil {
        model.revision.references-=1
        if model.revision.references==0 { gltf_model_destroy(&model.revision.model); free(model.revision,model.revision.allocator) }
    } else { gltf_model_destroy(&model.model) }
    model^={}
}
@(private="package")
scene_model_clone :: proc(dst,src:rawptr) {
    source:=cast(^Scene_Model)src; target:=cast(^Scene_Model)dst
    target^=source^; target.allocator=context.allocator; target.source.path=strings.clone(source.source.path)
    if source.revision!=nil { assert(source.revision.references<max(u32)); source.revision.references+=1 }
    else { target.model=gltf_model_clone(&source.model) }
}
/// Loads all resources, geometry, skins and clips before any entity or native owner changes.
scene_model_prepare :: proc(app:^Authoring,source:Gltf_Source)->(Scene_Model,Gltf_Error) {
    context.allocator=app.world.allocator
    if source.kind not_in (bit_set[Gltf_Source_Kind]{.Model,.Group,.Primitive}) || (source.kind!=.Primitive && (source.node_index!=0 || source.primitive_index!=0)) { return {},.Invalid_Data }
    if (!strings.has_suffix(source.path,".glb") && !strings.has_suffix(source.path,".gltf")) { return {},.Invalid_Path }
    if cached,present:=scene_model_preparation_find(app,source); present {
        if source.kind==.Primitive { if _,valid:=scene_model_selected_primitive(&cached); !valid { scene_model_destroy(&cached); return {},.Invalid_Data } }
        return cached,.None
    }
    scope,scope_error:=asset_path_scope(app,source.root,source.path); if scope_error!=.None { return {},.Invalid_Path }; defer asset_path_scope_destroy(&scope)
    model,error:=gltf_load(&scope.root,scope.path,app.world.allocator)
    if error!=.None { return {},error }
    result:=Scene_Model{source=source,model=model,allocator=app.world.allocator}; result.source.path=strings.clone(source.path)
    if source.kind==.Primitive { if _,valid:=scene_model_selected_primitive(&result); !valid { scene_model_destroy(&result); return {},.Invalid_Data } }
    if source.kind!=.Model { scene_model_revision_own(&result,app.world.allocator) }
    scene_model_preparation_insert(app,&result)
    return result,.None
}
@(private="package")
scene_model_revision_own :: proc(model:^Scene_Model,allocator:mem.Allocator) {
    if model.revision!=nil { return }; revision:=new(Scene_Model_Revision,allocator); revision^={model=model.model,references=1,allocator=allocator}; model.revision=revision
}
/// Resolves a node-global selector and the primitive's ordinal within that node's mesh.
scene_model_selected_primitive :: proc(component:^Scene_Model)->(^Gltf_Primitive,bool) {
    if component==nil || component.source.kind!=.Primitive || u64(component.source.node_index)>=u64(len(component.model.nodes)) { return nil,false }
    node:=component.model.nodes[component.source.node_index]; if node.mesh<0 { return nil,false }
    active,error:=gltf_active_nodes(&component.model,component.model.allocator); if error!=.None { return nil,false }; defer delete(active,component.model.allocator)
    if !active[component.source.node_index] { return nil,false }
    ordinal:u32
    for &primitive in component.model.primitives { if primitive.mesh==u32(node.mesh) {
        if ordinal==component.source.primitive_index { return &primitive,true }; ordinal+=1
    } }; return nil,false
}
/// Shares one immutable prepared CPU revision among independently authored primitive entities.
scene_model_select :: proc(source:^Scene_Model,node_index,primitive_index:u32,allocator:=context.allocator)->(Scene_Model,bool) {
    scene_model_revision_own(source,allocator)
    result:=source^; result.source.kind=.Primitive; result.source.node_index=node_index; result.source.primitive_index=primitive_index
    if _,valid:=scene_model_selected_primitive(&result); !valid { return {},false }
    result.allocator=allocator; result.source.path=strings.clone(source.source.path,allocator); assert(source.revision.references<max(u32)); source.revision.references+=1; return result,true
}
@(private="package")
scene_model_encode :: proc(state,value:rawptr,allocator:mem.Allocator)->([]byte,bool) {
    source:=(cast(^Scene_Model)value).source
    root:=asset_root_name(source.root)
    kind:="model"; if source.kind==.Group { kind="group" } else if source.kind==.Primitive { kind="primitive" }
    bytes,error:=json.marshal(struct {path,root,kind:string,node_index,primitive_index:u32}{source.path,root,kind,source.node_index,source.primitive_index},allocator=allocator)
    return bytes,error==nil
}
@(private="package")
scene_model_decode :: proc(state:rawptr,bytes:[]byte,allocator:mem.Allocator)->(rawptr,bool) {
    context.allocator=allocator; owner:=cast(^Authoring)state
    result:=new(Scene_Model,allocator); result.model.allocator=allocator
    tree,error:=json.parse(bytes,spec=.JSON,parse_integers=false,allocator=allocator)
    if error!=nil { return result,false }; defer json.destroy_value(tree)
    object,is_object:=tree.(json.Object)
    if !is_object || !recipe_keys(object,{"path","root","kind","node_index","primitive_index"}) { return result,false }
    path,is_path:=object["path"].(string); if !is_path { return result,false }
    source:=Gltf_Source{path=path}
    if value,present:=object["kind"]; present {
        kind,is_kind:=value.(string); if !is_kind { return result,false }
        switch kind {
        case "model": source.kind=.Model
        case "group": source.kind=.Group
        case "primitive": source.kind=.Primitive
        case: return result,false
        }
    }
    if !scene_model_source_index(object,"node_index",&source.node_index) || !scene_model_source_index(object,"primitive_index",&source.primitive_index) { return result,false }
    if source.kind!=.Primitive && (source.node_index!=0 || source.primitive_index!=0) { return result,false }
    if value,present:=object["root"]; present {
        root,is_root:=value.(string); if !is_root { return result,false }
        switch root {
        case "resource": source.root=.Resource
        case "project": source.root=.Project
        case "file": source.root=.File
        case: return result,false
        }
    }
    prepared,prepare_error:=scene_model_prepare(owner,source)
    if prepare_error!=.None { return result,false }; result^=prepared; return result,true
}
@(private="package")
scene_model_source_index :: proc(object:json.Object,name:string,target:^u32)->bool {
    value,present:=object[name]; if !present { return true }; number:f64
    #partial switch v in value {
    case json.Integer: number=f64(v)
    case json.Float: number=f64(v)
    case: return false
    }
    if !(number>=0 && number<=f64(max(u32))) || number!=f64(u32(number)) { return false }; target^=u32(number); return true
}
/// Registers source-only persistence and deep history cloning on the stationary authoring owner.
scene_model_register :: proc(app:^Authoring) {
    editor.editor_register(&app.world,&app.registry,"SceneModel",Scene_Model{},ecs.Value_Ops{scene_model_destroy,scene_model_clone},spawn_default=false)
    entry:=app.registry.entries["SceneModel"]; entry.value_state=app; entry.encode_owned=scene_model_encode; entry.decode_owned=scene_model_decode
}
