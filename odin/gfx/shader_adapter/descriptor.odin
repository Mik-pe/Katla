//! Canonical WGSL artifacts adapt once to backend-neutral native pipeline inputs.
package shader_adapter

import gfx ".."
import shader "../shader"
import "core:mem"

/// Application-authored raster state remains independent of compiler ownership.
Graphics_State :: struct {
    vertex:gfx.Vertex_Layout,
    stencil:gfx.Stencil_State,
    depth_bias:gfx.Depth_Bias,
    wireframe:bool,
    colors:[]gfx.Color_Target,
    depth:gfx.Depth_State,
    topology:gfx.Primitive_Topology,
    cull:gfx.Cull_Mode,
    front_counter_clockwise:bool,
}
/// Owns descriptor arrays; source binaries and raster-state arrays remain borrowed until native preparation.
Graphics :: struct { descriptor:gfx.Graphics_Desc, allocator:mem.Allocator }
/// Owns compute descriptor arrays while borrowing the immutable compiled entry.
Compute :: struct { descriptor:gfx.Compute_Desc, allocator:mem.Allocator }
/// Native descriptor shape rejection is explicit and preserves the current published pipeline.
Error :: enum { None, Missing_Entry, Invalid_Interface, Unsupported_Array, Unsupported_Image, Invalid_Vertex_Layout }
@(private="package")
access_mode :: proc(access:shader.Access)->gfx.Access_Mode {
    switch access {
    case .None: return .None
    case .Read: return .Read
    case .Write: return .Write
    case .Read_Write: return .Read_Write
    }
    return .None
}
@(private="package")
merge_mode :: proc(a,b:gfx.Access_Mode)->gfx.Access_Mode {
    if a==.None { return b }
    if b==.None || a==b { return a }
    return .Read_Write
}
@(private="package")
image_type :: proc(binding:shader.Binding)->(gfx.Texture_Sample_Type,gfx.Texture_Format,Error) {
    if binding.dimension not_in (bit_set[shader.Dimension]{.D2,.D3}) || binding.multisampled || (binding.dimension==.D3 && binding.arrayed) { return {},{},.Unsupported_Image }
    sample:gfx.Texture_Sample_Type
    #partial switch binding.sample_type {
    case .Float: sample=.Float
    case .Uint: sample=.Uint
    case .Sint: sample=.Sint
    case: return {},{},.Invalid_Interface
    }
    format:gfx.Texture_Format
    if binding.storage_format!="" {
        switch binding.storage_format {
        case "Rgba8Unorm": format=.RGBA8_Unorm
        case "Rgba16Float": format=.RGBA16_Float
        case "Rgba16Unorm": format=.RGBA16_Unorm
        case "R32Uint": format=.R32_Uint
        case "R32Float": format=.R32_Float
        case: return {},{},.Unsupported_Image
        }
    }
    return sample,format,.None
}
@(private="package")
binding_native_valid :: proc(binding:shader.Binding)->Error {
    if binding.array_count==0 { return .Invalid_Interface }
    if binding.kind!=.Texture {
        if binding.array_count!=1 { return .Unsupported_Array }
        if binding.metal_kind!=binding.kind || binding.metal_minimum_size!=binding.minimum_size { return .Invalid_Interface }
        return .None
    }
    if binding.metal_kind==.Texture {
        if binding.array_count!=1 || binding.metal_minimum_size!=0 { return .Invalid_Interface }
        return .None
    }
    if binding.metal_kind!=.Buffer || binding.metal_minimum_size!=u64(binding.array_count)*8 { return .Invalid_Interface }
    return .None
}
/// Releases only adapter-owned arrays; native preparation must finish before their owners die.
graphics_destroy :: proc(value:^Graphics) {
    delete(value.descriptor.buffers,value.allocator); delete(value.descriptor.images,value.allocator); delete(value.descriptor.samplers,value.allocator)
    value^={}
}
/// Releases only compute mapping arrays; compiled binaries retain their separate owner.
compute_destroy :: proc(value:^Compute) {
    delete(value.descriptor.buffers,value.allocator); delete(value.descriptor.images,value.allocator); delete(value.descriptor.samplers,value.allocator)
    value^={}
}
/// Maps every selected compute binding and its exact native runtime-array ABI.
compute :: proc(compiled:^shader.Compiled,name:string,allocator:=context.allocator)->(Compute,Error) {
    entry,present:=shader.find_entry(compiled,name,.Compute)
    if !present { return {},.Missing_Entry }
    result:=Compute{allocator=allocator}
    success:=false; defer { if !success { compute_destroy(&result) } }
    buffer_count,image_count,sampler_count:int
    for binding in entry.bindings {
        if native_error:=binding_native_valid(binding); native_error!=.None { return {},native_error }
        switch binding.kind {
        case .Buffer: buffer_count+=1
        case .Texture: image_count+=1
        case .Sampler: sampler_count+=1
        }
    }
    desc:=&result.descriptor
    desc^={entry=entry.name,metal_entry=entry.metal_name,metal_source=entry.metal_source,spirv=entry.spirv,local_size=entry.workgroup_size,runtime_sizes_index=entry.sizes_buffer,runtime_sizes_words=entry.sizes_word_count}
    desc.buffers=make([]gfx.Shader_Buffer,buffer_count,allocator)
    desc.images=make([]gfx.Shader_Compute_Image,image_count,allocator)
    desc.samplers=make([]gfx.Shader_Compute_Sampler,sampler_count,allocator)
    b,i,s:int
    for binding in entry.bindings {
        switch binding.kind {
        case .Buffer:
            desc.buffers[b]={group=binding.group,slot=binding.binding,metal_index=i32(binding.metal_index),size_index=binding.size_index,usage=.Uniform if binding.uniform else .Storage,mode=access_mode(binding.access),minimum_size=binding.minimum_size}; b+=1
        case .Texture:
            sample,format,err:=image_type(binding); if err!=.None { return {},err }
            desc.images[i]={group=binding.group,slot=binding.binding,metal_index=i32(binding.metal_index),usage=.Sampled if binding.storage_format=="" else .Storage,arrayed=binding.arrayed,depth=binding.depth,sample_type=sample,storage_format=format,mode=access_mode(binding.access),dimension=.D3 if binding.dimension==.D3 else .D2,array_count=binding.array_count,metal_kind=.Argument_Buffer if binding.metal_kind==.Buffer else .Texture}; i+=1
        case .Sampler:
            desc.samplers[s]={group=binding.group,slot=binding.binding,metal_index=i32(binding.metal_index),comparison=binding.comparison}; s+=1
        }
    }
    success=true; return result,.None
}
@(private="package")
vertex_inputs_valid :: proc(entry:^shader.Entry,layout:gfx.Vertex_Layout)->bool {
    if !gfx.vertex_layout_valid(layout) { return false }
    locations:=0
    for input in entry.inputs {
        if input.location<0 { continue }
        locations+=1; found:=false
        for attribute in layout.attributes {
            if attribute.location!=u32(input.location) { continue }; found=true
            size:=gfx.vertex_format_size(attribute.format)
            components:=size/4
            if attribute.format==.Unorm8x4 { components=4 }
            if components!=u32(input.components) || input.width!=4 { return false }
            kind:=shader.Scalar.Float
            if attribute.format in (bit_set[gfx.Vertex_Format]{.Uint,.Uint2,.Uint3,.Uint4}) { kind=.Uint }
            else if attribute.format in (bit_set[gfx.Vertex_Format]{.Sint,.Sint2,.Sint3,.Sint4}) { kind=.Sint }
            if kind!=input.scalar { return false }
        }
        if !found { return false }
    }
    return locations==len(layout.attributes)
}
@(private="package")
stage_interface_valid :: proc(vertex,fragment:^shader.Entry,colors:[]gfx.Color_Target)->bool {
    for input in fragment.inputs {
        if input.location<0 { continue }
        found:=false
        for output in vertex.outputs {
            if output.location!=input.location { continue }
            if output.scalar!=input.scalar || output.width!=input.width || output.components!=input.components || output.interpolation!=input.interpolation || output.per_primitive!=input.per_primitive { return false }
            found=true; break
        }
        if !found { return false }
    }
    for output in fragment.outputs {
        if output.location<0 { continue }
        if int(output.location)>=len(colors) || output.blend_source>0 || output.width!=4 { return false }
        #partial switch colors[output.location].format {
        case .RGBA8_Unorm,.BGRA8_Unorm,.RGBA16_Float,.RGBA16_Unorm,.RGBA8_Srgb,.BGRA8_Srgb,.R8_Unorm,.RG8_Unorm,.R32_Float: if output.scalar!=.Float { return false }
        case .R32_Uint: if output.scalar!=.Uint { return false }
        case: return false
        }
    }
    return true
}
/// Merges exact selected-stage logical resources while preserving each native argument index.
graphics :: proc(compiled:^shader.Compiled,vertex_name,fragment_name:string,state:Graphics_State,allocator:=context.allocator)->(Graphics,Error) {
    vertex,has_vertex:=shader.find_entry(compiled,vertex_name,.Vertex)
    if !has_vertex { return {},.Missing_Entry }
    fragment:^shader.Entry
    if fragment_name!="" { present:bool; fragment,present=shader.find_entry(compiled,fragment_name,.Fragment); if !present { return {},.Missing_Entry } }
    if !vertex_inputs_valid(vertex,state.vertex) { return {},.Invalid_Vertex_Layout }
    if fragment==nil && len(state.colors)>0 { return {},.Invalid_Interface }
    if fragment!=nil && !stage_interface_valid(vertex,fragment,state.colors) { return {},.Invalid_Interface }
    result:=Graphics{allocator=allocator}
    success:=false; defer { if !success { graphics_destroy(&result) } }
    buffers:=make([dynamic]gfx.Shader_Stage_Buffer,allocator); images:=make([dynamic]gfx.Shader_Stage_Image,allocator); samplers:=make([dynamic]gfx.Shader_Stage_Sampler,allocator)
    defer delete(buffers); defer delete(images); defer delete(samplers)
    for entry in ([2]^shader.Entry{vertex,fragment}) {
        if entry==nil { continue }
        stage:=gfx.Shader_Stage.Vertex if entry.stage==.Vertex else gfx.Shader_Stage.Fragment
        for binding in entry.bindings {
            if native_error:=binding_native_valid(binding); native_error!=.None { return {},native_error }
            switch binding.kind {
            case .Buffer:
                index:= -1
                for previous,j in buffers { if previous.group==binding.group && previous.slot==binding.binding { index=j; break } }
                usage:=gfx.Buffer_Usage.Uniform if binding.uniform else gfx.Buffer_Usage.Storage
                if index<0 { index=len(buffers); append(&buffers,gfx.Shader_Stage_Buffer{group=binding.group,slot=binding.binding,usage=usage,vertex_index= -1,fragment_index= -1,vertex_size_index= -1,fragment_size_index= -1,mode=access_mode(binding.access),minimum_size=binding.minimum_size}) }
                buffer:=&buffers[index]; if buffer.usage!=usage { return {},.Invalid_Interface }
                buffer.mode=merge_mode(buffer.mode,access_mode(binding.access)); buffer.minimum_size=max(buffer.minimum_size,binding.minimum_size)
                buffer.stages+={stage}
                if stage==.Vertex { buffer.vertex_index=i32(binding.metal_index); buffer.vertex_size_index=binding.size_index }
                else { buffer.fragment_index=i32(binding.metal_index); buffer.fragment_size_index=binding.size_index }
            case .Texture:
                sample,format,err:=image_type(binding); if err!=.None { return {},err }
                index:= -1
                for previous,j in images { if previous.group==binding.group && previous.slot==binding.binding { index=j; break } }
                usage:=gfx.Texture_Usage.Sampled if binding.storage_format=="" else gfx.Texture_Usage.Storage
                dimension:=gfx.Texture_Dimension.D3 if binding.dimension==.D3 else gfx.Texture_Dimension.D2
                if index<0 { index=len(images); append(&images,gfx.Shader_Stage_Image{group=binding.group,slot=binding.binding,usage=usage,vertex_index= -1,fragment_index= -1,arrayed=binding.arrayed,depth=binding.depth,sample_type=sample,storage_format=format,mode=access_mode(binding.access),dimension=dimension,array_count=binding.array_count,metal_kind=.Argument_Buffer if binding.metal_kind==.Buffer else .Texture}) }
                image:=&images[index]
                if image.usage!=usage || image.arrayed!=binding.arrayed || image.depth!=binding.depth || image.sample_type!=sample || image.storage_format!=format || image.dimension!=dimension || image.array_count!=binding.array_count || image.metal_kind!=(gfx.Metal_Image_Binding.Argument_Buffer if binding.metal_kind==.Buffer else gfx.Metal_Image_Binding.Texture) { return {},.Invalid_Interface }
                image.mode=merge_mode(image.mode,access_mode(binding.access))
                image.stages+={stage}
                if stage==.Vertex { image.vertex_index=i32(binding.metal_index) }
                else { image.fragment_index=i32(binding.metal_index) }
            case .Sampler:
                index:= -1
                for previous,j in samplers { if previous.group==binding.group && previous.slot==binding.binding { index=j; break } }
                if index<0 { index=len(samplers); append(&samplers,gfx.Shader_Stage_Sampler{group=binding.group,slot=binding.binding,vertex_index= -1,fragment_index= -1,comparison=binding.comparison}) }
                sampler:=&samplers[index]; if sampler.comparison!=binding.comparison { return {},.Invalid_Interface }
                sampler.stages+={stage}
                if stage==.Vertex { sampler.vertex_index=i32(binding.metal_index) }
                else { sampler.fragment_index=i32(binding.metal_index) }
            }
        }
    }
    desc:=&result.descriptor
    desc^={vertex_entry=vertex.name,vertex_metal_entry=vertex.metal_name,vertex_metal_source=vertex.metal_source,vertex_spirv=vertex.spirv,vertex_sizes_index=vertex.sizes_buffer,vertex_sizes_words=vertex.sizes_word_count,fragment_sizes_index= -1,vertex=state.vertex,stencil=state.stencil,depth_bias=state.depth_bias,wireframe=state.wireframe,colors=state.colors,depth=state.depth,topology=state.topology,cull=state.cull,front_counter_clockwise=state.front_counter_clockwise}
    if fragment!=nil { desc.fragment_entry=fragment.name; desc.fragment_metal_entry=fragment.metal_name; desc.fragment_metal_source=fragment.metal_source; desc.fragment_spirv=fragment.spirv; desc.fragment_sizes_index=fragment.sizes_buffer; desc.fragment_sizes_words=fragment.sizes_word_count }
    desc.buffers=make([]gfx.Shader_Stage_Buffer,len(buffers),allocator); copy(desc.buffers,buffers[:])
    desc.images=make([]gfx.Shader_Stage_Image,len(images),allocator); copy(desc.images,images[:])
    desc.samplers=make([]gfx.Shader_Stage_Sampler,len(samplers),allocator); copy(desc.samplers,samplers[:])
    success=true; return result,.None
}
