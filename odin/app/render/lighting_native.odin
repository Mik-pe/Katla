//! Native light and shadow resources are explicit app-owned allocations per frame slot.
package render

import gfx "../../gfx"
import "core:mem"

Feature_Pipelines :: struct { sky,grid:gfx.Graphics_Pipeline_Handle, cull:gfx.Pipeline_Handle, geometry:[2][5]gfx.Graphics_Pipeline_Handle,model_variants:[5][3]gfx.Graphics_Pipeline_Handle }
Feature_Slot :: struct { frame,points,indices,counts,shadow:gfx.Buffer_Handle, atlas,indicator:gfx.Texture_Handle }
Native_Features :: struct($R:typeid) { renderer:^R,operations:GPU_Ops(R),pipelines,reverse_pipelines:Feature_Pipelines,slots:[]Feature_Slot,shadow_sampler:gfx.Sampler_Handle,allocator:mem.Allocator }
feature_buffer_descs :: proc(width,height:u32)->[5]gfx.Buffer_Desc {
    tiles:=u64((width+15)/16)*u64((height+15)/16)
    return {{size=160,usage={.Uniform},memory=.CPU_Visible},{size=8192,usage={.Storage},memory=.CPU_Visible},{size=tiles*128*4,usage={.Storage,.Transfer_Destination},memory=.GPU_Private},{size=tiles*4,usage={.Storage,.Transfer_Destination},memory=.GPU_Private},{size=352,usage={.Storage},memory=.CPU_Visible}}
}
feature_texture_descs :: proc(width,height,size:u32)->[2]gfx.Texture_Desc { return {{width=size,height=size,depth=1,layers=1,mip_levels=1,format=.D32_Float,usage={.Depth_Attachment,.Sampled,.Transfer_Source}},{width=width,height=height,depth=1,layers=1,mip_levels=1,format=.R8_Unorm,usage={.Color_Attachment,.Sampled,.Transfer_Source}}} }
native_features_slots_create :: proc(owner:^Native_Features($R),count:int,width,height,size:u32)->gfx.Gpu_Error {
    buffers:=feature_buffer_descs(width,height); textures:=feature_texture_descs(width,height,size)
    owner.slots=make([]Feature_Slot,count,owner.allocator)
    for &slot in owner.slots {
        handles:=[5]^gfx.Buffer_Handle{&slot.frame,&slot.points,&slot.indices,&slot.counts,&slot.shadow}
        for desc,i in buffers {
            bytes:=make([]byte,int(desc.size),owner.allocator); defer delete(bytes,owner.allocator)
            error:gfx.Gpu_Error
            handles[i]^,error=owner.operations.create_buffer(owner.renderer,desc,bytes); if error!=.None { return error }
        }
        handles_image:=[2]^gfx.Texture_Handle{&slot.atlas,&slot.indicator}
        for desc,i in textures { error:gfx.Gpu_Error; handles_image[i]^,error=owner.operations.create_texture(owner.renderer,desc); if error!=.None { return error } }
    }
    return .None
}
native_features_slots_destroy :: proc(owner:^Native_Features($R))->gfx.Gpu_Error {
    error:=gfx.Gpu_Error.None
    for slot in owner.slots {
        for handle in ([5]gfx.Buffer_Handle{slot.frame,slot.points,slot.indices,slot.counts,slot.shadow}) { if handle.owner!=nil { e:=owner.operations.destroy_buffer(owner.renderer,handle); if e!=.None { error=e } } }
        for handle in ([2]gfx.Texture_Handle{slot.atlas,slot.indicator}) { if handle.owner!=nil { e:=owner.operations.destroy_texture(owner.renderer,handle); if e!=.None { error=e } } }
    }
    delete(owner.slots,owner.allocator); owner.slots=nil; return error
}
native_features_init :: proc(owner:^Native_Features($R),renderer:^R,ops:GPU_Ops(R),descriptors:Feature_Descriptors,slots:int,width,height,size:u32,allocator:mem.Allocator)->gfx.Gpu_Error {
    if ops.create_compute==nil || ops.destroy_compute==nil || ops.create_sampler==nil || ops.destroy_sampler==nil { return .Unsupported }
    owner^={renderer=renderer,operations=ops,allocator=allocator}
    success:=false; defer { if !success { native_features_destroy(owner) } }
    error:gfx.Gpu_Error
    owner.pipelines.sky,error=ops.create_pipeline(renderer,descriptors.sky); if error!=.None { return error }
    owner.pipelines.grid,error=ops.create_pipeline(renderer,descriptors.grid); if error!=.None { return error }
    owner.pipelines.cull,error=ops.create_compute(renderer,descriptors.cull); if error!=.None { return error }
    for kind in 0..<2 { for effect in 0..<5 { owner.pipelines.geometry[kind][effect],error=ops.create_pipeline(renderer,descriptors.geometry[kind][effect]); if error!=.None { return error } } }
    for effect in 0..<5 { for extra in 0..<3 {
        owner.pipelines.model_variants[effect][extra],error=ops.create_pipeline(renderer,descriptors.model_variants[effect][extra]);if error!=.None { return error }
    } }
    owner.reverse_pipelines.sky=owner.pipelines.sky; owner.reverse_pipelines.cull=owner.pipelines.cull
    owner.reverse_pipelines.grid,error=ops.create_pipeline(renderer,depth_descriptor(descriptors.grid,.Reverse)); if error!=.None { return error }
    for kind in 0..<2 {
        owner.reverse_pipelines.geometry[kind][0]=owner.pipelines.geometry[kind][0]
        for effect in 1..<5 { owner.reverse_pipelines.geometry[kind][effect],error=ops.create_pipeline(renderer,depth_descriptor(descriptors.geometry[kind][effect],.Reverse)); if error!=.None { return error } }
    }
    for effect in 0..<5 { for extra in 0..<3 {
        if effect==0 { owner.reverse_pipelines.model_variants[effect][extra]=owner.pipelines.model_variants[effect][extra] }
        else { owner.reverse_pipelines.model_variants[effect][extra],error=ops.create_pipeline(renderer,depth_descriptor(descriptors.model_variants[effect][extra],.Reverse));if error!=.None { return error } }
    } }
    owner.shadow_sampler,error=ops.create_sampler(renderer,{min_filter=.Linear,mag_filter=.Linear,address_u=.Clamp_Edge,address_v=.Clamp_Edge,address_w=.Clamp_Edge,comparison=true,compare=.Less_Equal,max_anisotropy=1}); if error!=.None { return error }
    error=native_features_slots_create(owner,slots,width,height,size); if error!=.None { return error }
    success=true; return .None
}
native_features_destroy :: proc(owner:^Native_Features($R))->gfx.Gpu_Error {
    if owner.renderer==nil { return .None }
    error:=native_features_slots_destroy(owner)
    for handle in ([2]gfx.Graphics_Pipeline_Handle{owner.pipelines.sky,owner.pipelines.grid}) { if handle.owner!=nil { e:=owner.operations.destroy_pipeline(owner.renderer,handle); if e!=.None { error=e } } }
    for kind in owner.pipelines.geometry { for handle in kind { if handle.owner!=nil { e:=owner.operations.destroy_pipeline(owner.renderer,handle); if e!=.None { error=e } } } }
    if owner.reverse_pipelines.grid.owner!=nil { e:=owner.operations.destroy_pipeline(owner.renderer,owner.reverse_pipelines.grid); if e!=.None { error=e } }
    for kind in owner.reverse_pipelines.geometry { for effect in 1..<5 { handle:=kind[effect]; if handle.owner!=nil { e:=owner.operations.destroy_pipeline(owner.renderer,handle); if e!=.None { error=e } } } }
    for effect in 0..<5 { for extra in 0..<3 {
        handle:=owner.pipelines.model_variants[effect][extra];if handle.owner!=nil { e:=owner.operations.destroy_pipeline(owner.renderer,handle);if e!=.None { error=e } }
        if effect!=0 { reverse:=owner.reverse_pipelines.model_variants[effect][extra];if reverse.owner!=nil { e:=owner.operations.destroy_pipeline(owner.renderer,reverse);if e!=.None { error=e } } }
    } }
    if owner.pipelines.cull.owner!=nil { e:=owner.operations.destroy_compute(owner.renderer,owner.pipelines.cull); if e!=.None { error=e } }
    if owner.shadow_sampler.owner!=nil { e:=owner.operations.destroy_sampler(owner.renderer,owner.shadow_sampler); if e!=.None { error=e } }
    owner^={}; return error
}

/// Selects exact authored sidedness and transform winding for every model coverage pass.
feature_model_pipeline :: proc(pipelines:Feature_Pipelines,effect:int,double_sided,mirrored:bool)->gfx.Graphics_Pipeline_Handle {
    variant:=int(double_sided)+2*int(mirrored)
    return pipelines.geometry[1][effect] if variant==0 else pipelines.model_variants[effect][variant-1]
}
