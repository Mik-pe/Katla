//! Actual selected SPIR-V stages determine every published graphics descriptor requirement.
package katla_vulkan

import gfx ".."
import spirv "../spirv"

@(private="package")
reflect_graphics_stage :: proc(r:^Renderer,desc:gfx.Graphics_Desc,pipeline:^Native_Graphics_Pipeline,reflection:spirv.Stage_Reflection,stage:gfx.Shader_Stage)->gfx.Gpu_Error {
    expected:int
    for binding in desc.buffers { if stage in binding.stages { expected+=1 } }
    for binding in desc.images { if stage in binding.stages { expected+=1 } }
    for binding in desc.samplers { if stage in binding.stages { expected+=1 } }
    if expected!=len(reflection.resources) { return .Invalid_Shader }
    for resource in reflection.resources {
        mode:=shader_access(resource.access)
        if resource.array_count==0 || (resource.kind!=.Image && resource.array_count!=1) { return .Unsupported }
        matched:=false
        switch resource.kind {
        case .Buffer:
            for binding in desc.buffers {
                if binding.group!=resource.group || binding.slot!=resource.binding || stage not_in binding.stages { continue }
                usage:=gfx.Buffer_Usage.Storage if resource.storage else gfx.Buffer_Usage.Uniform
                if binding.usage!=usage || !gfx.access_covers(binding.mode,mode) || binding.minimum_size<resource.minimum_size { return .Invalid_Shader }
                if (resource.access==.Write || resource.access==.Read_Write) && ((stage==.Vertex && !r.vertex_stores) || (stage==.Fragment && !r.fragment_stores)) { return .Unsupported }
                requirement:=gfx.Stage_Buffer_Requirement{binding.group,binding.slot,{stage},usage,resource.minimum_size,max(u64(1),u64(r.limits.minStorageBufferOffsetAlignment) if resource.storage else u64(r.limits.minUniformBufferOffsetAlignment)),u64(r.limits.maxStorageBufferRange) if resource.storage else u64(r.limits.maxUniformBufferRange),mode}
                found:=false
                for &prior in pipeline.buffers {
                    if prior.group==requirement.group && prior.slot==requirement.slot {
                        if prior.usage!=requirement.usage || prior.minimum_size!=requirement.minimum_size { return .Invalid_Shader }
                        prior.stages|=requirement.stages; prior.mode=merge_access(prior.mode,requirement.mode); found=true; break
                    }
                }
                if !found {
                    count:=len(pipeline.buffers)
                    resized:=make([]gfx.Stage_Buffer_Requirement,count+1,r.allocator); copy(resized,pipeline.buffers); resized[count]=requirement
                    delete(pipeline.buffers,r.allocator); pipeline.buffers=resized
                }
                matched=true; break
            }
        case .Image:
            if (resource.dimension!=.D2 && resource.dimension!=.D3) || resource.multisampled { return .Unsupported }
            dimension:=gfx.Texture_Dimension.D3 if resource.dimension==.D3 else gfx.Texture_Dimension.D2
            sample_type:gfx.Texture_Sample_Type
            switch resource.sample_type {
            case .Float: sample_type=.Float
            case .Sint: sample_type=.Sint
            case .Uint: sample_type=.Uint
            case .None: return .Invalid_Shader
            }
            storage_format:gfx.Texture_Format
            if resource.storage {
                switch resource.format {
                case 2: storage_format=.RGBA16_Float
                case 3: storage_format=.R32_Float
                case 13: storage_format=.RG8_Unorm
                case 15: storage_format=.R8_Unorm
                case 4: storage_format=.RGBA8_Unorm
                case 10: storage_format=.RGBA16_Unorm
                case 33: storage_format=.R32_Uint
                case: return .Unsupported
                }
            }
            for binding in desc.images {
                if binding.group!=resource.group || binding.slot!=resource.binding || stage not_in binding.stages { continue }
                usage:=gfx.Texture_Usage.Storage if resource.storage else gfx.Texture_Usage.Sampled
                if binding.array_count!=resource.array_count || !gfx.access_covers(binding.mode,mode) || binding.usage!=usage || binding.dimension!=dimension || binding.arrayed!=resource.arrayed || binding.depth!=resource.depth || binding.sample_type!=sample_type || (resource.storage && binding.storage_format!=storage_format) { return .Invalid_Shader }
                if resource.storage && (resource.access==.Write || resource.access==.Read_Write) && ((stage==.Vertex && !r.vertex_stores) || (stage==.Fragment && !r.fragment_stores)) { return .Unsupported }
                requirement:=gfx.Image_Binding_Requirement{binding.group,binding.slot,{stage},usage,resource.arrayed,resource.depth,dimension,sample_type,storage_format,mode,resource.array_count}
                found:=false
                for &prior in pipeline.images {
                    if prior.group==requirement.group && prior.slot==requirement.slot {
                        if prior.array_count!=requirement.array_count || prior.usage!=requirement.usage || prior.dimension!=requirement.dimension || prior.arrayed!=requirement.arrayed || prior.depth!=requirement.depth || prior.sample_type!=requirement.sample_type || prior.storage_format!=requirement.storage_format { return .Invalid_Shader }
                        prior.stages|=requirement.stages; prior.mode=merge_access(prior.mode,requirement.mode); found=true; break
                    }
                }
                if !found {
                    count:=len(pipeline.images)
                    resized:=make([]gfx.Image_Binding_Requirement,count+1,r.allocator); copy(resized,pipeline.images); resized[count]=requirement
                    delete(pipeline.images,r.allocator); pipeline.images=resized
                }
                matched=true; break
            }
        case .Sampler:
            for binding in desc.samplers {
                if binding.group!=resource.group || binding.slot!=resource.binding || stage not_in binding.stages { continue }
                if binding.comparison!=resource.comparison { return .Invalid_Shader }
                requirement:=gfx.Sampler_Requirement{binding.group,binding.slot,{stage},binding.comparison}
                found:=false
                for &prior in pipeline.samplers {
                    if prior.group==requirement.group && prior.slot==requirement.slot {
                        if prior.comparison!=requirement.comparison { return .Invalid_Shader }
                        prior.stages|=requirement.stages; found=true; break
                    }
                }
                if !found {
                    count:=len(pipeline.samplers)
                    resized:=make([]gfx.Sampler_Requirement,count+1,r.allocator); copy(resized,pipeline.samplers); resized[count]=requirement
                    delete(pipeline.samplers,r.allocator); pipeline.samplers=resized
                }
                matched=true; break
            }
        }
        if !matched { return .Invalid_Shader }
    }
    return .None
}
/// Compiles a pipeline only after selected-stage reflection matches all authored bindings.
create_graphics_pipeline :: proc(r:^Renderer,desc:gfx.Graphics_Desc)->(gfx.Graphics_Pipeline_Handle,gfx.Gpu_Error) {
    if r.device==nil || r.failed { return {},.Native_Failure }
    for pipeline in r.graphics_cache {
        if gfx.graphics_desc_equal(pipeline.key,desc) { pipeline.refs+=1;return gfx.storage_insert(&r.graphics,pipeline),.None }
    }
    if desc.vertex_entry=="" || (len(desc.colors)>0 && desc.fragment_entry=="") || (!desc.depth.enabled && len(desc.colors)==0) { return {},.Invalid_Shader }
    if !gfx.vertex_layout_valid(desc.vertex) { return {},.Invalid_Shader }
    if (desc.wireframe && !r.wireframe) || (desc.depth_bias.clamp!=0 && !r.depth_bias_clamp) { return {},.Unsupported }
    for attribute in desc.vertex.attributes { if attribute.location>=r.limits.maxVertexInputAttributes || attribute.offset>r.limits.maxVertexInputAttributeOffset { return {},.Invalid_Shader } }
    for binding in desc.vertex.buffers { if binding.binding>=r.limits.maxVertexInputBindings || binding.stride>r.limits.maxVertexInputBindingStride { return {},.Invalid_Shader } }
    if desc.stencil.enabled && (!desc.depth.enabled || .Stencil not_in gfx.texture_aspects(desc.depth.format)) { return {},.Invalid_Shader }
    if desc.depth.enabled && .Depth not_in gfx.texture_aspects(desc.depth.format) { return {},.Invalid_Shader }
    for color in desc.colors { if gfx.texture_aspects(color.format)!={.Color} { return {},.Invalid_Shader } }
    for binding in desc.buffers { if binding.stages=={} || .Compute in binding.stages { return {},.Invalid_Shader } }
    for binding in desc.images { if binding.stages=={} || .Compute in binding.stages { return {},.Invalid_Shader } }
    for binding in desc.samplers { if binding.stages=={} || .Compute in binding.stages { return {},.Invalid_Shader } }
    vertex,vertex_error:=spirv.reflect_entry(desc.vertex_spirv,desc.vertex_entry,.Vertex,r.allocator)
    if vertex_error!=.None { return {},.Invalid_Shader }; defer spirv.stage_destroy(&vertex)
    pipeline:=new(Native_Graphics_Pipeline,r.allocator); pipeline.refs=1; pipeline.depth=desc.depth; pipeline.stencil=desc.stencil.enabled
    success:=false; defer { if !success { release_graphics_pipeline(r,pipeline) } }
    error:=reflect_graphics_stage(r,desc,pipeline,vertex,.Vertex)
    if error!=.None { return {},error }
    if desc.fragment_entry!="" {
        fragment,fragment_error:=spirv.reflect_entry(desc.fragment_spirv,desc.fragment_entry,.Fragment,r.allocator)
        if fragment_error!=.None { return {},.Invalid_Shader }; defer spirv.stage_destroy(&fragment)
        for location in fragment.color_outputs { if int(location)>=len(desc.colors) { return {},.Invalid_Shader } }
        error=reflect_graphics_stage(r,desc,pipeline,fragment,.Fragment)
        if error!=.None { return {},error }
    }
    pipeline.vertex.attributes=make([]gfx.Vertex_Attribute,len(desc.vertex.attributes),r.allocator); copy(pipeline.vertex.attributes,desc.vertex.attributes)
    pipeline.vertex.buffers=make([]gfx.Vertex_Layout_Binding,len(desc.vertex.buffers),r.allocator); copy(pipeline.vertex.buffers,desc.vertex.buffers)
    pipeline.colors=make([]gfx.Texture_Format,len(desc.colors),r.allocator)
    for color,i in desc.colors { pipeline.colors[i]=color.format }
    error=graphics_pipeline_allocate(r,desc,pipeline)
    if error!=.None { return {},error }
    pipeline.key=gfx.graphics_desc_clone(desc,r.allocator);append(&r.graphics_cache,pipeline)
    success=true
    return gfx.storage_insert(&r.graphics,pipeline),.None
}

@(private="package")
shader_access :: proc(access:spirv.Access)->gfx.Access_Mode {
    switch access {
    case .None: return .None
    case .Read: return .Read
    case .Write: return .Write
    case .Read_Write: return .Read_Write
    }
    return .None
}
@(private="package")
merge_access :: proc(a,b:gfx.Access_Mode)->gfx.Access_Mode {
    if a==.None { return b }
    if b==.None || a==b { return a }
    return .Read_Write
}
