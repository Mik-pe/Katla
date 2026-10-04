//! Authored image choices retain immutable decoded CPU revisions through atomic history.
package app

import ecs "../ecs"
import editor "../editor"
import image "../image"
import resources "../resources"
import "core:mem"
import "core:slice"
import "core:strings"
import "core:encoding/json"
import "core:crypto/sha2"

/// One image snapshot owns its source identity and exact decoded samples.
Material_Image :: struct { source:Texture_Source,image:image.Texture_Image,digest:[32]byte }
/// CPU revisions are internal application resources; portable files contain only MaterialTextures.
Material_Images :: struct { roles:[5]Material_Image `inspect:"skip"` }
/// Releases source descriptors and samples with their captured allocators.
material_images_destroy :: proc(value:^Material_Images,allocator:=context.allocator) {
    for &role in value.roles { delete(role.source.path,allocator); image.texture_image_destroy(&role.image) }; value^={}
}
@(private="package")
material_images_destroy_value :: proc(value:rawptr) { material_images_destroy(cast(^Material_Images)value) }
@(private="package")
material_images_clone_value :: proc(dst,src:rawptr) {
    target,source:=cast(^Material_Images)dst,cast(^Material_Images)src; target^=source^
    for &role in target.roles { role.source.path=strings.clone(role.source.path); role.image.pixels=slice.clone(role.image.pixels); role.image.allocator=context.allocator }
}
@(private="package")
material_images_encode_value :: proc(state,value:rawptr,allocator:mem.Allocator)->([]byte,bool) {
    source:=cast(^Material_Images)value
    rows:[5]struct {kind:Texture_Source_Kind,root:Mesh_Path_Root,path:string,image_index,width,height:u32,digest:[32]byte}
    for role,index in source.roles { rows[index]={role.source.kind,role.source.root,role.source.path,role.source.image_index,role.image.width,role.image.height,role.digest} }
    bytes,error:=json.marshal(rows,allocator=allocator); return bytes,error==nil
}
/// Registers internal image snapshots with deep command ownership and deterministic comparison.
material_images_register :: proc(owner:^Authoring) {
    editor.editor_register(&owner.world,&owner.registry,"MaterialImages",Material_Images{},ecs.Value_Ops{material_images_destroy_value,material_images_clone_value},spawn_default=false,inspector_add=false,inspector_remove=false)
    entry:=owner.registry.entries["MaterialImages"]; entry.encode_owned=material_images_encode_value
}
@(private="package")
material_source_equal :: proc(a,b:Texture_Source)->bool { return a.kind==b.kind && a.root==b.root && a.path==b.path && a.image_index==b.image_index }
@(private="package")
material_image_digest :: proc(decoded:^image.Texture_Image)->[32]byte {
    digest:sha2.Context_256; sha2.init_256(&digest); dimensions:=[3]u32{decoded.width,decoded.height,u32(decoded.format)}
    sha2.update(&digest,mem.slice_to_bytes(dimensions[:])); sha2.update(&digest,decoded.pixels); result:[32]byte; sha2.final(&digest,result[:]); return result
}
/// Resolves a selected image through retained capability scopes before native preparation.
material_image_prepare :: proc(owner:^Authoring,source:Texture_Source)->(Material_Image,editor.Scene_Error) {
    if !texture_source_valid(source) { return {},.Invalid_Operation }
    result:=Material_Image{source=source}; result.source.path=strings.clone(source.path,owner.world.allocator)
    accepted:=false; defer { if !accepted { delete(result.source.path,owner.world.allocator); image.texture_image_destroy(&result.image) } }
    if source.kind==.Inherit || source.kind==.Neutral { accepted=true; return result,.None }
    scope,scope_error:=asset_path_scope(owner,source.root,source.path); if scope_error!=.None { return {},.Invalid_Operation }; defer asset_path_scope_destroy(&scope)
    encoded:[]byte
    if source.kind==.File {
        bytes,read_error:=resources.read_bytes(&scope.root,scope.path,image.TEXTURE_IMAGE_MAX_ENCODED_BYTES)
        if read_error!=.None { return {},.Invalid_Operation }; encoded=bytes
    } else {
        model,load_error:=gltf_load(&scope.root,scope.path,owner.world.allocator); if load_error!=.None { return {},.Invalid_Operation }; defer gltf_model_destroy(&model)
        if u64(source.image_index)>=u64(len(model.images)) { return {},.Invalid_Operation }; encoded=slice.clone(model.images[source.image_index].encoded,owner.world.allocator)
    }
    defer delete(encoded,owner.world.allocator)
    decoded,decode_error:=image.texture_image_decode(encoded,owner.world.allocator); if decode_error!=.None { return {},.Decode_Failed }; result.image=decoded
    result.digest=material_image_digest(&decoded); accepted=true; return result,.None
}
/// Prepares every changed source first, retaining exact accepted samples when identities match.
material_images_prepare :: proc(owner:^Authoring,sources:Texture_Assignments,previous:^Material_Images=nil,refresh_role:int= -1)->(Material_Images,editor.Scene_Error) {
    result:Material_Images; accepted:=false; defer { if !accepted { material_images_destroy(&result,owner.world.allocator) } }
    source_copy:=sources; choices:=texture_assignments_roles(&source_copy)
    for choice,index in choices {
        if index!=refresh_role && previous!=nil && material_source_equal(choice^,previous.roles[index].source) {
            prior:=previous.roles[index]; result.roles[index]=prior; result.roles[index].source.path=strings.clone(prior.source.path,owner.world.allocator)
            result.roles[index].image.pixels=slice.clone(prior.image.pixels,owner.world.allocator); result.roles[index].image.allocator=owner.world.allocator
        } else {
            prepared,error:=material_image_prepare(owner,choice^); if error!=.None { return {},error }; result.roles[index]=prepared
        }
    }
    accepted=true; return result,.None
}
/// Ensures staged source descriptors have actual prepared images before any GPU publication.
material_images_prepare_entities :: proc(owner:^Authoring,ids:[]ecs.Entity_Id)->editor.Scene_Error {
    for id in ids {
        choices,present:=ecs.get_component(&owner.world,id,Texture_Assignments); if !present { continue }
        previous:=ecs.get_component_mut(&owner.world,id,Material_Images)
        same:=previous!=nil; for source,index in texture_assignments_roles(&choices) { if previous==nil || !material_source_equal(source^,previous.roles[index].source) { same=false; break } }
        if same { continue }
        prepared,error:=material_images_prepare(owner,choices,previous); if error!=.None { return error }
        ecs.add_component(&owner.world,id,prepared)
    }
    return .None
}
