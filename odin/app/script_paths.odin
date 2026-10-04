//! Script sources retain installed roots or explicit file capabilities granted by scene admission.
package app
import ecs "../ecs"
import editor "../editor"
import resources "../resources"
import script "../script"
import "core:strings"
import "core:mem"
import "core:os"
import "core:path/filepath"

Script_Sources :: struct { files:map[string]Asset_Path_Scope,allocator:mem.Allocator }
/// Records only newly admitted exact files; failed staging revokes those capabilities.
Script_Source_Preparation :: struct { added:[dynamic]string,allocator:mem.Allocator,active:bool }
@(private="package")
script_sources_destroy :: proc(value:rawptr) {
    owner:=cast(^Script_Sources)value
    for path,scope in owner.files { retained:=scope; delete(retained.path,owner.allocator); asset_path_scope_destroy(&retained); delete(path,owner.allocator) }
    delete(owner.files); owner^={}
}
@(private="package")
script_sources_get :: proc(app:^Authoring)->^Script_Sources {
    if owner:=ecs.get_resource_mut(&app.world,Script_Sources); owner!=nil { return owner }
    ecs.insert_resource(&app.world,Script_Sources{make(map[string]Asset_Path_Scope,app.world.allocator),app.world.allocator},ecs.Value_Ops{destroy=script_sources_destroy})
    return ecs.get_resource_mut(&app.world,Script_Sources)
}
/// Temporarily retains exact File descriptors from an explicitly selected scene or prefab.
script_sources_prepare :: proc(app:^Authoring,snapshot:^Scene_Snapshot)->(Script_Source_Preparation,editor.Scene_Error) {
    token:=Script_Source_Preparation{added=make([dynamic]string,app.world.allocator),allocator=app.world.allocator,active=true}
    accepted:=false; defer { if !accepted { script_sources_finish(app,&token,false) } }
    for row in snapshot.entities {
        found:=false; for component in row.components { if component.name=="Script" { found=true; break } }; if !found { continue }
        raw,valid:=scene_row_owned_decode(app,row,"Script"); if !valid { return {},.Decode_Failed }
        component:=(cast(^Script_Component)raw)^
        if component.root==.File {
            if error:=script_sources_admit(app,&token,component.path); error!=.None { scene_row_owned_destroy(app,"Script",raw); return {},error }
        }
        scene_row_owned_destroy(app,"Script",raw)
    }
    accepted=true; return token,.None
}
/// Adds one explicit source selected by a native file picker to the same staging token.
script_sources_admit :: proc(app:^Authoring,token:^Script_Source_Preparation,path:string)->editor.Scene_Error {
    if !token.active || !asset_file_path_valid(path) || !strings.has_suffix(path,".luau") { return .Invalid_Field_Value }
    owner:=script_sources_get(app); if _,present:=owner.files[path]; present { return .None }
    if len(owner.files)>=100_000 { return .Invalid_Operation }
    scope,error:=asset_path_scope(app,.File,path); if error!=.None { return .Invalid_Operation }
    source,read_error:=resources.read_text(&scope.root,scope.path,1024*1024); delete(source,app.world.allocator)
    if read_error!=.None { asset_path_scope_destroy(&scope); return .Invalid_Operation }
    scope.path=strings.clone(scope.path,app.world.allocator)
    identity:=strings.clone(path,app.world.allocator); owner.files[identity]=scope; append(&token.added,identity); return .None
}
/// Commits capabilities after scene publication or revokes only those created by failed admission.
script_sources_finish :: proc(app:^Authoring,token:^Script_Source_Preparation,committed:bool) {
    if !token.active { return }
    if !committed {
        if owner:=ecs.get_resource_mut(&app.world,Script_Sources); owner!=nil {
            for path in token.added { if scope,present:=owner.files[path]; present { delete_key(&owner.files,path); retained:=scope; delete(retained.path,owner.allocator); asset_path_scope_destroy(&retained); delete(path,owner.allocator) } }
        }
    }
    delete(token.added); token^={}
}
/// Normalizes an explicit source descriptor without granting filesystem authority.
script_source_name :: proc(path:string,kind:Mesh_Path_Root,allocator:=context.allocator)->(string,bool) {
    if kind not_in (bit_set[Mesh_Path_Root]{.Resource,.Project,.File}) { return "",false }
    if kind==.File { if !asset_file_path_valid(path) { return "",false } }
    else if !resources.valid_relative_path(path) { return "",false }
    extension:=filepath.ext(path); base:=path[:len(path)-len(extension)]
    normalized:=strings.concatenate({base,".luau"},allocator)
    if kind==.Resource && !strings.has_prefix(normalized,"scripts/") { prefixed:=strings.concatenate({"scripts/",normalized},allocator); delete(normalized,allocator); normalized=prefixed }
    return normalized,true
}
/// Normalizes authored names while preserving explicit document origins and exact File authority.
script_source_resolve :: proc(app:^Authoring,path:string,kind:Mesh_Path_Root=.Resource)->(Script_Component,editor.Scene_Error) {
    context.allocator=app.world.allocator
    if filepath.is_abs(path) {
        normalized,valid_name:=script_source_name(path,.File,app.world.allocator); if !valid_name { return {},.Invalid_Field_Value }; defer delete(normalized)
        if kind==.File {
            owner:=ecs.get_resource_mut(&app.world,Script_Sources); if owner==nil { return {},.Invalid_Field_Value }; if _,present:=owner.files[normalized]; !present { return {},.Invalid_Field_Value }
            return {path=strings.clone(normalized),root=.File},.None
        }
        absolute,absolute_error:=os.get_absolute_path(normalized,app.world.allocator); if absolute_error!=nil { return {},.Invalid_Field_Value }; defer delete(absolute)
        identity,root,valid:=asset_identify_path(app,absolute); if !valid { return {},.Invalid_Field_Value }
        defer delete(identity)
        if root==.File {
            owner:=ecs.get_resource_mut(&app.world,Script_Sources); if owner==nil { return {},.Invalid_Field_Value }
            if _,present:=owner.files[identity]; !present { return {},.Invalid_Field_Value }
            return {path=strings.clone(identity),root=root},.None
        }
        return script_source_resolve(app,identity,root)
    }
    if kind==.File { return {},.Invalid_Field_Value }
    normalized,valid:=script_source_name(path,kind,app.world.allocator); if !valid { return {},.Invalid_Field_Value }
    return {path=normalized,root=kind},.None
}
/// Reads an installed source or a previously admitted exact File without opening new authority.
script_source_read :: proc(app:^Authoring,component:Script_Component)->([]byte,editor.Scene_Error) {
    resolved,error:=script_source_resolve(app,component.path,component.root); if error!=.None { return nil,error }; defer delete(resolved.path,app.world.allocator)
    if resolved.root==.File {
        owner:=ecs.get_resource_mut(&app.world,Script_Sources); scope,present:=owner.files[resolved.path]; if !present { return nil,.Invalid_Field_Value }
        source,read_error:=resources.read_text(&scope.root,scope.path,1024*1024); if read_error!=.None { return nil,.Invalid_Operation }; return source,.None
    }
    scope,scope_error:=asset_path_scope(app,resolved.root,resolved.path); if scope_error!=.None { return nil,.Invalid_Operation }; defer asset_path_scope_destroy(&scope)
    source,read_error:=resources.read_text(&scope.root,scope.path,1024*1024); if read_error!=.None { return nil,.Invalid_Operation }; return source,.None
}
/// Validates edited bytes with the canonical sandbox while preserving the current live instance.
script_source_check :: proc(app:^Authoring,source,path:string)->(string,editor.Scene_Error) {
    owner:=ecs.get_resource_mut(&app.world,Script_Native_Runtime); if owner==nil { return "",.Application_Owned }
    failure:=script.validate(owner.runtime,source,path); if failure!="" { return failure,.Invalid_Operation }; return "",.None
}
