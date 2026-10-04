//! Native material images consume prepared immutable CPU revisions before publication.
package render

import app ".."
import ecs "../../ecs"

@(private="package")
material_native_source :: proc(owner:^app.Authoring,id:ecs.Entity_Id)->^app.Scene_Model { return ecs.get_component_mut(&owner.world,id,app.Scene_Model) }
@(private="package")
material_native_image :: proc(owner:^app.Authoring,id:ecs.Entity_Id,role:int,digest:[32]byte)->^app.Material_Image {
    images:=ecs.get_component_mut(&owner.world,id,app.Material_Images)
    if images==nil || images.roles[role].image.width==0 || len(images.roles[role].image.pixels)==0 || images.roles[role].digest!=digest { return nil }
    return &images.roles[role]
}

@(private="package")
model_texture_prepare :: proc(cache:^Native_Model($R),owner:^app.Authoring,entry:Model_Entry,role:int)->(int,Native_Error) {
    view:=entry.views[role]; kind:=entry.material_sources[role]
    srgb:=role==0 || role==4 || (role==2 && entry.material.workflow==.Specular_Glossiness)
    entity:=entry.entity; image:i32; digest:[32]byte
    prepared:^app.Material_Image
    source:=material_native_source(owner,entity)
    if kind==.File || kind==.GltfImage {
        prepared=material_native_image(owner,entity,role,entry.material_digests[role]); if prepared==nil { return 0,{scene=.Invalid_Material} }
        digest=prepared.digest; entity=0; image= -3
    } else if kind==.Neutral || view.texture<0 { entity=0; image= -2 if role==1 else -1 }
    else {
        if source==nil || int(view.texture)>=len(source.model.textures) { return 0,{scene=.Invalid_Material} }
        image=source.model.textures[view.texture].image
        if image<0 || int(image)>=len(source.model.images) { return 0,{scene=.Invalid_Material} }
    }
    for item,index in cache.textures { if item.entity==entity && item.image==image && item.srgb==srgb && item.digest==digest { return index,{} } }
    decoded:Texture_Image; owned_decoded:=false
    encoded:[]byte; retained:=false
    defer { if owned_decoded { texture_image_destroy(&decoded) }; if !retained { delete(encoded,cache.allocator) } }
    if prepared!=nil { decoded=prepared.image }
    else if image<0 {
        pixels:=make([]byte,4,cache.allocator); copy(pixels,([]byte{128,128,255,255} if image== -2 else []byte{255,255,255,255})); decoded={width=1,height=1,pixels=pixels,allocator=cache.allocator,format=.RGBA8}; owned_decoded=true
    } else {
        bytes,read_error:=app.scene_model_image_read(owner,source,int(image),cache.allocator); if read_error!=.None { return 0,{scene=.Invalid_Material} }; encoded=bytes
        value,decode_error:=texture_image_decode(encoded,cache.allocator); if decode_error!=.None { return 0,{scene=.Invalid_Material} }; decoded=value; owned_decoded=true
    }
    native:Native_Texture; error:Native_Error
    if image== -2 {
        normal:=[8]byte{0x00,0x38,0x00,0x38,0x00,0x3c,0x00,0x3c}
        native,error=native_texture_upload_pixels(cache.renderer,cache.operations.gpu,1,1,.RGBA16_Float,normal[:],cache.allocator)
    } else { native,error=native_texture_upload(cache.renderer,cache.operations.gpu,&decoded,srgb,cache.allocator) }
    if error!={} { return 0,error }
    index:=len(cache.textures); append(&cache.textures,Model_Texture{entity=entity,image=image,srgb=srgb,native=native,encoded=encoded,digest=digest}); retained=true
    return index,{}
}
