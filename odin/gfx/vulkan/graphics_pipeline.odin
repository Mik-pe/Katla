//! Vulkan graphics pipelines own reflected layouts and explicit dynamic-rendering targets.
package katla_vulkan

import gfx ".."
import vk "vendor:vulkan"
import "core:strings"

@(private="package")
Native_Graphics_Pipeline :: struct {
    object:vk.Pipeline,
    layout:vk.PipelineLayout,
    set_layouts:[]vk.DescriptorSetLayout,
    buffers:[]gfx.Stage_Buffer_Requirement,
    images:[]gfx.Image_Binding_Requirement,
    samplers:[]gfx.Sampler_Requirement,
    colors:[]gfx.Texture_Format,
    depth:gfx.Depth_State,
    stencil:bool,
    vertex:gfx.Vertex_Layout,
    refs:int,
}
@(private="package")
release_graphics_pipeline :: proc(r:^Renderer,pipeline:^Native_Graphics_Pipeline) {
    pipeline.refs-=1
    if pipeline.refs!=0 { return }
    if pipeline.object!=0 { r.table.DestroyPipeline(r.device,pipeline.object,nil) }
    if pipeline.layout!=0 { r.table.DestroyPipelineLayout(r.device,pipeline.layout,nil) }
    for layout in pipeline.set_layouts { if layout!=0 { r.table.DestroyDescriptorSetLayout(r.device,layout,nil) } }
    delete(pipeline.set_layouts,r.allocator)
    delete(pipeline.vertex.attributes,r.allocator); delete(pipeline.vertex.buffers,r.allocator)
    delete(pipeline.buffers,r.allocator); delete(pipeline.images,r.allocator); delete(pipeline.samplers,r.allocator); delete(pipeline.colors,r.allocator)
    free(pipeline,r.allocator)
}
@(private="package")
shader_stages :: proc(stages:gfx.Shader_Stages)->vk.ShaderStageFlags {
    flags:vk.ShaderStageFlags
    for stage in stages {
        switch stage {
        case .Vertex: flags|={.VERTEX}
        case .Fragment: flags|={.FRAGMENT}
        case .Compute: flags|={.COMPUTE}
        }
    }
    return flags
}
@(private="package")
color_components :: proc(components:gfx.Color_Components)->vk.ColorComponentFlags {
    flags:vk.ColorComponentFlags
    for component in components {
        switch component {
        case .Red: flags|={.R}
        case .Green: flags|={.G}
        case .Blue: flags|={.B}
        case .Alpha: flags|={.A}
        }
    }
    return flags
}
@(private="package")
shader_module :: proc(r:^Renderer,words:[]u32)->(vk.ShaderModule,gfx.Gpu_Error) {
    if len(words)<5 || words[0]!=0x07230203 { return 0,.Invalid_Shader }
    module:vk.ShaderModule
    info:=vk.ShaderModuleCreateInfo{sType=.SHADER_MODULE_CREATE_INFO,codeSize=len(words)*4,pCode=raw_data(words)}
    if r.table.CreateShaderModule(r.device,&info,nil,&module)!=.SUCCESS { return 0,.Shader_Compile_Failed }
    return module,.None
}
@(private="package")
graphics_descriptors_allocate :: proc(r:^Renderer,desc:gfx.Graphics_Desc,pipeline:^Native_Graphics_Pipeline)->gfx.Gpu_Error {
    limits_error:=image_descriptor_limits(r,desc)
    if limits_error!=.None { return limits_error }
    max_group:int=-1
    for binding in desc.buffers { max_group=max(max_group,int(binding.group)) }
    for binding in desc.images { max_group=max(max_group,int(binding.group)) }
    for binding in desc.samplers { max_group=max(max_group,int(binding.group)) }
    if max_group>=int(r.limits.maxBoundDescriptorSets) || max_group>=32 || len(desc.buffers)+len(desc.images)+len(desc.samplers)>128 { return .Invalid_Shader }
    pipeline.set_layouts=make([]vk.DescriptorSetLayout,max_group+1,r.allocator)
    for _,group in pipeline.set_layouts {
        bindings:[128]vk.DescriptorSetLayoutBinding
        count:int
        flags:[128]vk.DescriptorBindingFlags
        update:bool
        for binding in desc.buffers {
            if int(binding.group)!=group { continue }
            descriptor:=vk.DescriptorType.STORAGE_BUFFER if binding.usage==.Storage else vk.DescriptorType.UNIFORM_BUFFER
            bindings[count]={binding=binding.slot,descriptorType=descriptor,descriptorCount=1,stageFlags=shader_stages(binding.stages)}; count+=1
        }
        for binding in desc.images {
            if int(binding.group)!=group { continue }
            descriptor:=vk.DescriptorType.STORAGE_IMAGE if binding.usage==.Storage else vk.DescriptorType.SAMPLED_IMAGE
            if binding.array_count==0 { return .Invalid_Shader }
            if binding.array_count>1 {
                if (binding.usage==.Sampled && !r.sampled_arrays) || (binding.usage==.Storage && !r.storage_arrays) { return .Unsupported }
                flags[count]={.UPDATE_AFTER_BIND}; update=true
            }
            bindings[count]={binding=binding.slot,descriptorType=descriptor,descriptorCount=binding.array_count,stageFlags=shader_stages(binding.stages)}; count+=1
        }
        for binding in desc.samplers {
            if int(binding.group)!=group { continue }
            bindings[count]={binding=binding.slot,descriptorType=.SAMPLER,descriptorCount=1,stageFlags=shader_stages(binding.stages)}; count+=1
        }
        for binding,i in bindings[:count] { for previous in bindings[:i] { if binding.binding==previous.binding { return .Invalid_Shader } } }
        info:=vk.DescriptorSetLayoutCreateInfo{sType=.DESCRIPTOR_SET_LAYOUT_CREATE_INFO,bindingCount=u32(count),pBindings=raw_data(bindings[:count])}
        binding_flags:=vk.DescriptorSetLayoutBindingFlagsCreateInfo{sType=.DESCRIPTOR_SET_LAYOUT_BINDING_FLAGS_CREATE_INFO,bindingCount=u32(count),pBindingFlags=raw_data(flags[:])}
        if update { info.flags={.UPDATE_AFTER_BIND_POOL}; info.pNext=&binding_flags }
        if r.table.CreateDescriptorSetLayout(r.device,&info,nil,&pipeline.set_layouts[group])!=.SUCCESS { return .Allocation_Failed }
    }
    layout_info:=vk.PipelineLayoutCreateInfo{sType=.PIPELINE_LAYOUT_CREATE_INFO,setLayoutCount=u32(len(pipeline.set_layouts)),pSetLayouts=raw_data(pipeline.set_layouts)}
    if r.table.CreatePipelineLayout(r.device,&layout_info,nil,&pipeline.layout)!=.SUCCESS { return .Allocation_Failed }
    return .None
}
@(private="package")
graphics_pipeline_allocate :: proc(r:^Renderer,desc:gfx.Graphics_Desc,pipeline:^Native_Graphics_Pipeline)->gfx.Gpu_Error {
    descriptor_error:=graphics_descriptors_allocate(r,desc,pipeline)
    if descriptor_error!=.None { return descriptor_error }
    vertex,vertex_error:=shader_module(r,desc.vertex_spirv)
    if vertex_error!=.None { return vertex_error }
    defer r.table.DestroyShaderModule(r.device,vertex,nil)
    fragment:vk.ShaderModule
    defer { if fragment!=0 { r.table.DestroyShaderModule(r.device,fragment,nil) } }
    if desc.fragment_entry!="" {
        error:gfx.Gpu_Error
        fragment,error=shader_module(r,desc.fragment_spirv)
        if error!=.None { return error }
    }
    vertex_name:=strings.clone_to_cstring(desc.vertex_entry,r.allocator); defer delete(vertex_name,r.allocator)
    fragment_name:=strings.clone_to_cstring(desc.fragment_entry,r.allocator); defer delete(fragment_name,r.allocator)
    stages:=[2]vk.PipelineShaderStageCreateInfo{
        {sType=.PIPELINE_SHADER_STAGE_CREATE_INFO,stage={.VERTEX},module=vertex,pName=vertex_name},
        {sType=.PIPELINE_SHADER_STAGE_CREATE_INFO,stage={.FRAGMENT},module=fragment,pName=fragment_name},
    }
    attributes:[32]vk.VertexInputAttributeDescription
    vertex_bindings:[16]vk.VertexInputBindingDescription
    for attribute,i in desc.vertex.attributes { attributes[i]={attribute.location,attribute.binding,vertex_format(attribute.format),attribute.offset} }
    for binding,i in desc.vertex.buffers { vertex_bindings[i]={binding.binding,binding.stride,.INSTANCE if binding.step==.Instance else .VERTEX} }
    vertex_input:=vk.PipelineVertexInputStateCreateInfo{sType=.PIPELINE_VERTEX_INPUT_STATE_CREATE_INFO,vertexBindingDescriptionCount=u32(len(desc.vertex.buffers)),pVertexBindingDescriptions=raw_data(vertex_bindings[:]),vertexAttributeDescriptionCount=u32(len(desc.vertex.attributes)),pVertexAttributeDescriptions=raw_data(attributes[:])}
    topology:vk.PrimitiveTopology
    switch desc.topology {
    case .Triangle_List: topology=.TRIANGLE_LIST
    case .Triangle_Strip: topology=.TRIANGLE_STRIP
    case .Line_List: topology=.LINE_LIST
    case .Point_List: topology=.POINT_LIST
    }
    assembly:=vk.PipelineInputAssemblyStateCreateInfo{sType=.PIPELINE_INPUT_ASSEMBLY_STATE_CREATE_INFO,topology=topology}
    viewport:=vk.PipelineViewportStateCreateInfo{sType=.PIPELINE_VIEWPORT_STATE_CREATE_INFO,viewportCount=1,scissorCount=1}
    cull:vk.CullModeFlags
    switch desc.cull {
    case .None: cull={}
    case .Front: cull={.FRONT}
    case .Back: cull={.BACK}
    }
    raster:=vk.PipelineRasterizationStateCreateInfo{sType=.PIPELINE_RASTERIZATION_STATE_CREATE_INFO,polygonMode=.LINE if desc.wireframe else .FILL,depthBiasEnable=b32(desc.depth_bias.constant!=0 || desc.depth_bias.slope!=0 || desc.depth_bias.clamp!=0),depthBiasConstantFactor=desc.depth_bias.constant,depthBiasSlopeFactor=desc.depth_bias.slope,depthBiasClamp=desc.depth_bias.clamp,cullMode=cull,frontFace=.COUNTER_CLOCKWISE if desc.front_counter_clockwise else .CLOCKWISE,lineWidth=1}
    multisample:=vk.PipelineMultisampleStateCreateInfo{sType=.PIPELINE_MULTISAMPLE_STATE_CREATE_INFO,rasterizationSamples={._1}}
    depth:=vk.PipelineDepthStencilStateCreateInfo{sType=.PIPELINE_DEPTH_STENCIL_STATE_CREATE_INFO,depthTestEnable=b32(desc.depth.test || desc.depth.write),depthWriteEnable=b32(desc.depth.write),depthCompareOp=vk.CompareOp(desc.depth.compare) if desc.depth.test else vk.CompareOp.ALWAYS,stencilTestEnable=b32(desc.stencil.enabled),front=stencil_face(desc.stencil.front,desc.stencil),back=stencil_face(desc.stencil.back,desc.stencil)}
    blends:[8]vk.PipelineColorBlendAttachmentState
    formats:[8]vk.Format
    if len(desc.colors)>len(blends) || len(desc.colors)>int(r.limits.maxColorAttachments) { return .Unsupported }
    for color,i in desc.colors {
        blends[i]={blendEnable=b32(color.blend_enabled),srcColorBlendFactor=vk.BlendFactor(color.source_color),dstColorBlendFactor=vk.BlendFactor(color.destination_color),colorBlendOp=vk.BlendOp(color.color_op),srcAlphaBlendFactor=vk.BlendFactor(color.source_alpha),dstAlphaBlendFactor=vk.BlendFactor(color.destination_alpha),alphaBlendOp=vk.BlendOp(color.alpha_op),colorWriteMask=color_components(color.write_mask)}
        formats[i]=texture_format(color.format)
    }
    blend:=vk.PipelineColorBlendStateCreateInfo{sType=.PIPELINE_COLOR_BLEND_STATE_CREATE_INFO,attachmentCount=u32(len(desc.colors)),pAttachments=raw_data(blends[:])}
    dynamic_states:=[2]vk.DynamicState{.VIEWPORT,.SCISSOR}
    dynamic_info:=vk.PipelineDynamicStateCreateInfo{sType=.PIPELINE_DYNAMIC_STATE_CREATE_INFO,dynamicStateCount=2,pDynamicStates=raw_data(dynamic_states[:])}
    rendering:=vk.PipelineRenderingCreateInfo{sType=.PIPELINE_RENDERING_CREATE_INFO,colorAttachmentCount=u32(len(desc.colors)),pColorAttachmentFormats=raw_data(formats[:]),depthAttachmentFormat=texture_format(desc.depth.format) if desc.depth.enabled else .UNDEFINED}
    if desc.depth.enabled && .Stencil in gfx.texture_aspects(desc.depth.format) { rendering.stencilAttachmentFormat=rendering.depthAttachmentFormat }
    info:=vk.GraphicsPipelineCreateInfo{sType=.GRAPHICS_PIPELINE_CREATE_INFO,pNext=&rendering,stageCount=2 if fragment!=0 else 1,pStages=raw_data(stages[:]),pVertexInputState=&vertex_input,pInputAssemblyState=&assembly,pViewportState=&viewport,pRasterizationState=&raster,pMultisampleState=&multisample,pDepthStencilState=&depth,pColorBlendState=&blend,pDynamicState=&dynamic_info,layout=pipeline.layout,basePipelineIndex=-1}
    if r.table.CreateGraphicsPipelines(r.device,0,1,&info,nil,&pipeline.object)!=.SUCCESS { return .Shader_Compile_Failed }
    return .None
}
/// Removes a graphics handle while accepted commands retain the pipeline and layouts.
destroy_graphics_pipeline :: proc(r:^Renderer,handle:gfx.Graphics_Pipeline_Handle)->gfx.Gpu_Error {
    pipeline,ok:=gfx.storage_remove(&r.graphics,handle)
    if !ok { return .Invalid_Resource }
    release_graphics_pipeline(r,pipeline)
    return .None
}
@(private="package")
query_graphics :: proc(state:rawptr,handle:gfx.Graphics_Pipeline_Handle)->(gfx.Graphics_Info,bool) {
    r:=cast(^Renderer)state
    entry,ok:=gfx.storage_get(&r.graphics,handle)
    if !ok { return {},false }
    p:=entry^
    draws:gfx.Draw_Kinds={.Generated,.Vertices,.Indexed}
    if r.indirect_first_instance { draws|={.Indirect,.Indexed_Indirect} }
    return {buffers=p.buffers,images=p.images,samplers=p.samplers,vertex=p.vertex,colors=p.colors,depth=p.depth,stencil=p.stencil,supported_draws=draws},true
}
/// Supplies native texture identities and reflected graphics contracts before recording.
graphics_query :: proc(r:^Renderer)->gfx.Graphics_Query { return {r,query_texture,query_graphics,query_sampler} }

@(private="package")
vertex_format :: proc(format:gfx.Vertex_Format)->vk.Format {
    switch format {
    case .Float: return .R32_SFLOAT
    case .Float2: return .R32G32_SFLOAT
    case .Float3: return .R32G32B32_SFLOAT
    case .Float4: return .R32G32B32A32_SFLOAT
    case .Uint: return .R32_UINT
    case .Uint2: return .R32G32_UINT
    case .Uint3: return .R32G32B32_UINT
    case .Uint4: return .R32G32B32A32_UINT
    case .Sint: return .R32_SINT
    case .Sint2: return .R32G32_SINT
    case .Sint3: return .R32G32B32_SINT
    case .Sint4: return .R32G32B32A32_SINT
    case .Unorm8x4: return .R8G8B8A8_UNORM
    }
    return .UNDEFINED
}
@(private="package")
stencil_face :: proc(face:gfx.Stencil_Face,state:gfx.Stencil_State)->vk.StencilOpState {
    return {vk.StencilOp(face.fail),vk.StencilOp(face.pass),vk.StencilOp(face.depth_fail),vk.CompareOp(face.compare),state.read_mask,state.write_mask,state.reference}
}
