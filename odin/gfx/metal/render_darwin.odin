#+build darwin, arm64
//! Authored attachment operations and stage tables encode into Metal 4 render passes.
package metal

import gfx ".."
import MTL "vendor:darwin/Metal"
import NS "core:sys/darwin/Foundation"
import "core:mem"

@(private="package")
resolve_texture :: proc(r:^Renderer,prepared:^gfx.Prepared_Graph,id:gfx.Image_Id)->(^Native_Texture,bool) {
    handle,found:=gfx.prepared_texture(prepared,id); if !found { return nil,false }
    texture,ok:=gfx.storage_get(&r.textures,handle); if !ok { return nil,false }
    return texture^,true
}

@(private="package")
primitive :: proc(topology:gfx.Primitive_Topology)->MTL.PrimitiveType {
    switch topology {
    case .Triangle_List: return .Triangle
    case .Triangle_Strip: return .TriangleStrip
    case .Line_List: return .Line
    case .Point_List: return .Point
    }
    unreachable()
}

@(private="package")
load_op :: proc(op:gfx.Load_Op)->MTL.LoadAction {
    switch op {
    case .Load: return .Load
    case .Clear: return .Clear
    case .Discard: return .DontCare
    }
    unreachable()
}

@(private="package")
configure_attachment :: proc(r:^Renderer,prepared:^gfx.Prepared_Graph,native:^NS.Object,access:gfx.Image_Access,load:gfx.Load_Op,store:gfx.Store_Op)->gfx.Gpu_Error {
    texture,ok:=resolve_texture(r,prepared,access.resource); if !ok { return .Invalid_Resource }
    send(nil,native,"setTexture:",texture.object)
    send(nil,native,"setLevel:",NS.UInteger(access.range.base_mip)); send(nil,native,"setSlice:",NS.UInteger(access.range.base_layer))
    send(nil,native,"setLoadAction:",load_op(load)); send(nil,native,"setStoreAction:",MTL.StoreAction.Store if store==.Store else MTL.StoreAction.DontCare)
    return .None
}

