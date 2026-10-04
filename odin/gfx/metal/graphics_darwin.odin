#+build darwin, arm64
//! Prepared Metal 4 graphics state validates its actual native stage reflection.
package metal

import gfx ".."
import MTL "vendor:darwin/Metal"
import NS "core:sys/darwin/Foundation"
import "core:math"

@(private="package")
Native_Sampler :: struct { object:^MTL.SamplerState, desc:gfx.Sampler_Desc, refs:int }
@(private="package")
Native_Graphics :: struct {
    object:^NS.Object,
    depth:^MTL.DepthStencilState,
    desc:gfx.Graphics_Desc,
    buffers:[]gfx.Stage_Buffer_Requirement,
    images:[]gfx.Image_Binding_Requirement,
    samplers:[]gfx.Sampler_Requirement,
    colors:[]gfx.Texture_Format,
    refs:int,
}

@(private="package")
compare_op :: proc(op:gfx.Compare_Op)->MTL.CompareFunction {
    switch op {
    case .Never: return .Never
    case .Less: return .Less
    case .Equal: return .Equal
    case .Less_Equal: return .LessEqual
    case .Greater: return .Greater
    case .Not_Equal: return .NotEqual
    case .Greater_Equal: return .GreaterEqual
    case .Always: return .Always
    }
    unreachable()
}

@(private="package")
address_mode :: proc(mode:gfx.Address_Mode)->MTL.SamplerAddressMode {
    switch mode {
    case .Repeat: return .Repeat
    case .Mirror_Repeat: return .MirrorRepeat
    case .Clamp_Edge: return .ClampToEdge
    case .Clamp_Border: return .ClampToBorderColor
    }
    unreachable()
}

/// Creates an immutable sampler with explicit filtering, addressing and comparison state.
create_sampler :: proc(r:^Renderer,desc:gfx.Sampler_Desc)->(gfx.Sampler_Handle,gfx.Gpu_Error) {
    if r.device==nil || r.failed { return {},.Native_Failure }
    if math.is_nan(desc.min_lod) || math.is_inf(desc.min_lod) || math.is_nan(desc.max_lod) || math.is_inf(desc.max_lod) || desc.min_lod<0 || desc.max_lod<desc.min_lod || desc.max_anisotropy==0 || desc.max_anisotropy>16 { return {},.Invalid_Range }
    native:=MTL.SamplerDescriptor.alloc()->init()
    if native==nil { return {},.Allocation_Failed }; defer native->release()
    native->setMinFilter(.Nearest if desc.min_filter==.Nearest else .Linear)
    native->setMagFilter(.Nearest if desc.mag_filter==.Nearest else .Linear)
    native->setMipFilter(.Nearest if desc.mip_filter==.Nearest else .Linear)
    native->setSAddressMode(address_mode(desc.address_u)); native->setTAddressMode(address_mode(desc.address_v)); native->setRAddressMode(address_mode(desc.address_w))
    native->setLodMinClamp(desc.min_lod); native->setLodMaxClamp(desc.max_lod)
    native->setMaxAnisotropy(NS.UInteger(desc.max_anisotropy))
    if desc.comparison { native->setCompareFunction(compare_op(desc.compare)) }
    object:=send(^MTL.SamplerState,r.device,"newSamplerStateWithDescriptor:",native)
    if object==nil { return {},.Allocation_Failed }
    sampler:=new(Native_Sampler,r.allocator); sampler^={object,desc,1}
    return gfx.storage_insert(&r.samplers,sampler),.None
}

@(private="package")
release_sampler :: proc(r:^Renderer,sampler:^Native_Sampler) {
    sampler.refs-=1
    if sampler.refs==0 { sampler.object->release(); free(sampler,r.allocator) }
}

/// Accepted submissions retain sampler identity even after the CPU registry removes it.
destroy_sampler :: proc(r:^Renderer,handle:gfx.Sampler_Handle)->gfx.Gpu_Error {
    sampler,ok:=gfx.storage_remove(&r.samplers,handle); if !ok { return .Invalid_Resource }
    release_sampler(r,sampler); return .None
}

