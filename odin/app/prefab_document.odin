//! Prefab documents validate one identity root and bind internal references through shared scene staging.
package app

import asset "../agent/assets"
import editor "../editor"
import ecs "../ecs"
import resources "../resources"
import ron "../encoding/ron"
import km "../math"
import "core:encoding/json"
import "core:strings"
import "core:fmt"

/// Owns a strict rooted scene template; its keys remain local until insertion.
Prefab_Document :: struct { scene:Scene_Snapshot,root:ecs.Entity_Id }
/// Releases every owned component DTO in a prepared template.
prefab_document_destroy :: proc(document:^Prefab_Document) { scene_snapshot_destroy(&document.scene); document^={} }
/// Checks root identity and complete subtree topology before resolving component descriptions.
prefab_document_decode :: proc(app:^Authoring,document_value:json.Value,origin:string)->(Prefab_Document,editor.Scene_Error) {
    context.allocator=app.world.allocator
    object,is_object:=document_value.(json.Object); if !is_object || !recipe_keys(object,{"version","root","scene"}) { return {},.Decode_Failed }
    version,is_version:=object["version"].(json.Integer); root,is_root:=scene_document_key(object["root"])
    if !is_version || version!=1 || !is_root || root<=0 { return {},.Decode_Failed }
    scene,is_scene:=object["scene"].(json.Object); if !is_scene { return {},.Decode_Failed }; entities,is_entities:=scene["entities"].(json.Array)
    if !is_entities || len(entities)==0 || len(entities)>100_000 { return {},.Invalid_Operation }
    parents:=make(map[u64]u64,app.world.allocator); defer delete(parents)
    has_root:=false
    for entity in entities {
        row,is_row:=entity.(json.Object); if !is_row { return {},.Decode_Failed }; id,is_id:=scene_document_key(row["id"]); if !is_id || id<=0 { return {},.Invalid_Operation }
        if _,duplicate:=parents[id]; duplicate { return {},.Invalid_Operation }
        parent:u64
        if value,present:=row["parent"]; present { if _,is_null:=value.(json.Null); !is_null { valid:bool; parent,valid=scene_document_key(value); if !valid || parent<=0 { return {},.Invalid_Operation } } }
        if id==root {
            transform,valid:=recipe_transform(row["transform"]); if parent!=0 || !valid || !km.transform_is_identity(transform) { return {},.Invalid_Operation }; has_root=true
        } else if parent==0 { return {},.Invalid_Operation }
        parents[id]=parent
    }
    if !has_root { return {},.Invalid_Operation }
    for id in parents {
        current:=id; steps:=0
        for current!=root {
            parent,exists:=parents[current]; if !exists || parent==0 || steps>=len(parents) { return {},.Invalid_Operation }; current=parent; steps+=1
        }
    }
    snapshot,err:=scene_document_decode(app,object["scene"],origin,true); if err!=.None { return {},err }
    return {snapshot,ecs.Entity_Id(root)},.None
}