@(private="package")
render_table :: proc(r:^Renderer,slot:^Native_Frame,pipeline:^Native_Graphics,prepared:^gfx.Prepared_Graph,packet:gfx.Render,constants:[]gfx.Constant_Binding,samplers:[]gfx.Sampler_Binding,images:[]gfx.Image_Binding,stage:gfx.Shader_Stage)->(^NS.Object,gfx.Gpu_Error) {
    vertex:=stage==.Vertex
    slot.capture_pipeline=gfx.capture_object(&r.capture,pipeline.object);slot.capture_layout=gfx.capture_object(&r.capture,&pipeline.desc)
    buffer_count,texture_count,sampler_count:u32
    if vertex { for binding in pipeline.desc.vertex.buffers { buffer_count=max(buffer_count,11+binding.binding) } }
    for binding in pipeline.desc.buffers { if stage in binding.stages { buffer_count=max(buffer_count,u32(binding.vertex_index if vertex else binding.fragment_index)+1) } }
    for binding in pipeline.desc.images { if stage in binding.stages {
        index:=u32(binding.vertex_index if vertex else binding.fragment_index)+1
        if binding.metal_kind==.Argument_Buffer { buffer_count=max(buffer_count,index) } else { texture_count=max(texture_count,index) }
    } }
    for binding in pipeline.desc.samplers { if stage in binding.stages { sampler_count=max(sampler_count,u32(binding.vertex_index if vertex else binding.fragment_index)+1) } }
    sizes_index:=pipeline.desc.vertex_sizes_index if vertex else pipeline.desc.fragment_sizes_index
    sizes_words:=pipeline.desc.vertex_sizes_words if vertex else pipeline.desc.fragment_sizes_words
    if sizes_words>0 { buffer_count=max(buffer_count,u32(sizes_index)+1) }
    if buffer_count>31 || texture_count>128 || sampler_count>16 { return nil,.Invalid_Shader }
    descriptor:=new_object("MTL4ArgumentTableDescriptor")
    if descriptor==nil { return nil,.Allocation_Failed }; defer descriptor->release()
    send(nil,descriptor,"setMaxBufferBindCount:",NS.UInteger(buffer_count)); send(nil,descriptor,"setMaxTextureBindCount:",NS.UInteger(texture_count)); send(nil,descriptor,"setMaxSamplerStateBindCount:",NS.UInteger(sampler_count))
    native_error:^NS.Error
    table:=send(^NS.Object,r.device,"newArgumentTableWithDescriptor:error:",descriptor,&native_error)
    if table==nil { report_error(native_error,"Metal graphics argument table allocation failed"); return nil,.Allocation_Failed }
    append(&slot.tables,table)
    capture_table(r,slot,table,stage)
    sizes:[]u32
    if sizes_words>0 { sizes=make([]u32,int(sizes_words),r.allocator) }; defer delete(sizes,r.allocator)
    for binding in packet.buffers {
        if !(stage in binding.stages) { continue }
        for requirement in pipeline.desc.buffers {
            if requirement.group!=binding.group || requirement.slot!=binding.slot || !(stage in requirement.stages) { continue }
            buffer,ok:=resolve_buffer(r,prepared,binding.access.resource); if !ok { return nil,.Invalid_Resource }
            index:=requirement.vertex_index if vertex else requirement.fragment_index
            if index<0 || index>=31 { return nil,.Invalid_Shader }
            send(nil,table,"setAddress:atIndex:",buffer.object->gpuAddress()+binding.access.range.offset,NS.UInteger(index))
            capture_buffer_binding(r,slot,table,cast(^NS.Object)buffer.object,binding.access,binding.group,binding.slot,u32(index))
            size_index:=requirement.vertex_size_index if vertex else requirement.fragment_size_index
            if size_index>=0 {
                if size_index>=i32(len(sizes)) || binding.access.range.size>u64(max(u32)) { return nil,.Invalid_Range }
                sizes[size_index]=u32(binding.access.range.size)
            }
        }
    }
    for constant in constants {
        if !(stage in constant.stages) { continue }
        for requirement in pipeline.desc.buffers {
            if requirement.group!=constant.group || requirement.slot!=constant.slot || !(stage in requirement.stages) { continue }
            index:=requirement.vertex_index if vertex else requirement.fragment_index
            if index<0 || index>=31 || len(constant.bytes)==0 { return nil,.Invalid_Shader }
            object:=r.device->newBufferWithLength(NS.UInteger(len(constant.bytes)),MTL.ResourceOptions{.HazardTrackingModeUntracked})
            if object==nil { return nil,.Allocation_Failed }
            copy(object->contents(),constant.bytes)
            append(&slot.auxiliary,cast(^NS.Object)object); capture_residency(r,slot,cast(^NS.Object)object)
            send(nil,table,"setAddress:atIndex:",object->gpuAddress(),NS.UInteger(index))
            capture_constant_binding(r,slot,table,cast(^NS.Object)object,constant.group,constant.slot,u32(index),u64(len(constant.bytes)),"immutable inline constants")
            size_index:=requirement.vertex_size_index if vertex else requirement.fragment_size_index
            if size_index>=0 {
                if size_index>=i32(len(sizes)) || u64(len(constant.bytes))>u64(max(u32)) { return nil,.Invalid_Range }
                sizes[size_index]=u32(len(constant.bytes))
            }
        }
    }
    for binding in images {
        if !(stage in binding.stages) { continue }
        for requirement in pipeline.desc.images {
            if requirement.group!=binding.group || requirement.slot!=binding.slot || !(stage in requirement.stages) { continue }
            index:=requirement.vertex_index if vertex else requirement.fragment_index
            err:=encode_image_binding(r,slot,prepared,table,binding,index,requirement.array_count,requirement.metal_kind,requirement.dimension,requirement.arrayed)
            if err!=.None { return nil,err }
        }
    }
    for binding in samplers {
        if !(stage in binding.stages) { continue }
        for requirement in pipeline.desc.samplers {
            if requirement.group!=binding.group || requirement.slot!=binding.slot || !(stage in requirement.stages) { continue }
            entry,ok:=gfx.storage_get(&r.samplers,binding.handle); if !ok { return nil,.Invalid_Resource }
            sampler:=entry^
            retained:=false
            for previous in slot.samplers { if previous==sampler { retained=true; break } }
            if !retained { sampler.refs+=1; append(&slot.samplers,sampler) }
            index:=requirement.vertex_index if vertex else requirement.fragment_index
            if index<0 || index>=16 { return nil,.Invalid_Shader }
            send(nil,table,"setSamplerState:atIndex:",sampler.object->gpuResourceID(),NS.UInteger(index))
            capture_sampler_binding(r,slot,table,sampler.object,binding.group,binding.slot,u32(index))
        }
    }
    if sizes_words>0 {
        object:=r.device->newBufferWithLength(NS.UInteger(sizes_words)*4,MTL.ResourceOptions{.HazardTrackingModeUntracked})
        if object==nil { return nil,.Allocation_Failed }
        copy(object->contents(),mem.slice_to_bytes(sizes))
        append(&slot.auxiliary,cast(^NS.Object)object)
        capture_residency(r,slot,cast(^NS.Object)object)
        send(nil,table,"setAddress:atIndex:",object->gpuAddress(),NS.UInteger(sizes_index))
        capture_constant_binding(r,slot,table,cast(^NS.Object)object,0,0,u32(sizes_index),u64(sizes_words)*4,"runtime-array sizes")
    }
    return table,.None
}