@(private="package")
compile_function :: proc(r:^Renderer,source,entry:string,stage:MTL.FunctionType)->(^NS.Object,gfx.Gpu_Error) {
    if len(source)==0 || len(entry)==0 { return nil,.Invalid_Shader }
    descriptor:=new_object("MTL4LibraryDescriptor")
    if descriptor==nil { return nil,.Allocation_Failed }; defer descriptor->release()
    text:=NS.String.alloc()->initWithOdinString(source)
    if text==nil { return nil,.Allocation_Failed }; defer text->release()
    send(nil,descriptor,"setSource:",text)
    native_error:^NS.Error
    library:=send(^NS.Object,r.compiler,"newLibraryWithDescriptor:error:",descriptor,&native_error)
    if library==nil { report_error(native_error,"Metal graphics shader compilation failed"); return nil,.Shader_Compile_Failed }; defer library->release()
    function:=new_object("MTL4LibraryFunctionDescriptor")
    if function==nil { return nil,.Allocation_Failed }
    name:=NS.String.alloc()->initWithOdinString(entry)
    if name==nil { function->release(); return nil,.Allocation_Failed }; defer name->release()
    selected:=send(^MTL.Function,library,"newFunctionWithName:",name)
    if selected==nil { function->release(); return nil,.Invalid_Shader }; defer selected->release()
    if selected->functionType()!=stage { function->release(); return nil,.Invalid_Shader }
    send(nil,function,"setLibrary:",library); send(nil,function,"setName:",name)
    return function,.None
}

@(private="package")
blend_factor :: proc(factor:gfx.Blend_Factor)->MTL.BlendFactor {
    switch factor {
    case .Zero: return .Zero
    case .One: return .One
    case .Source_Color: return .SourceColor
    case .One_Minus_Source_Color: return .OneMinusSourceColor
    case .Destination_Color: return .DestinationColor
    case .One_Minus_Destination_Color: return .OneMinusDestinationColor
    case .Source_Alpha: return .SourceAlpha
    case .One_Minus_Source_Alpha: return .OneMinusSourceAlpha
    case .Destination_Alpha: return .DestinationAlpha
    case .One_Minus_Destination_Alpha: return .OneMinusDestinationAlpha
    }
    unreachable()
}

@(private="package")
blend_op :: proc(op:gfx.Blend_Op)->MTL.BlendOperation {
    switch op {
    case .Add: return .Add
    case .Subtract: return .Subtract
    case .Reverse_Subtract: return .ReverseSubtract
    case .Min: return .Min
    case .Max: return .Max
    }
    unreachable()
}

@(private="package")
clone_slice :: proc(values:[]$T,r:^Renderer)->[]T { result:=make([]T,len(values),r.allocator); copy(result,values); return result }

@(private="package")
release_graphics :: proc(r:^Renderer,pipeline:^Native_Graphics) {
    pipeline.refs-=1
    if pipeline.refs!=0 { return }
    if pipeline.object!=nil { pipeline.object->release() }
    if pipeline.depth!=nil { pipeline.depth->release() }
    delete(pipeline.buffers,r.allocator); delete(pipeline.images,r.allocator); delete(pipeline.samplers,r.allocator); delete(pipeline.colors,r.allocator)
    delete(pipeline.desc.vertex.attributes,r.allocator); delete(pipeline.desc.vertex.buffers,r.allocator)
    delete(pipeline.desc.buffers,r.allocator); delete(pipeline.desc.images,r.allocator); delete(pipeline.desc.samplers,r.allocator); delete(pipeline.desc.colors,r.allocator)
    free(pipeline,r.allocator)
}

@(private="package")
binding_access_valid :: proc(native:MTL.ArgumentAccess,mode:gfx.Access_Mode)->bool {
    return !(gfx.access_reads(mode) && native==.WriteOnly) && !(gfx.access_writes(mode) && native==.ReadOnly)
}

