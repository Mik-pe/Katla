//! Typed image resources and subresource contracts shared by native backends.
package gfx

/// Pixel layouts, independent of native format constants.
Texture_Format :: enum { RGBA8_Unorm, BGRA8_Unorm, RGBA16_Float, R32_Uint, D32_Float, D24_Unorm_S8_Uint, D32_Float_S8_Uint, RGBA8_Srgb, BGRA8_Srgb, R8_Unorm, RG8_Unorm, R32_Float, BC1_RGBA_Unorm, BC3_RGBA_Unorm }
/// Image roles permitted by one native allocation.
Texture_Usage :: enum { Sampled, Storage, Color_Attachment, Depth_Attachment, Transfer_Source, Transfer_Destination, Present }
Texture_Usages :: bit_set[Texture_Usage]
/// Pixel aspects are independently tracked through graph synchronization.
Image_Aspect :: enum { Color, Depth, Stencil }
Image_Aspects :: bit_set[Image_Aspect]
/// Volume textures have one array layer; two-dimensional textures have depth one.
Texture_Desc :: struct { width,height,mip_levels,layers:u32, format:Texture_Format, usage:Texture_Usages, depth:u32 }
/// Nonempty ranges select actual mip levels, layers and aspects.
Image_Range :: struct { base_mip,mip_count,base_layer,layer_count:u32, aspects:Image_Aspects }
/// Logical image identity remains stable as declarations are appended.
Image_Id :: struct { owner:rawptr, index:int }
/// Arrival/final states are explicit contracts for imported images.
Image_State :: enum { Undefined, Shader_Read, Storage, Color_Attachment, Depth_Attachment, Transfer_Source, Transfer_Destination, Present }
/// Native subresource arrival contract; initialized content is independent of layout.
Image_Import :: struct { initial,final:Image_State, initialized:bool }
/// An access names actual subresources and permitted use.
Image_Access :: struct { resource:Image_Id, range:Image_Range, mode:Access_Mode, usage:Texture_Usage }
/// A real overlapping image dependency between authored passes.
Image_Hazard :: struct { before,after:Pass_Id, resource:Image_Id, source,destination:Image_Access }
/// Returns the actual format aspects.
texture_aspects :: proc(format:Texture_Format)->Image_Aspects {
    #partial switch format {
    case .D32_Float: return {.Depth}
    case .D24_Unorm_S8_Uint,.D32_Float_S8_Uint: return {.Depth,.Stencil}
    case: return {.Color}
    }
}
/// Returns bytes per uncompressed texel; compressed formats use block geometry.
texture_pixel_size :: proc(format:Texture_Format)->u32 {
    #partial switch format {
    case .R8_Unorm: return 1
    case .RG8_Unorm: return 2
    case .RGBA16_Float,.D32_Float_S8_Uint: return 8
    case .BC1_RGBA_Unorm,.BC3_RGBA_Unorm: return 0
    case: return 4
    }
}
/// Returns physical compression block geometry and its byte width.
texture_block_layout :: proc(format:Texture_Format)->(width,height,bytes:u32) {
    if format==.BC1_RGBA_Unorm { return 4,4,8 }
    if format==.BC3_RGBA_Unorm { return 4,4,16 }
    return 1,1,texture_pixel_size(format)
}
/// Formats admitted for native filtered mip generation.
texture_filterable_mips :: proc(format:Texture_Format)->bool {
    #partial switch format {
    case .RGBA8_Unorm,.BGRA8_Unorm,.RGBA8_Srgb,.BGRA8_Srgb,.R8_Unorm,.RG8_Unorm,.RGBA16_Float: return true
    case: return false
    }
}
/// Validates dimensions, mip count and roles without native allocation.
texture_desc_valid :: proc(desc:Texture_Desc)->bool {
    if desc.width==0 || desc.height==0 || desc.depth==0 || desc.mip_levels==0 || desc.layers==0 || desc.usage=={} { return false }
    if desc.depth>1 && desc.layers!=1 { return false }
    mips:u32=1; extent:=max(desc.width,desc.height,desc.depth)
    for extent>1 { extent>>=1; mips+=1 }
    if desc.mip_levels>mips { return false }
    depth:=texture_aspects(desc.format)!={.Color}
    compressed:=texture_pixel_size(desc.format)==0
    if depth && (.Color_Attachment in desc.usage || .Storage in desc.usage || desc.depth>1) { return false }
    if compressed && (desc.depth>1 || .Color_Attachment in desc.usage || .Storage in desc.usage || .Present in desc.usage) { return false }
    if !depth && .Depth_Attachment in desc.usage { return false }
    if (.RGBA8_Srgb==desc.format || .BGRA8_Srgb==desc.format) && .Storage in desc.usage { return false }
    if .Present in desc.usage && (desc.layers!=1 || desc.mip_levels!=1 || desc.depth!=1 || depth) { return false }
    return true
}
/// Checks a subresource range without overflowing arithmetic.
image_range_valid :: proc(range:Image_Range,desc:Texture_Desc)->bool {
    return range.mip_count>0 && range.layer_count>0 && range.aspects!={} && range.aspects&texture_aspects(desc.format)==range.aspects && range.base_mip<=desc.mip_levels && range.mip_count<=desc.mip_levels-range.base_mip && range.base_layer<=desc.layers && range.layer_count<=desc.layers-range.base_layer
}
/// Tests overlap independently for mip, layer and aspect dimensions.
image_ranges_overlap :: proc(a,b:Image_Range)->bool {
    return a.aspects&b.aspects!={} && range_overlaps({u64(a.base_mip),u64(a.mip_count)},{u64(b.base_mip),u64(b.mip_count)}) && range_overlaps({u64(a.base_layer),u64(a.layer_count)},{u64(b.base_layer),u64(b.layer_count)})
}
/// Names every subresource in one texture.
image_full_range :: proc(desc:Texture_Desc)->Image_Range { return {0,desc.mip_levels,0,desc.layers,texture_aspects(desc.format)} }
/// Resolves the physical extent of one mip.
texture_mip_extent :: proc(desc:Texture_Desc,mip:u32)->(u32,u32) { return max(u32(1),desc.width>>mip),max(u32(1),desc.height>>mip) }

/// Buffer-copy texel width is specific to the selected depth/stencil aspect.
image_region_pixel_size :: proc(format:Texture_Format,aspect:Image_Aspect)->u32 {
    if aspect==.Stencil { return 1 }
    if aspect==.Depth { return 4 }
    return texture_pixel_size(format)
}

/// Resolves every spatial dimension of a physical mip.
texture_mip_volume :: proc(desc:Texture_Desc,mip:u32)->(u32,u32,u32) { w,h:=texture_mip_extent(desc,mip); return w,h,max(u32(1),desc.depth>>mip) }