@(private="package")
encode_render :: proc(r:^Renderer,slot:^Native_Frame,prepared:^gfx.Prepared_Graph,pass:gfx.Prepared_Pass,packet:gfx.Render)->gfx.Gpu_Error {
    descriptor:=new_object("MTL4RenderPassDescriptor")
    if descriptor==nil { return .Allocation_Failed }; defer descriptor->release()
    colors:=send(^NS.Object,descriptor,"colorAttachments")
    width,height:u32
    for attachment,i in packet.colors {
        native:=send(^NS.Object,colors,"objectAtIndexedSubscript:",NS.UInteger(i))
        err:=configure_attachment(r,prepared,native,attachment.access,attachment.load,attachment.store); if err!=.None { return err }
        if attachment.load==.Clear { send(nil,native,"setClearColor:",MTL.ClearColor{attachment.clear[0],attachment.clear[1],attachment.clear[2],attachment.clear[3]}) }
        capture_attachment(r,slot,prepared,attachment.access,attachment.load,attachment.store,u32(i),{f64(attachment.clear[0]),f64(attachment.clear[1]),f64(attachment.clear[2]),f64(attachment.clear[3])},0,0,"color attachment")
        texture,_:=resolve_texture(r,prepared,attachment.access.resource); width,height=gfx.texture_mip_extent(texture.desc,attachment.access.range.base_mip)
    }
    if packet.depth.enabled {
        attachment:=packet.depth
        texture,_:=resolve_texture(r,prepared,attachment.access.resource)
        if .Depth in attachment.access.range.aspects {
            native:=send(^NS.Object,descriptor,"depthAttachment")
            err:=configure_attachment(r,prepared,native,attachment.access,attachment.load,attachment.store); if err!=.None { return err }
            if attachment.load==.Clear { send(nil,native,"setClearDepth:",attachment.clear_depth) }
            depth_access:=attachment.access;depth_access.range.aspects={.Depth}
            capture_attachment(r,slot,prepared,depth_access,attachment.load,attachment.store,0,{},attachment.clear_depth,0,"depth attachment")
        }
        if .Stencil in attachment.access.range.aspects {
            stencil:=send(^NS.Object,descriptor,"stencilAttachment")
            err:=configure_attachment(r,prepared,stencil,attachment.access,attachment.load,attachment.store); if err!=.None { return err }
            if attachment.load==.Clear { send(nil,stencil,"setClearStencil:",attachment.clear_stencil) }
            stencil_access:=attachment.access;stencil_access.range.aspects={.Stencil}
            capture_attachment(r,slot,prepared,stencil_access,attachment.load,attachment.store,1,{},0,attachment.clear_stencil,"stencil attachment")
        }
        width,height=gfx.texture_mip_extent(texture.desc,attachment.access.range.base_mip)
    }
    encoder:=send(^NS.Object,slot.command,"renderCommandEncoderWithDescriptor:",descriptor)
    if encoder==nil { return .Allocation_Failed }
    capture_encoder_begin(r,slot,encoder,"MTL4 render encoder")
    defer { send(nil,encoder,"endEncoding");capture_encoder_end(r,slot) }
    visibility:=NS.UInteger(1)
    for buffer in slot.buffers { if buffer.heap!=nil { visibility|=2 } }
    for texture in slot.textures { if texture.heap!=nil { visibility|=2 } }
    for alias in prepared.aliases { if alias.after==pass.id { visibility|=2 } }
    send(nil,encoder,"barrierAfterQueueStages:beforeStages:visibilityOptions:",NS.UInteger(max(int)),NS.UInteger(max(int)),visibility)
    capture_barrier(r,slot,visibility,prepared)
    for phase,phase_index in packet.phases {
        slot.capture_phase=phase_index
        if len(phase.draws)==0 { continue }
        entry,ok:=gfx.storage_get(&r.graphics,phase.pipeline); if !ok { return .Invalid_Resource }
        pipeline:=entry^
        retained:=false
        for previous in slot.graphics { if previous==pipeline { retained=true; break } }
        if !retained { pipeline.refs+=1; append(&slot.graphics,pipeline) }
        vertex,err:=render_table(r,slot,pipeline,prepared,packet,phase.constants,phase.samplers,phase.images,.Vertex); if err!=.None { return err }
        fragment,fragment_error:=render_table(r,slot,pipeline,prepared,packet,phase.constants,phase.samplers,phase.images,.Fragment); if fragment_error!=.None { return fragment_error }
        send(nil,encoder,"setRenderPipelineState:",pipeline.object)
        capture_emit(r,slot,{kind=.Bind_Pipeline,resource_index= -1,object=gfx.capture_object(&r.capture,pipeline.object),binding_stages=3,emitted=true,label="render pipeline state"})
        send(nil,encoder,"setArgumentTable:atStages:",vertex,NS.UInteger(1)); send(nil,encoder,"setArgumentTable:atStages:",fragment,NS.UInteger(2))
        send(nil,encoder,"setDepthStencilState:",pipeline.depth)
        if pipeline.desc.stencil.enabled { send(nil,encoder,"setStencilReferenceValue:",pipeline.desc.stencil.reference) }
        send(nil,encoder,"setDepthBias:slopeScale:clamp:",pipeline.desc.depth_bias.constant,pipeline.desc.depth_bias.slope,pipeline.desc.depth_bias.clamp)
        send(nil,encoder,"setTriangleFillMode:",MTL.TriangleFillMode.Lines if pipeline.desc.wireframe else MTL.TriangleFillMode.Fill)
        send(nil,encoder,"setCullMode:",MTL.CullMode.None if pipeline.desc.cull==.None else (MTL.CullMode.Front if pipeline.desc.cull==.Front else MTL.CullMode.Back))
        send(nil,encoder,"setFrontFacingWinding:",MTL.Winding.CounterClockwise if pipeline.desc.front_counter_clockwise else MTL.Winding.Clockwise)
        viewport:=MTL.Viewport{0,0,f64(width),f64(height),0,1}
        if phase.viewport.enabled { viewport={phase.viewport.x,phase.viewport.y,phase.viewport.width,phase.viewport.height,phase.viewport.min_depth,phase.viewport.max_depth} }
        scissor:=MTL.ScissorRect{0,0,NS.Integer(width),NS.Integer(height)}
        if phase.scissor.enabled { scissor={NS.Integer(phase.scissor.x),NS.Integer(phase.scissor.y),NS.Integer(phase.scissor.width),NS.Integer(phase.scissor.height)} }
        send(nil,encoder,"setViewport:",viewport); send(nil,encoder,"setScissorRect:",scissor)
        for operation in phase.draws {
            draw_error:=encode_draw(r,slot,prepared,pipeline,packet,phase,encoder,operation)
            if draw_error!=.None { return draw_error }
        }
    }
    slot.capture_phase= -1;slot.capture_stages=0
    return .None
}

