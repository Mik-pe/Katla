//! Image choices own only portable paths; immutable image generations belong to the application.
package agent

import ecs "../ecs"
import "core:encoding/json"
import "core:mem"
import "core:strings"

Material_Texture_Kind :: enum { Inherit, Neutral, File, Gltf_Image }
Material_Asset_Root :: enum { Resource, Scene, File }
/// A decoded path is owned by its captured request allocator.
Material_Texture_Source :: struct { kind:Material_Texture_Kind,root:Material_Asset_Root,path:string,image_index:u32 }
Material_Set_Texture :: struct { entities:[]ecs.Entity_Id,role:Material_Texture_Role,source:Material_Texture_Source }
/// Releases the source path using the request's captured allocator.
material_texture_source_destroy :: proc(source:^Material_Texture_Source,allocator:mem.Allocator) { delete(source.path,allocator); source^={} }
/// Decodes an independent wire source for asset/document consumers; returns an owned path.
material_texture_source_decode :: proc(data:[]byte,allocator:=context.allocator)->(Material_Texture_Source,Call_Error) {
    context.allocator=allocator
    index,validation:=material_arguments_validate(data,allocator); if validation!=.None { return {},validation }
    value,error:=json.parse(data,spec=.JSON,parse_integers=false,allocator=allocator); if error!=nil { return {},.Invalid_JSON }; defer json.destroy_value(value)
    source,valid:=material_texture_source_value(value,index,allocator); if !valid { return {},.Invalid_Arguments }; return source,.None
}
@(private="package")
material_texture_source_value :: proc(value:json.Value,index:u64,allocator:mem.Allocator)->(Material_Texture_Source,bool) {
    object,valid:=value.(json.Object); if !valid { return {},false }; kind,has_kind:=required_string(object,"kind"); if !has_kind { return {},false }
    source:Material_Texture_Source
    switch kind {
    case "inherit","neutral":
        if !material_keys_valid(object,{"kind"}) { return {},false }; source.kind=.Neutral if kind=="neutral" else .Inherit; return source,true
    case "file": source.kind=.File; if !material_keys_valid(object,{"kind","asset"}) { return {},false }
    case "gltf_image":
        source.kind=.Gltf_Image; if !material_keys_valid(object,{"kind","asset","image_index"}) { return {},false }
        number,present:=object["image_index"]; if !present || material_is_null(number) || index>u64(max(u32)) { return {},false }; source.image_index=u32(index)
    case: return {},false
    }
    asset,has_asset:=object["asset"].(json.Object); if !has_asset || len(asset)!=1 { return {},false }
    for key,item in asset {
        path,is_path:=item.(string); if !is_path || len(path)==0 || len(path)>4096 || strings.contains(path,"\x00") { return {},false }
        switch key {
        case "Resource": source.root=.Resource
        case "Scene": source.root=.Scene
        case "File": source.root=.File
        case: return {},false
        }
        // Path confinement and explicit File capabilities are checked by the retained application root.
        source.path=strings.clone(path,allocator)
    }
    return source,true
}