@(private="package")
reflect_stage :: proc(r:^Renderer,pipeline:^Native_Graphics,native:^NS.Array,stage:gfx.Shader_Stage)->gfx.Gpu_Error {
    if native==nil { return .Invalid_Shader }
    vertex:=stage==.Vertex
    sizes_index:=pipeline.desc.vertex_sizes_index if vertex else pipeline.desc.fragment_sizes_index
    sizes_words:=pipeline.desc.vertex_sizes_words if vertex else pipeline.desc.fragment_sizes_words
    used_buffers,used_images,used_samplers:int
    found_sizes:=false
    for i in 0..<int(native->count()) {
        binding:=native->objectAs(NS.UInteger(i),^MTL.Binding)
        active:=bool(binding->isUsed())
        index:=i32(binding->index())
        if stage==.Vertex && binding->type()==.Buffer {
            vertex_buffer:=false
            for buffer in pipeline.desc.vertex.buffers { if index==i32(10+buffer.binding) { vertex_buffer=true; break } }
            if vertex_buffer { continue }
        }
        if binding->type()==.Buffer && sizes_words>0 && index==sizes_index {
            if found_sizes || binding->access()!=.ReadOnly { return .Invalid_Shader }
            buffer:=cast(^MTL.BufferBinding)binding
            if u64(buffer->bufferDataSize())>u64(sizes_words)*4 { return .Invalid_Shader }
            found_sizes=true; continue
        }
        matched:=false
        #partial switch binding->type() {
        case .Buffer:
            native_buffer:=cast(^MTL.BufferBinding)binding
            for requested in pipeline.desc.images {
                if requested.metal_kind!=.Argument_Buffer || !(stage in requested.stages) || index!=(requested.vertex_index if vertex else requested.fragment_index) { continue }
                if matched || !reflect_image_array(native_buffer,requested.array_count,requested.dimension,requested.arrayed,requested.depth,requested.sample_type,requested.mode) { return .Invalid_Shader }
                matched=true; used_images+=1
            }
            for requested,j in pipeline.desc.buffers {
                if !(stage in requested.stages) || index!=(requested.vertex_index if vertex else requested.fragment_index) { continue }
                if !binding_access_valid(binding->access(),requested.mode) || matched || (requested.usage!=.Storage && requested.usage!=.Uniform) || (requested.usage==.Uniform && binding->access()!=.ReadOnly) { return .Invalid_Shader }
                requirement:=&pipeline.buffers[j]
                requirement.minimum_size=max(requirement.minimum_size,max(u64(native_buffer->bufferDataSize()),requested.minimum_size,1))
                requirement.alignment=max(requirement.alignment,max(u64(native_buffer->bufferAlignment()),1))
                requirement.maximum_size=u64(send(NS.UInteger,r.device,"maxBufferLength"))
                matched=true; used_buffers+=1
            }
        case .Texture:
            for requested in pipeline.desc.images {
                if requested.metal_kind!=.Texture || !(stage in requested.stages) || index!=(requested.vertex_index if vertex else requested.fragment_index) { continue }
                if !binding_access_valid(binding->access(),requested.mode) || matched || (requested.usage!=.Sampled && requested.usage!=.Storage) || (requested.usage==.Sampled && binding->access()!=.ReadOnly) { return .Invalid_Shader }
                texture:=cast(^MTL.TextureBinding)binding
                expected_type:=shader_texture_type(requested.dimension,requested.arrayed)
                expected_data:=MTL.DataType.Float
                if requested.sample_type==.Sint { expected_data=.Int }
                if requested.sample_type==.Uint { expected_data=.UInt }
                if texture->textureType()!=expected_type || bool(texture->isDepthTexture())!=requested.depth || texture->textureDataType()!=expected_data || texture->arrayLength()>1 { return .Invalid_Shader }
                matched=true; used_images+=1
            }
        case .Sampler:
            for requested in pipeline.desc.samplers {
                if !(stage in requested.stages) || index!=(requested.vertex_index if vertex else requested.fragment_index) { continue }
                if matched { return .Invalid_Shader }
                matched=true; used_samplers+=1
            }
        case: if active { return .Unsupported }; continue
        }
        if !matched && active { return .Invalid_Shader }
    }
    expected_buffers,expected_images,expected_samplers:int
    for requested in pipeline.desc.buffers { if stage in requested.stages { expected_buffers+=1 } }
    for requested in pipeline.desc.images { if stage in requested.stages { expected_images+=1 } }
    for requested in pipeline.desc.samplers { if stage in requested.stages { expected_samplers+=1 } }
    if used_buffers!=expected_buffers || used_images!=expected_images || used_samplers!=expected_samplers || found_sizes!=(sizes_words>0) { return .Invalid_Shader }
    return .None
}