@(private="package")
encode_image_copy :: proc(r:^Renderer,slot:^Native_Frame,encoder:^NS.Object,prepared:^gfx.Prepared_Graph,packet:gfx.Copy_Image_Buffer)->gfx.Gpu_Error {
    source,source_ok:=resolve_texture(r,prepared,packet.source)
    destination,destination_ok:=resolve_buffer(r,prepared,packet.destination)
    if !source_ok || !destination_ok { return .Invalid_Resource }
    layout,valid:=gfx.image_region_layout(packet.region,source.desc); if !valid { return .Invalid_Range }
    bytes_per_row:=NS.UInteger(layout.bytes_per_row)
    send(nil,encoder,"copyFromTexture:sourceSlice:sourceLevel:sourceOrigin:sourceSize:toBuffer:destinationOffset:destinationBytesPerRow:destinationBytesPerImage:options:",source.object,NS.UInteger(packet.region.layer),NS.UInteger(packet.region.mip),MTL.Origin{NS.Integer(packet.region.x),NS.Integer(packet.region.y),NS.Integer(packet.region.z)},MTL.Size{NS.Integer(packet.region.width),NS.Integer(packet.region.height),NS.Integer(packet.region.depth)},destination.object,NS.UInteger(packet.destination_offset),bytes_per_row,NS.UInteger(layout.bytes_per_image),image_copy_options(source.desc.format,packet.region.aspect))
    region:=packet.region;region.bytes_per_row=layout.bytes_per_row;region.bytes_per_image=layout.bytes_per_image
    capture_transfer_image(r,slot,cast(^NS.Object)source.object,packet.source,{region.mip,1,region.layer,1,{region.aspect}},0,region)
    capture_transfer_rows(r,slot,cast(^NS.Object)destination.object,packet.destination,packet.destination_offset,1,region,layout)
    return .None
}