/// Loads, prepares and atomically writes rooted scene templates through the retained project root.
asset_authoring_prefab :: proc(app:^Authoring,request:asset.Prefab_Request)->(editor.Tool_Result,editor.Undo_Group) {
    allocator:=app.world.allocator; context.allocator=allocator; result:=error_result(&app.world,.None)
    roots:=ecs.get_resource_mut(&app.world,Asset_Roots); if roots==nil { result.error=.Invalid_Operation; return result,{} }
    document:=request.document; owned_document:=false; defer { if owned_document { json.destroy_value(document) } }
    if request.action==.Read || request.action==.Instantiate {
        bytes,read_error:=resources.read_text(&roots.project,request.path); if read_error!=.None { result.error=.Invalid_Operation; return result,{} }; defer delete(bytes,allocator)
        parsed,parse_error:=ron.parse(string(bytes),allocator); if parse_error.kind!=.None { result.error=.Decode_Failed; return result,{} }; document=parsed; owned_document=true
    }
    prefab,decode_error:=prefab_document_decode(app,document,request.path)
    if decode_error!=.None { result.error=decode_error; return result,{} }; defer prefab_document_destroy(&prefab)
    start_key:u64
    if request.action==.Instantiate {
        if app.mode!=.Editing { result.error=.Editing_Required; return result,{} }
        identity:=ecs.get_resource_mut(&app.world,Scene_Identity); if identity==nil || identity.next_entity_id==0 { result.error=.Invalid_Operation; return result,{} }; start_key=identity.next_entity_id
    }
    stage,stage_error:=scene_snapshot_stage(app,&prefab.scene,start_key)
    if stage_error!=.None { result.error=stage_error; return result,{} }
    published:=false; defer scene_stage_destroy(app,&stage,!published)
    if request.action==.Instantiate {
        root:=stage.mapping[prefab.root]
        transform:=km.Transform{position=request.position,rotation=km.Quat(request.rotation),scale=request.scale}
        if !km.quat_is_normalized(transform.rotation) { result.error=.Invalid_Field_Value; return result,{} }
        for vector in ([2]km.Vec3{transform.position,transform.scale}) { for value in vector { if !mesh_finite(value) { result.error=.Invalid_Field_Value; return result,{} } } }
        for axis in transform.scale { if axis==0 { result.error=.Invalid_Field_Value; return result,{} } }
        ecs.add_component(&app.world,root,Scene_Transform{transform})
        if len(request.name)>0 { ecs.add_component(&app.world,root,Scene_Name{strings.clone(request.name,allocator)}) }
        ids:=make([]string,len(stage.entities),allocator); defer { for text in ids { delete(text,allocator) }; delete(ids,allocator) }
        for entity,i in stage.entities { ids[i]=fmt.aprintf("%d",u64(entity)) }
        root_id:=fmt.aprintf("%d",u64(root)); defer delete(root_id,allocator)
        data,marshal_error:=json.marshal(struct {root_entity,path:string,entities:[]string}{root_id,request.path,ids},allocator=allocator)
        if marshal_error!=nil { result.error=.Decode_Failed; return result,{} }; result.data=data
        append(&result.entities,root); for entity in stage.entities { if entity!=root { append(&result.entities,entity) } }
        preparation,prepare_error:=scene_prepare_begin(app,stage.entities[:],.Insert)
        if prepare_error!=.None { clear(&result.entities); result.error=prepare_error; return result,{} }; defer scene_prepare_finish(&preparation,published)
        group:=editor.created_entities_group(&app.world,&app.registry,stage.entities[:])
        ecs.insert_resource(&app.world,Scene_Identity{stage.next_key}); published=true
        return result,group
    }
    public_document,public_ok:=scene_document_public_clone(document); if !public_ok { result.error=.Decode_Failed; return result,{} }; defer json.destroy_value(public_document)
    data,marshal_error:=json.marshal(struct {path:string,document:json.Value,entity_count:int,published:bool}{request.path,public_document,len(stage.entities),false},allocator=allocator)
    if marshal_error!=nil { result.error=.Decode_Failed; return result,{} }; result.data=data
    if request.action==.Write {
        if app.mode!=.Editing { result.error=.Editing_Required; return result,{} }
        published_data,published_error:=json.marshal(struct {path:string,document:json.Value,entity_count:int,published:bool}{request.path,public_document,len(stage.entities),true},allocator=allocator)
        if published_error!=nil { result.error=.Decode_Failed; return result,{} }; selected:=false; defer { if !selected { delete(published_data,allocator) } }
        ron_document,ron_ok:=scene_document_ron_clone(document); if !ron_ok { result.error=.Decode_Failed; return result,{} }; defer json.destroy_value(ron_document)
        bytes,write_error:=ron.write(ron_document,allocator); if write_error.kind!=.None { result.error=.Decode_Failed; return result,{} }; defer delete(bytes,allocator)
        did_publish,write_error_native:=resources.write_atomic(&roots.project,request.path,bytes)
        if did_publish { delete(result.data,allocator); result.data=published_data; selected=true }; if write_error_native!=.None { result.error=.Invalid_Operation }
    }
    return result,{}
}
