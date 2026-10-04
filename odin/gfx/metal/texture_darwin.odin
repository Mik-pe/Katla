#+build darwin, arm64
//! Native texture allocations retain their physical heap and exact submission owners.
package metal

import gfx ".."
import MTL "vendor:darwin/Metal"
import NS "core:sys/darwin/Foundation"

@(private="package")
Native_Heap :: struct { object:^MTL.Heap, refs,pending:int, epoch:u64 }
@(private="package")
Native_Texture :: struct { object:^MTL.Texture, heap:^Native_Heap, desc:gfx.Texture_Desc, refs,pending:int, content_epoch:u64, initialized:[]bool }

@(private="package")
pixel_format :: proc(format:gfx.Texture_Format)->MTL.PixelFormat {
    switch format {
    case .RGBA8_Srgb: return .RGBA8Unorm_sRGB
    case .BGRA8_Srgb: return .BGRA8Unorm_sRGB
    case .R8_Unorm: return .R8Unorm
    case .RG8_Unorm: return .RG8Unorm
    case .R32_Float: return .R32Float
    case .BC1_RGBA_Unorm: return .BC1_RGBA
    case .BC3_RGBA_Unorm: return .BC3_RGBA
    case .RGBA8_Unorm: return .RGBA8Unorm
    case .BGRA8_Unorm: return .BGRA8Unorm
    case .RGBA16_Float: return .RGBA16Float
    case .RGBA16_Unorm: return .RGBA16Unorm
    case .R32_Uint: return .R32Uint
    case .D32_Float: return .Depth32Float
    case .D24_Unorm_S8_Uint: return .Depth24Unorm_Stencil8
    case .D32_Float_S8_Uint: return .Depth32Float_Stencil8
    }
    unreachable()
}

@(private="package")
texture_descriptor :: proc(desc:gfx.Texture_Desc)->^MTL.TextureDescriptor {
    native:=MTL.TextureDescriptor.alloc()->init()
    if native==nil { return nil }
    native->setPixelFormat(pixel_format(desc.format))
    native->setWidth(NS.UInteger(desc.width)); native->setHeight(NS.UInteger(desc.height)); native->setDepth(NS.UInteger(desc.depth))
    native->setMipmapLevelCount(NS.UInteger(desc.mip_levels)); native->setArrayLength(NS.UInteger(desc.layers))
    native->setTextureType(.Type3D if desc.depth>1 else (.Type2DArray if desc.layers>1 else .Type2D))
    native->setStorageMode(.Private); native->setHazardTrackingMode(.Untracked)
    usage:MTL.TextureUsage
    if .Sampled in desc.usage { usage|={.ShaderRead} }
    if .Storage in desc.usage { usage|={.ShaderWrite} }
    if .Color_Attachment in desc.usage || .Depth_Attachment in desc.usage { usage|={.RenderTarget} }
    usage|={.PixelFormatView}
    native->setUsage(usage)
    return native
}

@(private="package")
release_heap :: proc(r:^Renderer,heap:^Native_Heap) {
    heap.refs-=1
    if heap.refs==0 { heap.object->release(); free(heap,r.allocator) }
}

@(private="package")
release_texture :: proc(r:^Renderer,texture:^Native_Texture) {
    texture.refs-=1
    if texture.refs==0 {
        texture.object->release()
        if texture.heap!=nil { release_heap(r,texture.heap) }
        delete(texture.initialized,r.allocator); free(texture,r.allocator)
    }
}

/// Creates one private, explicitly synchronized texture without hidden initialized contents.
create_texture :: proc(r:^Renderer,desc:gfx.Texture_Desc)->(gfx.Texture_Handle,gfx.Gpu_Error) {
    if r.device==nil || r.failed { return {},.Native_Failure }
    if !texture_supported(r,desc) { return {},.Unsupported }
    if !gfx.texture_desc_valid(desc) || desc.width>16384 || desc.height>16384 || desc.layers>2048 || desc.depth>2048 || (desc.depth>1 && (desc.width>2048 || desc.height>2048)) || .Present in desc.usage { return {},.Invalid_Range }
    native:=texture_descriptor(desc); if native==nil { return {},.Allocation_Failed }; defer native->release()
    object:=r.device->newTextureWithDescriptor(native)
    if object==nil { return {},.Allocation_Failed }
    texture:=new(Native_Texture,r.allocator)
    texture^={object=object,desc=desc,refs=1}
    new_texture_content(r,texture)
    return gfx.storage_insert(&r.textures,texture),.None
}

/// Removes CPU identity immediately while submissions/readback tickets retain native storage.
destroy_texture :: proc(r:^Renderer,handle:gfx.Texture_Handle)->gfx.Gpu_Error {
    texture,ok:=gfx.storage_remove(&r.textures,handle); if !ok { return .Invalid_Resource }
    release_texture(r,texture)
    return .None
}

@(private="package")
query_texture :: proc(state:rawptr,handle:gfx.Texture_Handle)->(gfx.Texture_Info,bool) {
    r:=cast(^Renderer)state; texture,ok:=gfx.storage_get(&r.textures,handle)
    if !ok { return {},false }
    identity:=rawptr(texture^.object)
    if texture^.heap!=nil { identity=texture^.heap.object }
    return {texture^.desc,identity},true
}


@(private="package")
texture_supported :: proc(r:^Renderer,desc:gfx.Texture_Desc)->bool {
    if desc.format==.D24_Unorm_S8_Uint || (desc.depth>1 && .Color_Attachment in desc.usage) { return false }
    if desc.format==.BC1_RGBA_Unorm || desc.format==.BC3_RGBA_Unorm { return bool(send(NS.BOOL,r.device,"supportsBCTextureCompression")) }
    return true
}

@(private="package")
shader_texture_type :: proc(dimension:gfx.Texture_Dimension,arrayed:bool)->MTL.TextureType {
    return .Type3D if dimension==.D3 else (.Type2DArray if arrayed else .Type2D)
}