/// Creates complete immutable graphics state before any frame can reference it.
create_graphics_pipeline :: proc(r:^Renderer,desc:gfx.Graphics_Desc)->(gfx.Graphics_Pipeline_Handle,gfx.Gpu_Error) {
    if r.compiler==nil || r.failed { return {},.Native_Failure }
    if desc.depth.enabled && desc.depth.format==.D24_Unorm_S8_Uint { return {},.Unsupported }
    if !graphics_native_bindings_valid(desc) { return {},.Invalid_Shader }
    for image in desc.images { if image.metal_kind==.Argument_Buffer && r.device->argumentBuffersSupport()!=.Tier2 { return {},.Unsupported } }
    if !gfx.graphics_desc_valid(desc) || !gfx.vertex_layout_valid(desc.vertex) || (desc.stencil.enabled && (!desc.depth.enabled || !(.Stencil in gfx.texture_aspects(desc.depth.format)))) { return {},.Invalid_Shader }
    biases:=[3]f32{desc.depth_bias.constant,desc.depth_bias.slope,desc.depth_bias.clamp}
    for value in biases { if math.is_nan(value) || math.is_inf(value) { return {},.Invalid_Shader } }
    if len(desc.colors)>8 || (len(desc.colors)==0 && !desc.depth.enabled) || (len(desc.colors)>0 && len(desc.fragment_entry)==0) { return {},.Invalid_Shader }
    if desc.depth.enabled && gfx.texture_aspects(desc.depth.format)=={.Color} { return {},.Invalid_Shader }
    vertex,err:=compile_function(r,desc.vertex_metal_source,desc.vertex_metal_entry,.Vertex)
    if err!=.None { return {},err }; defer vertex->release()
    fragment:^NS.Object
    defer { if fragment!=nil { fragment->release() } }
    if len(desc.fragment_entry)>0 {
        fragment,err=compile_function(r,desc.fragment_metal_source,desc.fragment_metal_entry,.Fragment)
        if err!=.None { return {},err }
    }
    descriptor:=new_object("MTL4RenderPipelineDescriptor")
    if descriptor==nil { return {},.Allocation_Failed }; defer descriptor->release()
    options:=new_object("MTL4PipelineOptions")
    if options==nil { return {},.Allocation_Failed }; defer options->release()
    send(nil,options,"setShaderReflection:",NS.UInteger(3)); send(nil,descriptor,"setOptions:",options)
    send(nil,descriptor,"setVertexFunctionDescriptor:",vertex); send(nil,descriptor,"setFragmentFunctionDescriptor:",fragment)
    send(nil,descriptor,"setRasterSampleCount:",NS.UInteger(1))
    if len(desc.vertex.buffers)>0 {
        native_vertex,vertex_error:=vertex_descriptor(desc.vertex); if vertex_error!=.None { return {},vertex_error }
        send(nil,descriptor,"setVertexDescriptor:",native_vertex); native_vertex->release()
    }
    attachments:=send(^NS.Object,descriptor,"colorAttachments")
    for color,i in desc.colors {
        if gfx.texture_aspects(color.format)!={.Color} { return {},.Invalid_Shader }
        attachment:=send(^NS.Object,attachments,"objectAtIndexedSubscript:",NS.UInteger(i))
        send(nil,attachment,"setPixelFormat:",pixel_format(color.format))
        send(nil,attachment,"setBlendingState:",NS.Integer(1) if color.blend_enabled else NS.Integer(0))
        send(nil,attachment,"setSourceRGBBlendFactor:",blend_factor(color.source_color)); send(nil,attachment,"setDestinationRGBBlendFactor:",blend_factor(color.destination_color)); send(nil,attachment,"setRgbBlendOperation:",blend_op(color.color_op))
        send(nil,attachment,"setSourceAlphaBlendFactor:",blend_factor(color.source_alpha)); send(nil,attachment,"setDestinationAlphaBlendFactor:",blend_factor(color.destination_alpha)); send(nil,attachment,"setAlphaBlendOperation:",blend_op(color.alpha_op))
        mask:NS.UInteger
        if .Red in color.write_mask { mask|=8 }; if .Green in color.write_mask { mask|=4 }; if .Blue in color.write_mask { mask|=2 }; if .Alpha in color.write_mask { mask|=1 }
        send(nil,attachment,"setWriteMask:",mask)
    }
    native_error:^NS.Error
    object:=send(^NS.Object,r.compiler,"newRenderPipelineStateWithDescriptor:compilerTaskOptions:error:",descriptor,cast(^NS.Object)nil,&native_error)
    if object==nil { report_error(native_error,"Metal graphics pipeline compilation failed"); return {},.Shader_Compile_Failed }
    pipeline:=new(Native_Graphics,r.allocator); pipeline^={object=object,desc=desc,refs=1}
    success:=false; defer { if !success { release_graphics(r,pipeline) } }
    pipeline.desc.buffers=clone_slice(desc.buffers,r); pipeline.desc.images=clone_slice(desc.images,r); pipeline.desc.samplers=clone_slice(desc.samplers,r); pipeline.desc.colors=clone_slice(desc.colors,r)
    pipeline.desc.vertex.attributes=clone_slice(desc.vertex.attributes,r); pipeline.desc.vertex.buffers=clone_slice(desc.vertex.buffers,r)
    pipeline.desc.vertex_metal_entry=""; pipeline.desc.fragment_metal_entry=""
    pipeline.desc.vertex_metal_source=""; pipeline.desc.fragment_metal_source=""; pipeline.desc.vertex_entry=""; pipeline.desc.fragment_entry=""; pipeline.desc.vertex_spirv=nil; pipeline.desc.fragment_spirv=nil
    pipeline.buffers=make([]gfx.Stage_Buffer_Requirement,len(desc.buffers),r.allocator)
    for buffer,i in desc.buffers { pipeline.buffers[i]={group=buffer.group,slot=buffer.slot,stages=buffer.stages,usage=buffer.usage,mode=buffer.mode,minimum_size=buffer.minimum_size} }
    pipeline.images=make([]gfx.Image_Binding_Requirement,len(desc.images),r.allocator)
    for image,i in desc.images { pipeline.images[i]={group=image.group,slot=image.slot,stages=image.stages,usage=image.usage,arrayed=image.arrayed,depth=image.depth,dimension=image.dimension,sample_type=image.sample_type,storage_format=image.storage_format,mode=image.mode,array_count=image.array_count} }
    pipeline.samplers=make([]gfx.Sampler_Requirement,len(desc.samplers),r.allocator)
    for sampler,i in desc.samplers { pipeline.samplers[i]={sampler.group,sampler.slot,sampler.stages,sampler.comparison} }
    pipeline.colors=make([]gfx.Texture_Format,len(desc.colors),r.allocator)
    for color,i in desc.colors { pipeline.colors[i]=color.format }
    reflection:=send(^MTL.RenderPipelineReflection,object,"reflection")
    if reflection==nil { return {},.Invalid_Shader }
    err=reflect_stage(r,pipeline,reflection->vertexBindings(),.Vertex)
    if err!=.None { return {},err }
    if fragment!=nil {
        err=reflect_stage(r,pipeline,reflection->fragmentBindings(),.Fragment)
        if err!=.None { return {},err }
    } else {
        for buffer in desc.buffers { if .Fragment in buffer.stages { return {},.Invalid_Shader } }
        for image in desc.images { if .Fragment in image.stages { return {},.Invalid_Shader } }
        for sampler in desc.samplers { if .Fragment in sampler.stages { return {},.Invalid_Shader } }
    }
    if desc.depth.enabled {
        depth:=MTL.DepthStencilDescriptor.alloc()->init()
        if depth==nil { return {},.Allocation_Failed }; defer depth->release()
        depth->setDepthCompareFunction(compare_op(desc.depth.compare) if desc.depth.test else .Always); depth->setDepthWriteEnabled(NS.BOOL(desc.depth.write))
        if desc.stencil.enabled {
            front:=stencil_descriptor(desc.stencil.front,desc.stencil); if front==nil { return {},.Allocation_Failed }; defer front->release()
            back:=stencil_descriptor(desc.stencil.back,desc.stencil); if back==nil { return {},.Allocation_Failed }; defer back->release()
            depth->setFrontFaceStencil(front); depth->setBackFaceStencil(back)
        }
        pipeline.depth=send(^MTL.DepthStencilState,r.device,"newDepthStencilStateWithDescriptor:",depth)
        if pipeline.depth==nil { return {},.Allocation_Failed }
    }
    success=true
    return gfx.storage_insert(&r.graphics,pipeline),.None
}

