//! Scene capture exports authored component DTOs and rebases reproducible asset sources to a destination.
package app

import editor "../editor"
import ecs "../ecs"
import km "../math"
import ron "../encoding/ron"
import "core:encoding/json"
import "core:strings"

@(private="package")
scene_json_put :: proc(fields:^json.Object,name:string,value:json.Value) { fields^[strings.clone(name)]=value }
@(private="package")
scene_row_has :: proc(row:Scene_Entity,name:string)->bool { for component in row.components { if component.name==name { return true } }; return false }
/// Rebases a known source to the destination's directory without copying assets or traversing parents.
scene_asset_reference :: proc(app:^Authoring,path:string,root:Mesh_Path_Root,origin:string)->(json.Value,bool) {
    if root==.Resource { return trigger_json_value(struct {Resource:string}{path}),true }
    separator:=strings.last_index_byte(origin,'/'); prefix:=""; if separator>=0 { prefix=origin[:separator+1] }
    if strings.has_prefix(path,prefix) { return trigger_json_value(struct {Scene:string}{path[len(prefix):]}),true }
    roots:=ecs.get_resource_mut(&app.world,Asset_Roots); if roots==nil { return nil,false }
    resource_prefix:=""; project_prefix:=strings.concatenate({roots.project.path,"/"}); defer delete(project_prefix)
    if strings.has_prefix(roots.resource.path,project_prefix) { resource_prefix=roots.resource.path[len(project_prefix):] }
    resource_child:=strings.concatenate({resource_prefix,"/"}); defer delete(resource_child)
    if len(resource_prefix)>0 && strings.has_prefix(path,resource_child) { return trigger_json_value(struct {Resource:string}{path[len(resource_child):]}),true }
    absolute:=strings.concatenate({roots.project.path,"/",path}); defer delete(absolute)
    return trigger_json_value(struct {File:string}{absolute}),true
}
@(private="package")
scene_export_mesh :: proc(app:^Authoring,row:Scene_Entity,fields:^json.Object,origin:string)->editor.Scene_Error {
    if scene_row_has(row,"SceneModel") {
        value,decoded:=scene_row_owned_decode(app,row,"SceneModel"); defer scene_row_owned_destroy(app,"SceneModel",value); if !decoded { return .Decode_Failed }
        source:=(cast(^Scene_Model)value).source; path,valid:=scene_asset_reference(app,source.path,source.root,origin); if !valid { return .Invalid_Operation }; defer json.destroy_value(path)
        descriptor:=trigger_json_value(struct {GltfModel:struct {path:json.Value}}{{path}}); if descriptor==nil { return .Decode_Failed }; scene_json_put(fields,"source",descriptor); return .None
    }
    if !scene_row_has(row,"SceneMesh") { scene_json_put(fields,"source",strings.clone("Empty")); return .None }
    value,decoded:=scene_row_owned_decode(app,row,"SceneMesh"); defer scene_row_owned_destroy(app,"SceneMesh",value)
    if !decoded { return .Decode_Failed }; source:=(cast(^Scene_Mesh)value).source
    switch source.kind {
    case .Empty: scene_json_put(fields,"source",strings.clone("Empty"))
    case .Recipe:
        path,valid:=scene_asset_reference(app,source.path,source.root,origin); if !valid { return .Invalid_Operation }; defer json.destroy_value(path)
        descriptor:=trigger_json_value(struct {MeshAsset:struct {path:json.Value}}{{path}}); if descriptor==nil { return .Decode_Failed }; scene_json_put(fields,"source",descriptor)
    case .Geometry:
        tree,parse_error:=json.parse(source.geometry,spec=.JSON,parse_integers=true); if parse_error!=nil { return .Decode_Failed }
        geometry,is_geometry:=tree.(json.Object); if !is_geometry { json.destroy_value(tree); return .Decode_Failed }
        kind,is_kind:=geometry["kind"].(string); if !is_kind { json.destroy_value(tree); return .Decode_Failed }
        variant:=""; switch kind {
        case "cube": variant="Cube"
        case "sphere": variant="Sphere"
        case "plane": variant="Plane"
        case "cylinder": variant="Cylinder"
        case "cone","triangles": json.destroy_value(tree); scene_json_put(fields,"source",strings.clone("Empty")); return .None
        case "torus": variant="Torus"
        case: json.destroy_value(tree); return .Invalid_Operation
        }
        for key in geometry { if key=="kind" { delete_key(&geometry,key); delete(key); break } }; delete(kind)
        descriptor:=make(json.Object,context.allocator); descriptor[strings.clone(variant)]=geometry; scene_json_put(fields,"source",descriptor)
    }
    return .None
}

