//! Native texture ownership and image views retain exact subresource identities.
package katla_vulkan

import gfx ".."
import vk "vendor:vulkan"

@(private="package")
Native_Image_View :: struct { range:gfx.Image_Range, object:vk.ImageView, arrayed:bool }
@(private="package")
Native_Texture :: struct {
    allocation:Image_Memory,
    swapchain:^Native_Swapchain,
    desc:gfx.Texture_Desc,
    views:[dynamic]Native_Image_View,
    layouts:[]vk.ImageLayout,
    initialized:[]bool,
    refs,pending:int,
    latest_submission,content_epoch:u64,
}
@(private="package")
texture_format :: proc(format:gfx.Texture_Format)->vk.Format {
    switch format {
    case .RGBA8_Srgb: return .R8G8B8A8_SRGB
    case .BGRA8_Srgb: return .B8G8R8A8_SRGB
    case .R8_Unorm: return .R8_UNORM
    case .RG8_Unorm: return .R8G8_UNORM
    case .R32_Float: return .R32_SFLOAT
    case .BC1_RGBA_Unorm: return .BC1_RGBA_UNORM_BLOCK
    case .BC3_RGBA_Unorm: return .BC3_UNORM_BLOCK
    case .RGBA8_Unorm: return .R8G8B8A8_UNORM
    case .BGRA8_Unorm: return .B8G8R8A8_UNORM
    case .RGBA16_Float: return .R16G16B16A16_SFLOAT
    case .R32_Uint: return .R32_UINT
    case .D32_Float: return .D32_SFLOAT
    case .D24_Unorm_S8_Uint: return .D24_UNORM_S8_UINT
    case .D32_Float_S8_Uint: return .D32_SFLOAT_S8_UINT
    }
    return .UNDEFINED
}
@(private="package")
image_aspects :: proc(aspects:gfx.Image_Aspects)->vk.ImageAspectFlags {
    result:vk.ImageAspectFlags
    for aspect in aspects {
        switch aspect {
        case .Color: result|={.COLOR}
        case .Depth: result|={.DEPTH}
        case .Stencil: result|={.STENCIL}
        }
    }
    return result
}
@(private="package")
image_range :: proc(range:gfx.Image_Range)->vk.ImageSubresourceRange {
    return {image_aspects(range.aspects),range.base_mip,range.mip_count,range.base_layer,range.layer_count}
}
@(private="package")
texture_state_index :: #force_inline proc(texture:^Native_Texture,mip,layer:u32,aspect:gfx.Image_Aspect)->int {
    return (int(layer)*int(texture.desc.mip_levels)+int(mip))*3+int(aspect)
}
@(private="package")
release_texture :: proc(r:^Renderer,texture:^Native_Texture) {
    texture.refs-=1
    if texture.refs!=0 { return }
    for view in texture.views { r.table.DestroyImageView(r.device,view.object,nil) }
    if texture.swapchain!=nil { release_swapchain(r,texture.swapchain) }
    else {
        r.table.DestroyImage(r.device,texture.allocation.object,nil)
        release_heap(r,texture.allocation.heap)
    }
    delete(texture.views)
    delete(texture.layouts,r.allocator)
    delete(texture.initialized,r.allocator)
    free(texture,r.allocator)
}
/// Creates an uninitialized image whose actual mip/layer layouts begin undefined.
create_texture :: proc(r:^Renderer,desc:gfx.Texture_Desc)->(gfx.Texture_Handle,gfx.Gpu_Error) {
    info,info_error:=texture_create_info(r,desc)
    if info_error!=.None { return {},info_error }
    allocation,err:=image_memory_allocate(r,&info)
    if err!=.None { return {},err }
    texture:=new(Native_Texture,r.allocator)
    texture.allocation=allocation; texture.desc=desc; texture.refs=1
    texture.views=make([dynamic]Native_Image_View,r.allocator)
    texture.layouts=make([]vk.ImageLayout,int(desc.mip_levels)*int(desc.layers)*3,r.allocator)
    texture.initialized=make([]bool,len(texture.layouts),r.allocator)
    return gfx.storage_insert(&r.textures,texture),.None
}
@(private="package")
texture_create_info :: proc(r:^Renderer,desc:gfx.Texture_Desc)->(vk.ImageCreateInfo,gfx.Gpu_Error) {
    if r.device==nil || r.failed { return {},.Native_Failure }
    if !gfx.texture_desc_valid(desc) { return {},.Invalid_Range }
    if .Present in desc.usage { return {},.Unsupported }
    if (desc.format==.BC1_RGBA_Unorm || desc.format==.BC3_RGBA_Unorm) && !r.compression_bc { return {},.Unsupported }
    if desc.depth>1 && (desc.width>r.limits.maxImageDimension3D || desc.height>r.limits.maxImageDimension3D || desc.depth>r.limits.maxImageDimension3D) { return {},.Invalid_Range }
    if desc.depth==1 && (desc.width>r.limits.maxImageDimension2D || desc.height>r.limits.maxImageDimension2D || desc.layers>r.limits.maxImageArrayLayers) { return {},.Invalid_Range }
    if (desc.format==.D24_Unorm_S8_Uint || desc.format==.D32_Float_S8_Uint) && !r.separate_depth_stencil { return {},.Unsupported }
    properties:vk.FormatProperties; r.instance_api.GetPhysicalDeviceFormatProperties(r.physical,texture_format(desc.format),&properties)
    required:vk.FormatFeatureFlags
    if .Sampled in desc.usage { required|={.SAMPLED_IMAGE} }
    if .Storage in desc.usage { required|={.STORAGE_IMAGE} }
    if .Color_Attachment in desc.usage { required|={.COLOR_ATTACHMENT} }
    if .Depth_Attachment in desc.usage { required|={.DEPTH_STENCIL_ATTACHMENT} }
    if .Transfer_Source in desc.usage { required|={.TRANSFER_SRC} }
    if .Transfer_Destination in desc.usage { required|={.TRANSFER_DST} }
    if required&properties.optimalTilingFeatures!=required { return {},.Unsupported }
    usage:vk.ImageUsageFlags
    for kind in desc.usage {
        switch kind {
        case .Sampled: usage|={.SAMPLED}
        case .Storage: usage|={.STORAGE}
        case .Color_Attachment: usage|={.COLOR_ATTACHMENT}
        case .Depth_Attachment: usage|={.DEPTH_STENCIL_ATTACHMENT}
        case .Transfer_Source: usage|={.TRANSFER_SRC}
        case .Transfer_Destination: usage|={.TRANSFER_DST}
        case .Present: return {},.Unsupported
        }
    }
    info:=vk.ImageCreateInfo{sType=.IMAGE_CREATE_INFO,imageType=.D3 if desc.depth>1 else vk.ImageType.D2,format=texture_format(desc.format),extent={desc.width,desc.height,desc.depth},mipLevels=desc.mip_levels,arrayLayers=desc.layers,samples={._1},tiling=.OPTIMAL,usage=usage,sharingMode=.EXCLUSIVE,initialLayout=.UNDEFINED}
    image_properties:vk.ImageFormatProperties
    result:=r.instance_api.GetPhysicalDeviceImageFormatProperties(r.physical,info.format,info.imageType,info.tiling,info.usage,info.flags,&image_properties)
    if result!=.SUCCESS { return {},.Unsupported }
    if desc.width>image_properties.maxExtent.width || desc.height>image_properties.maxExtent.height || desc.depth>image_properties.maxExtent.depth || desc.mip_levels>image_properties.maxMipLevels || desc.layers>image_properties.maxArrayLayers { return {},.Invalid_Range }
    return info,.None
}
/// Invalidates the typed registry identity while accepted work and readbacks retain the image.
destroy_texture :: proc(r:^Renderer,handle:gfx.Texture_Handle)->gfx.Gpu_Error {
    entry,found:=gfx.storage_get(&r.textures,handle)
    if !found || entry^.swapchain!=nil { return .Invalid_Resource }
    texture,ok:=gfx.storage_remove(&r.textures,handle)
    if !ok { return .Invalid_Resource }
    release_texture(r,texture)
    return .None
}
@(private="package")
texture_view :: proc(r:^Renderer,texture:^Native_Texture,range:gfx.Image_Range,arrayed:=false)->(vk.ImageView,gfx.Gpu_Error) {
    if !gfx.image_range_valid(range,texture.desc) { return 0,.Invalid_Range }
    for view in texture.views { if view.range==range && view.arrayed==arrayed { return view.object,.None } }
    kind:=vk.ImageViewType.D3 if texture.desc.depth>1 else vk.ImageViewType.D2_ARRAY if arrayed || range.layer_count>1 else vk.ImageViewType.D2
    info:=vk.ImageViewCreateInfo{sType=.IMAGE_VIEW_CREATE_INFO,image=texture.allocation.object,viewType=kind,format=texture_format(texture.desc.format),subresourceRange=image_range(range)}
    view:vk.ImageView
    if r.table.CreateImageView(r.device,&info,nil,&view)!=.SUCCESS { return 0,.Allocation_Failed }
    append(&texture.views,Native_Image_View{range,view,arrayed})
    return view,.None
}
@(private="package")
query_texture :: proc(state:rawptr,handle:gfx.Texture_Handle)->(gfx.Texture_Info,bool) {
    r:=cast(^Renderer)state
    entry,ok:=gfx.storage_get(&r.textures,handle)
    if !ok { return {},false }
    if entry^.swapchain!=nil && (!r.surface.acquired || r.surface.frame.texture!=handle) { return {},false }
    identity:rawptr=entry^
    if entry^.allocation.heap!=nil { identity=entry^.allocation.heap }
    return {entry^.desc,identity},true
}
