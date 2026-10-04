#+build darwin, arm64
//! Selected compute bindings preserve compiler mapping, native reflection and runtime-array bounds.
package metal

import gfx ".."
import MTL "vendor:darwin/Metal"
import NS "core:sys/darwin/Foundation"
import "core:mem"

@(private="package")
reflect_compute :: proc(r:^Renderer,pipeline:^Native_Pipeline,reflected:^NS.Array)->gfx.Gpu_Error {
    used_buffers,used_images,used_samplers:int
    found_sizes:=false
    for i in 0..<int(reflected->count()) {
        binding:=reflected->objectAs(NS.UInteger(i),^MTL.Binding)
        active:=bool(binding->isUsed())
        index:=i32(binding->index())
        if binding->type()==.Buffer && pipeline.desc.runtime_sizes_words>0 && index==pipeline.desc.runtime_sizes_index {
            if found_sizes || binding->access()!=.ReadOnly { return .Invalid_Shader }
            native:=cast(^MTL.BufferBinding)binding
            if u64(native->bufferDataSize())>u64(pipeline.desc.runtime_sizes_words)*4 { return .Invalid_Shader }
            found_sizes=true; continue
        }
        found:=false
        #partial switch binding->type() {
        case .Buffer:
            native:=cast(^MTL.BufferBinding)binding
            for requested in pipeline.desc.images {
                if requested.metal_kind!=.Argument_Buffer || requested.metal_index!=index { continue }
                if found || !reflect_image_array(native,requested.array_count,requested.dimension,requested.arrayed,requested.depth,requested.sample_type,requested.mode) { return .Invalid_Shader }
                found=true; used_images+=1
            }
            for requested,j in pipeline.desc.buffers {
                if requested.metal_index!=index { continue }
                if !binding_access_valid(binding->access(),requested.mode) || found || (requested.usage==.Uniform && binding->access()!=.ReadOnly) { return .Invalid_Shader }
                pipeline.requirements[j]={group=requested.group,slot=requested.slot,usage=requested.usage,mode=requested.mode,minimum_size=max(u64(native->bufferDataSize()),requested.minimum_size,1),alignment=max(u64(native->bufferAlignment()),1),maximum_size=u64(send(NS.UInteger,r.device,"maxBufferLength"))}
                found=true; used_buffers+=1
            }
        case .Texture:
            native:=cast(^MTL.TextureBinding)binding
            for requested in pipeline.desc.images {
                if requested.metal_kind!=.Texture || requested.metal_index!=index { continue }
                if !binding_access_valid(binding->access(),requested.mode) || found || (requested.usage==.Sampled && binding->access()!=.ReadOnly) { return .Invalid_Shader }
                expected_type:=shader_texture_type(requested.dimension,requested.arrayed)
                expected_data:=MTL.DataType.Float
                if requested.sample_type==.Sint { expected_data=.Int }
                if requested.sample_type==.Uint { expected_data=.UInt }
                if native->textureType()!=expected_type || bool(native->isDepthTexture())!=requested.depth || native->textureDataType()!=expected_data || native->arrayLength()>1 { return .Invalid_Shader }
                found=true; used_images+=1
            }
        case .Sampler:
            for requested in pipeline.desc.samplers {
                if requested.metal_index!=index { continue }
                if found { return .Invalid_Shader }
                found=true; used_samplers+=1
            }
        case: if active { return .Unsupported }; continue
        }
        if !found && active { return .Invalid_Shader }
    }
    if used_buffers!=len(pipeline.desc.buffers) || used_images!=len(pipeline.desc.images) || used_samplers!=len(pipeline.desc.samplers) || found_sizes!=(pipeline.desc.runtime_sizes_words>0) { return .Invalid_Shader }
    return .None
}