/// Encodes current scene v3 built-ins and versioned registered extension DTOs with document-local references.
scene_document_encode :: proc(app:^Authoring,snapshot:^Scene_Snapshot,name,origin:string)->(json.Value,editor.Scene_Error) {
    context.allocator=app.world.allocator
    document:=make(json.Object,app.world.allocator); success:=false; defer { if !success { json.destroy_value(document) } }
    if snapshot.next_entity_id==0 { return nil,.Invalid_Operation }
    scene_json_put(&document,"version",json.Integer(3)); scene_json_put(&document,"name",strings.clone(name)); scene_json_put(&document,"next_entity_id",scene_document_key_value(snapshot.next_entity_id))
    entities:=make(json.Array,0,app.world.allocator)
    transferred_entities:=false; defer { if !transferred_entities { json.destroy_value(entities) } }
    for row in snapshot.entities {
        fields:=make(json.Object,app.world.allocator); transferred:=false; defer { if !transferred { json.destroy_value(fields) } }
        scene_json_put(&fields,"id",scene_document_key_value(u64(row.key)))
        transform,has_transform:=scene_row_owned_decode(app,row,"SceneTransform"); defer scene_row_owned_destroy(app,"SceneTransform",transform)
        if !has_transform { return nil,.Component_Not_Found }; scene_json_put(&fields,"transform",trigger_json_value((cast(^Scene_Transform)transform).local))
        if scene_row_has(row,"SceneName") { value,ok:=scene_row_owned_decode(app,row,"SceneName"); defer scene_row_owned_destroy(app,"SceneName",value); if !ok { return nil,.Decode_Failed }; scene_json_put(&fields,"name",strings.clone((cast(^Scene_Name)value).name)) }
        if scene_row_has(row,"SceneParent") { value,ok:=scene_row_owned_decode(app,row,"SceneParent"); defer scene_row_owned_destroy(app,"SceneParent",value); if !ok { return nil,.Decode_Failed }; parent:=(cast(^Scene_Parent)value).entity; scene_json_put(&fields,"parent",scene_document_key_value(u64(parent))) }
        if err:=scene_export_mesh(app,row,&fields,origin); err!=.None { return nil,err }
        if scene_row_has(row,"SurfaceMaterial") {
            value,ok:=scene_row_owned_decode(app,row,"SurfaceMaterial"); defer scene_row_owned_destroy(app,"SurfaceMaterial",value); if !ok { return nil,.Decode_Failed }
            surface:=(cast(^Surface_Material)value)^; descriptor:=trigger_json_value(struct {metallic,roughness,ao:f32}{surface.metallic,surface.roughness,surface.ao})
            if surface.has_tint { object:=descriptor.(json.Object); color:=km.color_to_srgb(surface.linear_color); scene_json_put(&object,"color",trigger_json_value([4]f32{color.r,color.g,color.b,color.a})); descriptor=object }
            scene_json_put(&fields,"drawable",descriptor)
        }
        if err:=scene_builtin_components_encode(app,row,&fields,origin); err!=.None { return nil,err }
        extensions:=make(json.Object,app.world.allocator); extensions_transferred:=false; defer { if !extensions_transferred { json.destroy_value(extensions) } }
        if scene_row_has(row,"SceneUnknown") {
            value,ok:=scene_row_owned_decode(app,row,"SceneUnknown"); defer scene_row_owned_destroy(app,"SceneUnknown",value); if !ok { return nil,.Decode_Failed }
            tree,parse_error:=json.parse((cast(^Scene_Unknown)value).components,spec=.JSON,parse_integers=true); if parse_error!=nil { return nil,.Decode_Failed }; unknown,is_unknown:=tree.(json.Object)
            if !is_unknown { json.destroy_value(tree); return nil,.Decode_Failed }; delete(extensions); extensions=unknown
        }
        for component in row.components {
            known:=false
            for builtin in ([17]string{"PhysicsJoint","SceneModel","SceneKey","SceneName","SceneTransform","SceneParent","SceneMesh","SurfaceMaterial","SceneUnknown","AnimationPlayer","ParticleEmitter","Script","PhysicsBody","TriggerVolume","TriggerRules","Velocity","SceneSource"}) { if component.name==builtin { known=true; break } }
            if component.name=="AnimationModel" && scene_row_has(row,"SceneModel") { known=true }
            if component.name=="SceneMesh" && !scene_mesh_has_builtin_source(component.data) { known=false }
            if known { continue }
            tree,parse_error:=ron.parse_json(component.data); if parse_error.kind!=.None { return nil,.Decode_Failed }; defer json.destroy_value(tree)
            bytes,write_error:=ron.write(tree); if write_error.kind!=.None { return nil,.Decode_Failed }; defer delete(bytes)
            descriptor:=trigger_json_value(struct {version:u32,data:string}{1,string(bytes)})
            if old,exists:=extensions[component.name]; exists { json.destroy_value(old); extensions[component.name]=descriptor }
            else { scene_json_put(&extensions,component.name,descriptor) }
        }
        if len(extensions)>0 { scene_json_put(&fields,"components",extensions); extensions_transferred=true }
        append(&entities,fields); transferred=true
    }
    scene_json_put(&document,"entities",entities); transferred_entities=true; success=true; return document,.None
}

@(private="package")
scene_mesh_has_builtin_source :: proc(data:[]byte)->bool {
    tree,err:=json.parse(data,spec=.JSON,parse_integers=true); if err!=nil { return false }; defer json.destroy_value(tree)
    source,is_source:=tree.(json.Object); if !is_source { return false }
    kind,is_kind:=source["kind"].(string); if !is_kind { return false }; if kind!="Geometry" { return true }
    descriptor,is_descriptor:=source["document"].(json.Object); if !is_descriptor { return false }; geometry_kind,is_geometry_kind:=descriptor["kind"].(string)
    return is_geometry_kind && geometry_kind!="cone" && geometry_kind!="triangles"
}
