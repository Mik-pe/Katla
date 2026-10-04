//! Immutable samplers are retained by each accepted descriptor packet.
package katla_vulkan

import gfx ".."
import vk "vendor:vulkan"

@(private="package")
Native_Sampler :: struct { object:vk.Sampler, desc:gfx.Sampler_Desc, refs:int }
@(private="package")
release_sampler :: proc(r:^Renderer,sampler:^Native_Sampler) {
    sampler.refs-=1
    if sampler.refs==0 {
        for cached,index in r.sampler_cache { if cached==sampler { ordered_remove(&r.sampler_cache,index);break } }
        r.table.DestroySampler(r.device,sampler.object,nil); free(sampler,r.allocator)
    }
}
/// Creates explicit filtering, addressing and comparison state without hidden defaults.
create_sampler :: proc(r:^Renderer,desc:gfx.Sampler_Desc)->(gfx.Sampler_Handle,gfx.Gpu_Error) {
    if r.device==nil || r.failed { return {},.Native_Failure }
    normalized,valid:=gfx.sampler_desc_normalize(desc);if !valid { return {},.Invalid_Range }
    for sampler in r.sampler_cache { if sampler.desc==normalized { sampler.refs+=1;return gfx.storage_insert(&r.samplers,sampler),.None } }
    if normalized.max_anisotropy>1 && !r.sampler_anisotropy { return {},.Unsupported }
    info:=vk.SamplerCreateInfo{sType=.SAMPLER_CREATE_INFO,magFilter=vk.Filter(normalized.mag_filter),minFilter=vk.Filter(normalized.min_filter),mipmapMode=.LINEAR if normalized.mip_filter==.Linear else .NEAREST,addressModeU=vk.SamplerAddressMode(normalized.address_u),addressModeV=vk.SamplerAddressMode(normalized.address_v),addressModeW=vk.SamplerAddressMode(normalized.address_w),anisotropyEnable=b32(normalized.max_anisotropy>1),maxAnisotropy=min(f32(normalized.max_anisotropy),r.limits.maxSamplerAnisotropy),compareEnable=b32(normalized.comparison),compareOp=vk.CompareOp(normalized.compare),minLod=normalized.min_lod,maxLod=normalized.max_lod,borderColor=.FLOAT_TRANSPARENT_BLACK}
    sampler:=new(Native_Sampler,r.allocator); sampler.desc=normalized; sampler.refs=1
    if r.table.CreateSampler(r.device,&info,nil,&sampler.object)!=.SUCCESS { free(sampler,r.allocator); return {},.Allocation_Failed }
    append(&r.sampler_cache,sampler)
    return gfx.storage_insert(&r.samplers,sampler),.None
}
/// Removes the registry owner without releasing descriptors still used by accepted work.
destroy_sampler :: proc(r:^Renderer,handle:gfx.Sampler_Handle)->gfx.Gpu_Error {
    sampler,ok:=gfx.storage_remove(&r.samplers,handle)
    if !ok { return .Invalid_Resource }
    release_sampler(r,sampler)
    return .None
}
@(private="package")
query_sampler :: proc(state:rawptr,handle:gfx.Sampler_Handle)->(gfx.Sampler_Info,bool) {
    r:=cast(^Renderer)state
    entry,ok:=gfx.storage_get(&r.samplers,handle)
    if !ok { return {},false }
    return {entry^.desc,entry^},true
}
