//! Intentional File assets use an exact retained parent capability and confined basename.
package app

import ecs "../ecs"
import resources "../resources"
import "core:strings"
import "core:path/filepath"
import "core:unicode/utf8"

/// Borrows installed roots or owns the explicit parent directory of an absolute File asset.
Asset_Path_Scope :: struct { root:resources.Root,path:string,owned:bool }
/// An intentional absolute path must still describe one regular confined child name.
asset_file_path_valid :: proc(path:string)->bool {
    if !filepath.is_abs(path) || !utf8.valid_string(path) { return false }
    for character in path { if character==0 { return false } }
    return resources.valid_relative_path(filepath.base(path))
}
/// Accepts project-relative or intentional absolute document destinations.
asset_document_path_valid :: proc(path:string)->bool { return resources.valid_relative_path(path) || asset_file_path_valid(path) }
/// Opens a source scope without passing any unrestricted filename to an asset decoder.
asset_path_scope :: proc(owner:^Authoring,kind:Mesh_Path_Root,path:string)->(Asset_Path_Scope,resources.Error) {
    if kind==.File {
        if !asset_file_path_valid(path) { return {},.Invalid_Path }
        root,error:=resources.root_open(filepath.dir(path),owner.world.allocator); if error!=.None { return {},error }
        return {root,filepath.base(path),true},.None
    }
    roots:=ecs.get_resource_mut(&owner.world,Asset_Roots)
    if roots==nil || !resources.valid_relative_path(path) { return {},.Invalid_Path }
    switch kind {
    case .Resource: return {roots.resource,path,false},.None
    case .Project: return {roots.project,path,false},.None
    case .File:
    }
    return {},.Invalid_Path
}
/// Releases only the scope's explicitly owned directory capability.
asset_path_scope_destroy :: proc(scope:^Asset_Path_Scope) { if scope.owned { resources.root_destroy(&scope.root) }; scope^={} }
/// Captures a stable absolute source identity independently from the process working directory.
asset_absolute_path :: proc(owner:^Authoring,kind:Mesh_Path_Root,path:string)->(string,bool) {
    if kind==.File { if !asset_file_path_valid(path) { return "",false }; return strings.clone(path,owner.world.allocator),true }
    roots:=ecs.get_resource_mut(&owner.world,Asset_Roots); if roots==nil || !resources.valid_relative_path(path) { return "",false }
    base:=""; switch kind {
    case .Resource: base=roots.resource.path
    case .Project: base=roots.project.path
    case .File:
    }
    if base=="" { return "",false }; return strings.concatenate({base,"/",path},owner.world.allocator),true
}
/// Retains project-relative identity where possible and explicit File identity outside the project.
asset_identify_path :: proc(owner:^Authoring,absolute:string)->(string,Mesh_Path_Root,bool) {
    if !asset_file_path_valid(absolute) { return "",.File,false }
    identity:=absolute
    when ODIN_OS==.Windows { identity=asset_windows_identity(absolute,owner.world.allocator); defer delete(identity,owner.world.allocator) }
    if roots:=ecs.get_resource_mut(&owner.world,Asset_Roots); roots!=nil {
        for root,index in ([2]resources.Root{roots.resource,roots.project}) {
            root_path:=root.path
            when ODIN_OS==.Windows { root_path=asset_windows_identity(root.path,owner.world.allocator); defer delete(root_path,owner.world.allocator) }
            prefix:=strings.concatenate({root_path,"/"},owner.world.allocator); defer delete(prefix,owner.world.allocator)
            if strings.has_prefix(identity,prefix) {
                relative:=identity[len(prefix):]; if !resources.valid_relative_path(relative) { return "",.File,false }
                kind:Mesh_Path_Root=.Resource; if index==1 { kind=.Project }; return strings.clone(relative,owner.world.allocator),kind,true
            }
        }
    }
    return strings.clone(absolute,owner.world.allocator),.File,true
}
@(private="package")
asset_windows_identity :: proc(path:string,allocator:=context.allocator)->string {
    bytes:=make([]byte,len(path),allocator); copy(bytes,path)
    for &character in bytes { if character=='\\' { character='/' } }
    if len(bytes)>=2 && bytes[1]==':' && bytes[0]>='a' && bytes[0]<='z' { bytes[0]-=32 }
    return string(bytes)
}
/// Canonical source codecs retain all three explicit filesystem origins.
asset_root_name :: proc(root:Mesh_Path_Root)->string { switch root {
    case .Resource: return "resource"
    case .Project: return "project"
    case .File: return "file"
    }; return ""
}
