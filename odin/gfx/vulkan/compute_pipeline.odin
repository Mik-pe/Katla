//! Selected compute reflection owns grouped buffer, texture and sampler descriptor layouts.
package katla_vulkan

import gfx ".."
import spirv "../spirv"
import vk "vendor:vulkan"
import "core:strings"

/// Compiles one selected compute entry with its complete reflected descriptor interface.
create_pipeline :: proc(r:^Renderer,desc:gfx.Compute_Desc)->(gfx.Pipeline_Handle,gfx.Gpu_Error) {
    if r.device==nil || r.failed { return {},.Native_Failure }
    reflection,reflection_error:=spirv.reflect_entry(desc.spirv,desc.entry,.Compute,r.allocator)
    if reflection_error!=.None { return {},.Invalid_Shader }; defer spirv.stage_destroy(&reflection)
    if reflection.local_size!=desc.local_size { return {},.Invalid_Shader }
    threads:u64=1
    for count,i in desc.local_size { if count==0 || count>r.limits.maxComputeWorkGroupSize[i] || u64(count)>u64(r.limits.maxComputeWorkGroupInvocations)/threads { return {},.Invalid_Shader }; threads*=u64(count) }
    interface:=new(Native_Graphics_Pipeline,r.allocator); interface.refs=1
    pipeline:=new(Native_Pipeline,r.allocator); pipeline.interface=interface; pipeline.refs=1; pipeline.local_size=desc.local_size
    success:=false; defer { if !success { release_pipeline(r,pipeline) } }
    native_desc:gfx.Graphics_Desc
    native_desc.buffers=make([]gfx.Shader_Stage_Buffer,len(desc.buffers),r.allocator); defer delete(native_desc.buffers,r.allocator)
    native_desc.images=make([]gfx.Shader_Stage_Image,len(desc.images),r.allocator); defer delete(native_desc.images,r.allocator)
    native_desc.samplers=make([]gfx.Shader_Stage_Sampler,len(desc.samplers),r.allocator); defer delete(native_desc.samplers,r.allocator)
    for binding,i in desc.buffers { native_desc.buffers[i]={group=binding.group,slot=binding.slot,stages={.Compute},usage=binding.usage,mode=binding.mode,minimum_size=binding.minimum_size} }
    for binding,i in desc.images { native_desc.images[i]={group=binding.group,slot=binding.slot,stages={.Compute},usage=binding.usage,arrayed=binding.arrayed,depth=binding.depth,dimension=binding.dimension,sample_type=binding.sample_type,storage_format=binding.storage_format,mode=binding.mode,array_count=binding.array_count} }
    for binding,i in desc.samplers { native_desc.samplers[i]={group=binding.group,slot=binding.slot,stages={.Compute},comparison=binding.comparison} }
    error:=reflect_graphics_stage(r,native_desc,interface,reflection,.Compute)
    if error!=.None { return {},error }
    error=graphics_descriptors_allocate(r,native_desc,interface)
    if error!=.None { return {},error }
    pipeline.requirements=make([]gfx.Binding_Requirement,len(interface.buffers),r.allocator)
    for binding,i in interface.buffers { pipeline.requirements[i]={binding.group,binding.slot,binding.usage,binding.minimum_size,binding.alignment,binding.maximum_size,binding.mode} }
    module,module_error:=shader_module(r,desc.spirv)
    if module_error!=.None { return {},module_error }; defer r.table.DestroyShaderModule(r.device,module,nil)
    name:=strings.clone_to_cstring(desc.entry,r.allocator); defer delete(name,r.allocator)
    info:=vk.ComputePipelineCreateInfo{sType=.COMPUTE_PIPELINE_CREATE_INFO,stage={sType=.PIPELINE_SHADER_STAGE_CREATE_INFO,stage={.COMPUTE},module=module,pName=name},layout=interface.layout,basePipelineIndex=-1}
    if r.table.CreateComputePipelines(r.device,0,1,&info,nil,&pipeline.object)!=.SUCCESS { return {},.Shader_Compile_Failed }
    success=true
    return gfx.storage_insert(&r.pipelines,pipeline),.None
}
