//! Current scene documents bind source descriptions to canonical application component codecs.
package app

import ecs "../ecs"
import editor "../editor"
import resources "../resources"
import ron "../encoding/ron"
import km "../math"
import "core:encoding/json"
import "core:strings"

/// Serializes one registered typed component into an owned document row.
scene_row_component :: proc(app:^Authoring,row:^Scene_Entity,name:string,value:$T)->editor.Scene_Error {
    entry:=app.registry.entries[name]; if entry==nil || entry.T!=T { return .Component_Not_Found }
    copy_value:=value
    data,encoded:=editor.editor_encode_value(entry,&copy_value,app.world.allocator)
    if !encoded { delete(data,app.world.allocator); return .Decode_Failed }
    append(&row.components,Scene_Component{name=strings.clone(name,app.world.allocator),data=data}); return .None
}
/// Transfers a contextual codec's source wire bytes into the row.
scene_row_wire :: proc(app:^Authoring,row:^Scene_Entity,name:string,data:[]byte)->editor.Scene_Error {
    if app.registry.entries[name]==nil { delete(data,app.world.allocator); return .Component_Not_Found }
    append(&row.components,Scene_Component{name=strings.clone(name,app.world.allocator),data=data}); return .None
}
/// Reads either current RON variants or their serde JSON externally tagged form.
scene_variant :: proc(value:json.Value)->(name:string,payload:json.Value,valid:bool) {
    if text,is_text:=value.(string); is_text { return text,nil,true }
    object,is_object:=value.(json.Object); if !is_object { return }
    if text,is_variant:=object["__variant"].(string); is_variant {
        if !recipe_keys(object,{"__variant","__payload"}) { return }; return text,object["__payload"],true
    }
    if len(object)!=1 { return }; for key,item in object { return key,item,true }; return
}
/// Resolves scene-local paths from the retained project origin; external absolute paths are rejected.
scene_asset_path :: proc(app:^Authoring,value:json.Value,origin:string)->(path:string,root:Mesh_Path_Root,valid:bool) {
    kind,payload,is_variant:=scene_variant(value); if !is_variant { return }
    if array,is_array:=payload.(json.Array); is_array { if len(array)!=1 { return }; payload=array[0] }
    text,is_text:=payload.(string); if !is_text { return }
    switch kind {
    case "Resource": if !resources.valid_relative_path(text) { return }; return strings.clone(text,app.world.allocator),.Resource,true
    case "Scene":
        if !resources.valid_relative_path(text) || !resources.valid_relative_path(origin) { return }
        separator:=strings.last_index_byte(origin,'/'); if separator<0 { return strings.clone(text,app.world.allocator),.Project,true }
        return strings.concatenate({origin[:separator+1],text},app.world.allocator),.Project,true
    case "File":
        roots:=ecs.get_resource_mut(&app.world,Asset_Roots); if roots==nil { return }
        prefix:=strings.concatenate({roots.project.path,"/"},app.world.allocator); defer delete(prefix,app.world.allocator)
        if !strings.has_prefix(text,prefix) { return }; relative:=text[len(prefix):]; if !resources.valid_relative_path(relative) { return }
        return strings.clone(relative,app.world.allocator),.Project,true
    }
    return
}
@(private="package")
scene_document_mesh :: proc(app:^Authoring,row:^Scene_Entity,value:json.Value,origin:string)->editor.Scene_Error {
    kind,payload,valid:=scene_variant(value); if value==nil { kind="Empty"; valid=true }; if !valid { return .Decode_Failed }
    data:[]byte; err:json.Marshal_Error
    switch kind {
    case "Empty":
        if payload!=nil { return .Decode_Failed }; data,err=json.marshal(struct {kind:string}{"Empty"},allocator=app.world.allocator)
    case "MeshAsset":
        object,is_object:=payload.(json.Object); if !is_object || !recipe_keys(object,{"path"}) { return .Decode_Failed }
        path,root,path_valid:=scene_asset_path(app,object["path"],origin); if !path_valid { return .Invalid_Operation }; defer delete(path,app.world.allocator)
        root_name:="resource"; if root==.Project { root_name="project" }
        data,err=json.marshal(struct {kind,path,root:string}{"Recipe",path,root_name},allocator=app.world.allocator)
    case "Cube","Sphere","Plane","Cylinder","Cone","Torus":
        object,is_object:=payload.(json.Object); if !is_object { return .Decode_Failed }
        geometry:=make(json.Object,app.world.allocator); defer delete(geometry)
        for key,item in object { geometry[key]=item }
        lower:=strings.to_lower(kind,app.world.allocator); defer delete(lower,app.world.allocator); geometry["kind"]=lower
        if _,_,ok:=recipe_geometry_budget(geometry); !ok { return .Invalid_Operation }
        data,err=json.marshal(struct {kind:string,document:json.Value}{"Geometry",geometry},allocator=app.world.allocator)
    case "Trigger","ParticleEmitter":
        if payload!=nil { return .Decode_Failed }; return .None
    case: return .Application_Owned
    }
    if err!=nil { delete(data,app.world.allocator); return .Decode_Failed }; return scene_row_wire(app,row,"SceneMesh",data)
}
@(private="package")
scene_document_material :: proc(app:^Authoring,row:^Scene_Entity,raw_value:json.Value)->editor.Scene_Error {
    object,is_object:=raw_value.(json.Object); if !is_object || !recipe_keys(object,{"color","metallic","roughness","ao"}) { return .Decode_Failed }
    material:=Surface_Material{roughness=0.5,ao=1}
    if value,present:=object["color"]; present { color,valid:=recipe_vector(value,4); if !valid { return .Invalid_Field_Value }; for axis in color { if axis<0 || axis>1 { return .Invalid_Field_Value } }; material.linear_color=km.color_to_linear(km.Color{color[0],color[1],color[2],color[3]}); material.has_tint=true }
    for field in ([3]string{"metallic","roughness","ao"}) {
        if value,present:=object[field]; present { number,valid:=recipe_number(value); if !valid || number<0 || number>1 { return .Invalid_Field_Value }; switch field {
            case "metallic": material.metallic=number
            case "roughness": material.roughness=number
            case "ao": material.ao=number
            } }
    }
    return scene_row_component(app,row,"SurfaceMaterial",material)
}