/// Registry removal preserves ready graphics state retained by accepted frames.
destroy_graphics_pipeline :: proc(r:^Renderer,handle:gfx.Graphics_Pipeline_Handle)->gfx.Gpu_Error {
    pipeline,ok:=gfx.storage_remove(&r.graphics,handle); if !ok { return .Invalid_Resource }
    release_graphics(r,pipeline); return .None
}

@(private="package")
query_graphics :: proc(state:rawptr,handle:gfx.Graphics_Pipeline_Handle)->(gfx.Graphics_Info,bool) {
    r:=cast(^Renderer)state; pipeline,ok:=gfx.storage_get(&r.graphics,handle)
    if !ok { return {},false }
    return {buffers=pipeline^.buffers,images=pipeline^.images,samplers=pipeline^.samplers,vertex=pipeline^.desc.vertex,supported_draws={.Generated,.Vertices,.Indexed,.Indirect,.Indexed_Indirect},colors=pipeline^.colors,depth=pipeline^.desc.depth,stencil=pipeline^.desc.stencil.enabled},true
}

@(private="package")
query_sampler :: proc(state:rawptr,handle:gfx.Sampler_Handle)->(gfx.Sampler_Info,bool) {
    r:=cast(^Renderer)state; sampler,ok:=gfx.storage_get(&r.samplers,handle)
    if !ok { return {},false }
    return {sampler^.desc,sampler^.object},true
}