@(private="package")
image_copy_options :: proc(format:gfx.Texture_Format,aspect:gfx.Image_Aspect)->NS.UInteger {
    if format==.D32_Float_S8_Uint || format==.D24_Unorm_S8_Uint {
        if aspect==.Depth { return 1 }
        if aspect==.Stencil { return 2 }
    }
    return 0
}

@(private="package")
encode_buffer_image_copy :: proc(r:^Renderer,slot:^Native_Frame,encoder:^NS.Object,prepared:^gfx.Prepared_Graph,packet:gfx.Copy_Buffer_Image)->gfx.Gpu_Error {
    source,source_ok:=resolve_buffer(r,prepared,packet.source)
    destination,destination_ok:=resolve_texture(r,prepared,packet.destination)
    if !source_ok || !destination_ok { return .Invalid_Resource }
    layout,valid:=gfx.image_region_layout(packet.region,destination.desc); if !valid { return .Invalid_Range }
    row:=NS.UInteger(layout.bytes_per_row)
    send(nil,encoder,"copyFromBuffer:sourceOffset:sourceBytesPerRow:sourceBytesPerImage:sourceSize:toTexture:destinationSlice:destinationLevel:destinationOrigin:options:",source.object,NS.UInteger(packet.source_offset),row,NS.UInteger(layout.bytes_per_image),MTL.Size{NS.Integer(packet.region.width),NS.Integer(packet.region.height),NS.Integer(packet.region.depth)},destination.object,NS.UInteger(packet.region.layer),NS.UInteger(packet.region.mip),MTL.Origin{NS.Integer(packet.region.x),NS.Integer(packet.region.y),NS.Integer(packet.region.z)},image_copy_options(destination.desc.format,packet.region.aspect))
    region:=packet.region;region.bytes_per_row=layout.bytes_per_row;region.bytes_per_image=layout.bytes_per_image
    capture_transfer_rows(r,slot,cast(^NS.Object)source.object,packet.source,packet.source_offset,0,region,layout)
    capture_transfer_image(r,slot,cast(^NS.Object)destination.object,packet.destination,{region.mip,1,region.layer,1,{region.aspect}},1,region)
    return .None
}

@(private="package")
encode_mips :: proc(r:^Renderer,slot:^Native_Frame,prepared:^gfx.Prepared_Graph,encoder:^NS.Object,packet:gfx.Generate_Mips)->gfx.Gpu_Error {
    texture,ok:=resolve_texture(r,prepared,packet.resource); if !ok { return .Invalid_Resource }
    if !gfx.texture_filterable_mips(texture.desc.format) { return .Unsupported }
    object:=texture.object
    if packet.range!=gfx.image_full_range(texture.desc) {
        view:=send(^MTL.Texture,object,"newTextureViewWithPixelFormat:textureType:levels:slices:",pixel_format(texture.desc.format),object->textureType(),NS.Range{NS.UInteger(packet.range.base_mip),NS.UInteger(packet.range.mip_count)},NS.Range{NS.UInteger(packet.range.base_layer),NS.UInteger(packet.range.layer_count)})
        if view==nil { return .Invalid_Range }
        append(&slot.auxiliary,cast(^NS.Object)view); capture_residency(r,slot,cast(^NS.Object)view);capture_auxiliary(r,slot,cast(^NS.Object)view,"mipmap texture view");object=view
    }
    send(nil,encoder,"generateMipmapsForTexture:",object)
    base:=packet.range;base.mip_count=1
    lower:=packet.range;lower.base_mip+=1;lower.mip_count-=1
    capture_transfer_image(r,slot,cast(^NS.Object)object,packet.resource,base,0)
    capture_transfer_image(r,slot,cast(^NS.Object)object,packet.resource,lower,1)
    return .None
}
