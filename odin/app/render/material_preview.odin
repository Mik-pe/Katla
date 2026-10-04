//! Material previews own accepted source samples independently of same-frame scene replacement.
package render

import gfx "../../gfx"
import ecs "../../ecs"
import app ".."
import ui "../../ui"
import image "../../image"
import "core:mem"

Material_Preview_Receipt :: struct { textures:[5]ui.Texture_Id,entity:ecs.Entity_Id,ready:bool,changed:int,error:Native_Error }
@(private="package")
Material_Preview_Entry :: struct { texture:ui.Texture_Id,native:Native_Texture,digest:[32]byte,format:gfx.Texture_Format,image:i32,srgb:bool }
/// Update and destruction occur after the owner's GPU wait; current frame IDs survive scene actions.
Material_Preview_Cache :: struct($R:typeid) { ui:^UI_GPU(R),operations:GPU_Ops(R),entries:[5]Material_Preview_Entry,retired:[dynamic]Material_Preview_Entry,next_id:u64,entity:ecs.Entity_Id,has_entity:bool,allocator:mem.Allocator }
@(private="package")
material_preview_sample_size :: proc(format:image.Image_Format)->int {
    switch format {
    case .RGBA8:return 4
    case .RGBA16:return 8
    case .RGBA32_Float:return 16
    };return 0
}
@(private="package")
material_preview_resize :: proc(source:^Texture_Image,allocator:mem.Allocator)->(Texture_Image,Texture_Image_Error) {
    stride:=material_preview_sample_size(source.format)
    if stride==0 || source.width==0 || source.height==0 || u64(source.width)*u64(source.height)*u64(stride)!=u64(len(source.pixels)) { return {},.Invalid_Data }
    longest:=max(source.width,source.height);edge:=min(longest,u32(128))
    width:=max(u32(1),u32(u64(source.width)*u64(edge)/u64(longest)));height:=max(u32(1),u32(u64(source.height)*u64(edge)/u64(longest)))
    pixels,error:=mem.make([]byte,int(width*height)*stride,allocator)
    if error!=nil || raw_data(pixels)==nil { return {},.Allocation }
    result:=Texture_Image{width=width,height=height,pixels=pixels,allocator=allocator,format=source.format}
    for y in 0..<height { for x in 0..<width {
        sx:=min(source.width-1,u32((u64(x)*2+1)*u64(source.width)/(u64(width)*2)));sy:=min(source.height-1,u32((u64(y)*2+1)*u64(source.height)/(u64(height)*2)))
        copy(result.pixels[int(y*width+x)*stride:int(y*width+x+1)*stride],source.pixels[int(sy*source.width+sx)*stride:int(sy*source.width+sx+1)*stride])
    } };return result,.None
}
/// Creates a five-role owner; texture IDs are distinct from viewports and asset thumbnails.
material_preview_init :: proc(cache:^Material_Preview_Cache($R),owner:^UI_GPU(R),operations:GPU_Ops(R),allocator:mem.Allocator=context.allocator)->Native_Error {
    if cache.ui!=nil || owner==nil || owner.renderer==nil || operations.destroy_texture==nil { return {gpu=.Invalid_Resource} }
    cache^={ui=owner,operations=operations,next_id=1<<50,retired=make([dynamic]Material_Preview_Entry,allocator),allocator=allocator};return {}
}
@(private="package")
material_preview_release :: proc(cache:^Material_Preview_Cache($R),entry:Material_Preview_Entry)->Native_Error {
    if entry.texture==0 { return {} }
    error:=cache.operations.destroy_texture(cache.ui.renderer,entry.native.texture);if error!=.None { return {gpu=error} }
    registry:=ui_gpu_remove_texture(cache.ui,entry.texture);if registry!=.None { return {gpu=.Invalid_Resource} };return {}
}
@(private="package")
material_preview_image :: proc(owner:^app.Authoring,entry:Model_Entry,role:int,texture:Model_Texture,allocator:mem.Allocator)->(Texture_Image,Native_Error) {
    source:Texture_Image;owned:=false;defer { if owned { texture_image_destroy(&source) } }
    if len(texture.encoded)>0 {
        decoded,error:=texture_image_decode(texture.encoded,allocator);if error!=.None { return {},{scene=.Invalid_Material} };source=decoded;owned=true
    } else if texture.image== -3 {
        accepted:=material_native_image(owner,entry.entity,role,texture.digest);if accepted==nil { return {},{scene=.Invalid_Material} };source=accepted.image
    } else if texture.image== -2 {
        values:=[4]f32{.5,.5,1,1};source={width=1,height=1,pixels=mem.slice_to_bytes(values[:]),format=.RGBA32_Float}
        result,error:=material_preview_resize(&source,allocator);if error!=.None { return {},{scene=.Invalid_Material} };return result,{}
    } else if texture.image== -1 {
        white:=[4]byte{255,255,255,255};source={width=1,height=1,pixels=white[:],format=.RGBA8}
        result,error:=material_preview_resize(&source,allocator);if error!=.None { return {},{scene=.Invalid_Material} };return result,{}
    } else { return {},{scene=.Invalid_Material} }
    result,error:=material_preview_resize(&source,allocator);if error!=.None { return {},{scene=.Invalid_Material} };return result,{}
}
/// Publishes all changed roles together, using the first accepted primitive of the selected entity.
/// Decode, allocation or registration rejection preserves the previous five previews.
material_preview_update :: proc(cache:^Material_Preview_Cache($R),models:^Native_Model(R),owner:^app.Authoring,entity:ecs.Entity_Id)->Material_Preview_Receipt {
    receipt:=Material_Preview_Receipt{entity=entity}
    if cache.has_entity && cache.entity==entity { receipt.ready=true;for item,i in cache.entries { receipt.textures[i]=item.texture } }
    if cache.ui==nil || cache.ui.prepared { receipt.error={gpu=.Busy};return receipt }
    for len(cache.retired)>0 { error:=material_preview_release(cache,cache.retired[len(cache.retired)-1]);if error!={} { receipt.error=error;return receipt };pop(&cache.retired) }
    index:= -1
    if models!=nil { for entry,i in models.batch.entries { if entry.entity==entity { index=i;break } } }
    if index<0 { receipt.ready=false;receipt.textures={};return receipt }
    if models.renderer!=cache.ui.renderer || index>=len(models.receipts) { receipt.error={gpu=.Invalid_Resource};return receipt }
    candidate:=cache.entries;created:[5]bool;success:=false;changed:int
    defer { if !success { for item,i in candidate { if created[i] { material_preview_release(cache,item) } } } }
    entry:=models.batch.entries[index]
    for role in 0..<5 {
        texture_index:=models.receipts[index].textures[role]
        if texture_index<0 || texture_index>=len(models.textures) { receipt.error={gpu=.Invalid_Resource};return receipt }
        texture:=models.textures[texture_index];digest:=texture.digest
        if len(texture.encoded)>0 { digest=thumbnail_digest(texture.encoded) }
        old:=candidate[role]
        if old.texture!=0 && old.digest==digest && old.format==texture.native.desc.format && old.image==texture.image && old.srgb==texture.srgb { continue }
        decoded,error:=material_preview_image(owner,entry,role,texture,cache.allocator);if error!={} { receipt.error=error;return receipt }
        native,native_error:=native_texture_upload(cache.ui.renderer,cache.operations,&decoded,texture.srgb,cache.allocator);texture_image_destroy(&decoded)
        if native_error!={} { receipt.error=native_error;return receipt }
        id:=ui.Texture_Id(cache.next_id);cache.next_id+=1
        if registry:=ui_gpu_texture(cache.ui,id,{handle=native.texture,desc=native.desc,encoding=.Linear});registry!=.None { cache.operations.destroy_texture(cache.ui.renderer,native.texture);receipt.error={gpu=.Invalid_Resource};return receipt }
        candidate[role]={id,native,digest,texture.native.desc.format,texture.image,texture.srgb};created[role]=true;changed+=1
    }
    for old,i in cache.entries { if created[i] && old.texture!=0 { append(&cache.retired,old) } }
    cache.entries=candidate;cache.entity=entity;cache.has_entity=true
    for item,i in candidate { receipt.textures[i]=item.texture };receipt.ready=true;receipt.changed=changed;success=true;return receipt
}
/// Releases independent previews only after UI recordings have retired.
material_preview_destroy :: proc(cache:^Material_Preview_Cache($R))->Native_Error {
    if cache.ui==nil { return {} }
    for len(cache.retired)>0 { error:=material_preview_release(cache,cache.retired[len(cache.retired)-1]);if error!={} { return error };pop(&cache.retired) }
    for &entry in cache.entries { error:=material_preview_release(cache,entry);if error!={} { return error };entry={} }
    delete(cache.retired);cache^={};return {}
}