/// Converts strict scene v3 descriptors into owned component wire values; preparation happens in shared staging.
scene_document_decode :: proc(app:^Authoring,document:json.Value,origin:string="",prefab:bool=false)->(Scene_Snapshot,editor.Scene_Error) {
    allocator:=app.world.allocator; context.allocator=allocator
    result:=Scene_Snapshot{entities=make([dynamic]Scene_Entity,allocator),allocator=allocator}
    success:=false; defer { if !success { scene_snapshot_destroy(&result) } }
    object,is_object:=document.(json.Object)
    if !is_object || !recipe_keys(object,{"version","name","author","created_at","modified_at","engine_version","next_entity_id","entities"}) { return {},.Decode_Failed }
    version,is_version:=object["version"].(json.Integer); name,is_name:=object["name"].(string); next,is_next:=object["next_entity_id"].(json.Integer)
    entities,is_entities:=object["entities"].(json.Array)
    if !is_version || version!=3 || !is_name || len(name)>4096 || !is_next || next<1 || !is_entities || len(entities)>100_000 { return {},.Decode_Failed }
    for field in ([4]string{"author","created_at","modified_at","engine_version"}) { if value,present:=object[field]; present { if _,is_null:=value.(json.Null); !is_null { if _,is_text:=value.(string); !is_text { return {},.Decode_Failed } } } }
    result.next_entity_id=u64(next); used:=make(map[ecs.Entity_Id]bool,allocator); defer delete(used)
    for entity in entities {
        fields,is_fields:=entity.(json.Object)
        if !is_fields || !recipe_keys(fields,{"id","name","parent","transform","source","drawable","point_light","particle_emitter","animation","velocity","script","perspective","directional_light","audio_emitter","rigid_body","reverb_zone","collider_shape","physics_material","trigger_volume","collision_filter","trigger_rules","joint","components"}) { return {},.Decode_Failed }
        id,is_id:=fields["id"].(json.Integer); if !is_id || id<=0 || id>=next || used[ecs.Entity_Id(id)] { return {},.Invalid_Operation }; used[ecs.Entity_Id(id)]=true
        append(&result.entities,Scene_Entity{key=ecs.Entity_Id(id),components=make([dynamic]Scene_Component,allocator)}); row:=&result.entities[len(result.entities)-1]
        transform,valid_transform:=recipe_transform(fields["transform"]); if !valid_transform { return {},.Invalid_Field_Value }
        if err:=scene_row_component(app,row,"SceneTransform",Scene_Transform{transform}); err!=.None { return {},err }
        if value,present:=fields["name"]; present { if _,is_null:=value.(json.Null); !is_null { text,is_text:=value.(string); if !is_text || len(text)>4096 { return {},.Decode_Failed }; if err:=scene_row_component(app,row,"SceneName",Scene_Name{text}); err!=.None { return {},err } } }
        if value,present:=fields["parent"]; present { if _,is_null:=value.(json.Null); !is_null { parent,is_parent:=value.(json.Integer); if !is_parent || parent<=0 || parent>=next || parent==id { return {},.Invalid_Operation }; if err:=scene_row_component(app,row,"SceneParent",Scene_Parent{ecs.Entity_Id(parent)}); err!=.None { return {},err } } }
        if err:=scene_document_mesh(app,row,fields["source"],origin); err!=.None { return {},err }
        if value,present:=fields["drawable"]; present { if _,is_null:=value.(json.Null); !is_null { if err:=scene_document_material(app,row,value); err!=.None { return {},err } } }
        if err:=scene_builtin_components_decode(app,row,fields,origin); err!=.None { return {},err }
        if components,present:=fields["components"]; present {
            extensions,is_extensions:=components.(json.Object); if !is_extensions { return {},.Decode_Failed }
            unknown:=make(json.Object,allocator); defer delete(unknown)
            for component_name,wire in extensions {
                description,is_description:=wire.(json.Object); if !is_description || !recipe_keys(description,{"version","data"}) { return {},.Decode_Failed }
                component_version,is_component_version:=description["version"].(json.Integer); text,is_text:=description["data"].(string)
                if !is_component_version || component_version<1 || component_version>i64(max(u32)) || !is_text { return {},.Decode_Failed }
                if app.registry.entries[component_name]==nil || component_version!=1 {
                    if prefab { return {},.Component_Not_Found }; unknown[component_name]=wire; continue
                }
                parsed,parse_error:=ron.parse(text,allocator); if parse_error.kind!=.None { return {},.Decode_Failed }; defer json.destroy_value(parsed)
                data,marshal_error:=json.marshal(parsed,allocator=allocator); if marshal_error!=nil { return {},.Decode_Failed }
                if err:=scene_row_wire(app,row,component_name,data); err!=.None { return {},err }
            }
            if len(unknown)>0 {
                data,marshal_error:=json.marshal(unknown,allocator=allocator); if marshal_error!=nil { return {},.Decode_Failed }; defer delete(data,allocator)
                if err:=scene_row_component(app,row,"SceneUnknown",Scene_Unknown{data}); err!=.None { return {},err }
            }
        }
    }
    success=true; return result,.None
}
