//! Immutable samplers are retained by each accepted descriptor packet.
package katla_vulkan

import gfx ".."
import vk "vendor:vulkan"
import "core:math"

@(private="package")
Native_Sampler :: struct { object:vk.Sampler, desc:gfx.Sampler_Desc, refs:int }
@(private="package")
release_sampler :: proc(r:^Renderer,sampler:^Native_Sampler) {
    sampler.refs-=1
    if sampler.refs==0 { r.table.DestroySampler(r.device,sampler.object,nil); free(sampler,r.allocator) }
}
/// Creates explicit filtering, addressing and comparison state without hidden defaults.
create_sampler :: proc(r:^Renderer,desc:gfx.Sampler_Desc)->(gfx.Sampler_Handle,gfx.Gpu_Error) {
    if r.device==nil || r.failed { return {},.Native_Failure }
    if math.is_nan(desc.min_lod) || math.is_inf(desc.min_lod) || math.is_nan(desc.max_lod) || math.is_inf(desc.max_lod) || desc.min_lod<0 || desc.max_lod<desc.min_lod || desc.max_anisotropy==0 { return {},.Invalid_Range }
    if desc.max_anisotropy>1 && (!r.sampler_anisotropy || f32(desc.max_anisotropy)>r.limits.maxSamplerAnisotropy) { return {},.Unsupported }
    info:=vk.SamplerCreateInfo{sType=.SAMPLER_CREATE_INFO,magFilter=vk.Filter(desc.mag_filter),minFilter=vk.Filter(desc.min_filter),mipmapMode=vk.SamplerMipmapMode(desc.mip_filter),addressModeU=vk.SamplerAddressMode(desc.address_u),addressModeV=vk.SamplerAddressMode(desc.address_v),addressModeW=vk.SamplerAddressMode(desc.address_w),anisotropyEnable=b32(desc.max_anisotropy>1),maxAnisotropy=f32(desc.max_anisotropy),compareEnable=b32(desc.comparison),compareOp=vk.CompareOp(desc.compare),minLod=desc.min_lod,maxLod=desc.max_lod,borderColor=.FLOAT_TRANSPARENT_BLACK}
    sampler:=new(Native_Sampler,r.allocator); sampler.desc=desc; sampler.refs=1
    if r.table.CreateSampler(r.device,&info,nil,&sampler.object)!=.SUCCESS { free(sampler,r.allocator); return {},.Allocation_Failed }
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