/// Creates and verifies native compute state before encoding, without a frame-path compiler fallback.
create_pipeline :: proc(r:^Renderer,desc:gfx.Compute_Desc)->(gfx.Pipeline_Handle,gfx.Gpu_Error) {
    if r.compiler==nil || r.failed { return {},.Native_Failure }
    if len(desc.entry)==0 || len(desc.metal_entry)==0 || len(desc.metal_source)==0 || len(desc.buffers)>31 || len(desc.images)>128 || len(desc.samplers)>16 { return {},.Invalid_Shader }
    if (desc.runtime_sizes_words==0)!=(desc.runtime_sizes_index<0) || desc.runtime_sizes_index>=31 || desc.runtime_sizes_words>4096 { return {},.Invalid_Shader }
    threads:u64=1
    limits:=r.device->maxThreadsPerThreadgroup()
    dimensions:=[3]NS.Integer{limits.width,limits.height,limits.depth}
    for count,i in desc.local_size { if count==0 || NS.Integer(count)>dimensions[i] || u64(count)>1024/threads { return {},.Invalid_Shader }; threads*=u64(count) }
    for buffer,i in desc.buffers {
        if buffer.metal_index<0 || buffer.metal_index>=31 || (buffer.usage!=.Storage && buffer.usage!=.Uniform) || (buffer.size_index>=0 && buffer.size_index>=i32(desc.runtime_sizes_words)) || buffer.metal_index==desc.runtime_sizes_index { return {},.Invalid_Shader }
        for previous in desc.buffers[:i] { if previous.metal_index==buffer.metal_index || (previous.group==buffer.group && previous.slot==buffer.slot) { return {},.Invalid_Shader } }
    }
    for image,i in desc.images {
        if image.array_count==0 || image.array_count>4096 || (image.metal_kind==.Texture && image.array_count!=1) || image.metal_index<0 || image.metal_index>=128 || (image.usage!=.Sampled && image.usage!=.Storage) { return {},.Invalid_Shader }
        if image.metal_kind==.Argument_Buffer {
            if r.device->argumentBuffersSupport()!=.Tier2 { return {},.Unsupported }
            if image.metal_index>=31 || image.metal_index==desc.runtime_sizes_index { return {},.Invalid_Shader }
            for buffer in desc.buffers { if buffer.metal_index==image.metal_index { return {},.Invalid_Shader } }
        }
        for previous in desc.images[:i] { if ((previous.metal_kind==.Argument_Buffer)==(image.metal_kind==.Argument_Buffer) && previous.metal_index==image.metal_index) || (previous.group==image.group && previous.slot==image.slot) { return {},.Invalid_Shader } }
        for buffer in desc.buffers { if buffer.group==image.group && buffer.slot==image.slot { return {},.Invalid_Shader } }
    }
    for sampler,i in desc.samplers {
        if sampler.metal_index<0 || sampler.metal_index>=16 { return {},.Invalid_Shader }
        for previous in desc.samplers[:i] { if previous.metal_index==sampler.metal_index || (previous.group==sampler.group && previous.slot==sampler.slot) { return {},.Invalid_Shader } }
        for buffer in desc.buffers { if buffer.group==sampler.group && buffer.slot==sampler.slot { return {},.Invalid_Shader } }
        for image in desc.images { if image.group==sampler.group && image.slot==sampler.slot { return {},.Invalid_Shader } }
    }
    function,err:=compile_function(r,desc.metal_source,desc.metal_entry,.Kernel)
    if err!=.None { return {},err }; defer function->release()
    descriptor:=new_object("MTL4ComputePipelineDescriptor")
    if descriptor==nil { return {},.Allocation_Failed }; defer descriptor->release()
    options:=new_object("MTL4PipelineOptions")
    if options==nil { return {},.Allocation_Failed }; defer options->release()
    send(nil,options,"setShaderReflection:",NS.UInteger(3)); send(nil,descriptor,"setOptions:",options)
    send(nil,descriptor,"setComputeFunctionDescriptor:",function)
    native_error:^NS.Error
    object:=send(^NS.Object,r.compiler,"newComputePipelineStateWithDescriptor:compilerTaskOptions:error:",descriptor,cast(^NS.Object)nil,&native_error)
    if object==nil { report_error(native_error,"Metal compute pipeline compilation failed"); return {},.Shader_Compile_Failed }
    pipeline:=new(Native_Pipeline,r.allocator); pipeline^={object=object,desc=desc,local_size=desc.local_size,refs=1}
    pipeline.desc.buffers=clone_slice(desc.buffers,r); pipeline.desc.images=clone_slice(desc.images,r); pipeline.desc.samplers=clone_slice(desc.samplers,r)
    pipeline.desc.entry=""; pipeline.desc.metal_entry=""; pipeline.desc.metal_source=""; pipeline.desc.spirv=nil
    success:=false; defer { if !success { release_pipeline(r,pipeline) } }
    pipeline.requirements=make([]gfx.Binding_Requirement,len(desc.buffers),r.allocator)
    pipeline.images=make([]gfx.Image_Binding_Requirement,len(desc.images),r.allocator)
    for image,i in desc.images { pipeline.images[i]={group=image.group,slot=image.slot,stages={.Compute},usage=image.usage,arrayed=image.arrayed,depth=image.depth,dimension=image.dimension,sample_type=image.sample_type,storage_format=image.storage_format,mode=image.mode,array_count=image.array_count} }
    pipeline.samplers=make([]gfx.Sampler_Requirement,len(desc.samplers),r.allocator)
    for sampler,i in desc.samplers { pipeline.samplers[i]={sampler.group,sampler.slot,{.Compute},sampler.comparison} }
    reflection:=send(^MTL.ComputePipelineReflection,object,"reflection")
    if reflection==nil { return {},.Invalid_Shader }
    err=reflect_compute(r,pipeline,reflection->bindings())
    if err!=.None { return {},err }
    pipeline.max_threads=u64(send(NS.UInteger,object,"maxTotalThreadsPerThreadgroup"))
    if threads>pipeline.max_threads { return {},.Invalid_Shader }
    success=true
    return gfx.storage_insert(&r.pipelines,pipeline),.None
}