/// Supplies mandatory image, pipeline and sampler queries for portable preflight.
graphics_query :: proc(r:^Renderer)->gfx.Graphics_Query { return {r,query_texture,query_graphics,query_sampler} }

@(private="package")
graphics_native_bindings_valid :: proc(desc:gfx.Graphics_Desc)->bool {
    if (desc.vertex_sizes_words==0)!=(desc.vertex_sizes_index<0) || (desc.fragment_sizes_words==0)!=(desc.fragment_sizes_index<0) || desc.vertex_sizes_index>=31 || desc.fragment_sizes_index>=31 || desc.vertex_sizes_words>4096 || desc.fragment_sizes_words>4096 { return false }
    for stage in ([2]gfx.Shader_Stage{.Vertex,.Fragment}) {
        vertex:=stage==.Vertex
        sizes_index:=desc.vertex_sizes_index if vertex else desc.fragment_sizes_index
        words:=desc.vertex_sizes_words if vertex else desc.fragment_sizes_words
        for buffer,i in desc.buffers {
            if !(stage in buffer.stages) { continue }
            index:=buffer.vertex_index if vertex else buffer.fragment_index
            size_index:=buffer.vertex_size_index if vertex else buffer.fragment_size_index
            if index<0 || index>=31 || index==sizes_index || size_index>=i32(words) { return false }
            for previous in desc.buffers[:i] { if stage in previous.stages && index==(previous.vertex_index if vertex else previous.fragment_index) { return false } }
            if vertex { for layout in desc.vertex.buffers { if index==i32(10+layout.binding) || sizes_index==i32(10+layout.binding) { return false } } }
        }
        for image,i in desc.images {
            if !(stage in image.stages) { continue }
            index:=image.vertex_index if vertex else image.fragment_index
            if image.array_count==0 || image.array_count>4096 || (image.metal_kind==.Texture && image.array_count!=1) || index<0 || index>=128 { return false }
            if image.metal_kind==.Argument_Buffer {
                if index>=31 || index==sizes_index { return false }
                for buffer in desc.buffers { if stage in buffer.stages && index==(buffer.vertex_index if vertex else buffer.fragment_index) { return false } }
                if vertex { for layout in desc.vertex.buffers { if index==i32(10+layout.binding) { return false } } }
            }
            for previous in desc.images[:i] { if stage in previous.stages && (previous.metal_kind==.Argument_Buffer)==(image.metal_kind==.Argument_Buffer) && index==(previous.vertex_index if vertex else previous.fragment_index) { return false } }
        }
        for sampler,i in desc.samplers {
            if !(stage in sampler.stages) { continue }
            index:=sampler.vertex_index if vertex else sampler.fragment_index
            if index<0 || index>=16 { return false }
            for previous in desc.samplers[:i] { if stage in previous.stages && index==(previous.vertex_index if vertex else previous.fragment_index) { return false } }
        }
    }
    return true
}
