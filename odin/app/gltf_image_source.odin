//! Texture refresh reads through the same source capability without replacing the authored model.
package app

import resources "../resources"
import "core:slice"

/// Reads one current image revision; returned bytes belong to the supplied allocator.
/// Embedded images are re-extracted from a complete validated source model.
scene_model_image_read :: proc(owner:^Authoring,source:^Scene_Model,index:int,allocator:=context.allocator)->([]byte,Gltf_Error) {
    if owner==nil || source==nil || index<0 || index>=len(source.model.images) { return nil,.Invalid_Data }
    scope,error:=asset_path_scope(owner,source.source.root,source.source.path)
    if error!=.None { return nil,.Invalid_Path }; defer asset_path_scope_destroy(&scope)
    image:=source.model.images[index]
    if image.source_path!="" {
        bytes,read_error:=resources.read_bytes(&scope.root,image.source_path,32*1024*1024)
        if read_error!=.None { return nil,.IO }; defer delete(bytes,scope.root.allocator)
        result:=slice.clone(bytes,allocator); return result,.None
    }
    model,load_error:=gltf_load(&scope.root,scope.path,allocator)
    if load_error!=.None { return nil,load_error }; defer gltf_model_destroy(&model)
    if len(model.images)!=len(source.model.images) { return nil,.Invalid_Data }
    return slice.clone(model.images[index].encoded,allocator),.None
}