@(private="package")
encode_dispatch :: proc(r:^Renderer,slot:^Native_Frame,prepared:^gfx.Prepared_Graph,encoder:^NS.Object,packet:gfx.Dispatch)->gfx.Gpu_Error {
    entry,ok:=gfx.storage_get(&r.pipelines,packet.pipeline); if !ok { return .Invalid_Resource }
    pipeline:=entry^; retain_pipeline(slot,pipeline)
    buffer_count,texture_count,sampler_count:u32
    for binding in pipeline.desc.buffers { buffer_count=max(buffer_count,u32(binding.metal_index)+1) }
    for binding in pipeline.desc.images { if binding.metal_kind==.Argument_Buffer { buffer_count=max(buffer_count,u32(binding.metal_index)+1) } else { texture_count=max(texture_count,u32(binding.metal_index)+1) } }
    for binding in pipeline.desc.samplers { sampler_count=max(sampler_count,u32(binding.metal_index)+1) }
    if pipeline.desc.runtime_sizes_words>0 { buffer_count=max(buffer_count,u32(pipeline.desc.runtime_sizes_index)+1) }
    descriptor:=new_object("MTL4ArgumentTableDescriptor")
    if descriptor==nil { return .Allocation_Failed }; defer descriptor->release()
    send(nil,descriptor,"setMaxBufferBindCount:",NS.UInteger(buffer_count)); send(nil,descriptor,"setMaxTextureBindCount:",NS.UInteger(texture_count)); send(nil,descriptor,"setMaxSamplerStateBindCount:",NS.UInteger(sampler_count))
    native_error:^NS.Error
    table:=send(^NS.Object,r.device,"newArgumentTableWithDescriptor:error:",descriptor,&native_error)
    if table==nil { return .Allocation_Failed }; append(&slot.tables,table)
    sizes:=make([]u32,int(pipeline.desc.runtime_sizes_words),r.allocator); defer delete(sizes,r.allocator)
    for binding in packet.bindings {
        buffer,present:=resolve_buffer(r,prepared,binding.access.resource); if !present { return .Invalid_Resource }
        for requirement in pipeline.desc.buffers {
            if requirement.group!=binding.group || requirement.slot!=binding.slot { continue }
            send(nil,table,"setAddress:atIndex:",buffer.object->gpuAddress()+binding.access.range.offset,NS.UInteger(requirement.metal_index))
            if requirement.size_index>=0 {
                if binding.access.range.size>u64(max(u32)) { return .Invalid_Range }
                sizes[requirement.size_index]=u32(binding.access.range.size)
            }
        }
    }
    for binding in packet.images {
        for requirement in pipeline.desc.images {
            if requirement.group!=binding.group || requirement.slot!=binding.slot { continue }
            err:=encode_image_binding(r,slot,prepared,table,binding,requirement.metal_index,requirement.array_count,requirement.metal_kind,requirement.dimension,requirement.arrayed)
            if err!=.None { return err }
        }
    }
    for binding in packet.samplers {
        entry,present:=gfx.storage_get(&r.samplers,binding.handle); if !present { return .Invalid_Resource }
        sampler:=entry^; retained:=false
        for previous in slot.samplers { if previous==sampler { retained=true; break } }
        if !retained { sampler.refs+=1; append(&slot.samplers,sampler) }
        for requirement in pipeline.desc.samplers { if requirement.group==binding.group && requirement.slot==binding.slot { send(nil,table,"setSamplerState:atIndex:",sampler.object->gpuResourceID(),NS.UInteger(requirement.metal_index)) } }
    }
    if len(sizes)>0 {
        buffer:=r.device->newBufferWithLength(NS.UInteger(len(sizes))*4,MTL.ResourceOptions{.HazardTrackingModeUntracked})
        if buffer==nil { return .Allocation_Failed }
        copy(buffer->contents(),mem.slice_to_bytes(sizes))
        append(&slot.auxiliary,cast(^NS.Object)buffer); send(nil,slot.residency,"addAllocation:",buffer)
        send(nil,table,"setAddress:atIndex:",buffer->gpuAddress(),NS.UInteger(pipeline.desc.runtime_sizes_index))
    }
    send(nil,encoder,"setComputePipelineState:",pipeline.object); send(nil,encoder,"setArgumentTable:",table)
    groups:=MTL.Size{NS.Integer(packet.groups[0]),NS.Integer(packet.groups[1]),NS.Integer(packet.groups[2])}
    local:=MTL.Size{NS.Integer(pipeline.local_size[0]),NS.Integer(pipeline.local_size[1]),NS.Integer(pipeline.local_size[2])}
    if packet.indirect.enabled {
        command,command_ok:=resolve_buffer(r,prepared,packet.indirect.command.resource); if !command_ok { return .Invalid_Resource }
        send(nil,encoder,"dispatchThreadgroupsWithIndirectBuffer:threadsPerThreadgroup:",command.object->gpuAddress()+packet.indirect.command.range.offset,local)
    } else { send(nil,encoder,"dispatchThreadgroups:threadsPerThreadgroup:",groups,local) }
    return .None
}
